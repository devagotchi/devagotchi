//! Approvals on a board: who decides them, which boards a person approves on, and announcing a decision to the agents that wait.

use std::sync::Arc;

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::{ApprovalOut, Kind};
use crate::{
    agenthub::{Board, approval::ApprovalStatus},
    identity::Subject,
    mcp::Hub,
    tenancy::orgs::Role,
};

impl Hub {
    /// Push "approval needed" to the phones of the people who decide on configured board
    /// `project`: the payload names the approval and links to it, never the command text.
    /// Returns how many phones it reaches (the board tools say whom per kind: `approvers`).
    #[cfg(test)]
    pub async fn announce_approval(&self, project: &str, approval: &ApprovalStatus) -> usize {
        self.announce_approval_to(self.config.people_for_board(project), project, approval)
            .await
    }

    /// Push "approval needed" to these people's phones only (a test goes to the person
    /// who asked for it). Returns how many phones the push goes to.
    pub async fn announce_approval_to(
        &self,
        people: Vec<String>,
        project: &str,
        approval: &ApprovalStatus,
    ) -> usize {
        let Some(push) = self.push.clone() else {
            return 0;
        };
        let phones = push.phones_for(&people).await;
        let payload = serde_json::json!({
            "title": "Approval needed",
            "body": approval.title,
            "url": format!("/approvals#{project}-{}", approval.id),
            "tag": approval.id,
        });
        tokio::spawn(async move { push.notify(&people, &payload).await });
        phones
    }

    /// An approval with the page where a person decides it.
    pub(super) fn approval_out(&self, project: &str, approval: ApprovalStatus) -> ApprovalOut {
        let url = format!(
            "{}/approvals#{project}-{}",
            self.config.public_url.trim_end_matches('/'),
            approval.id
        );
        ApprovalOut {
            approval,
            url,
            phones: None,
            standing: None,
        }
    }

    /// Whether a signed-in person with `role` decides approvals: a member and above. Agents
    /// decide nothing: no board tool decides, and only a browser session reaches here.
    pub(crate) fn person_decides(&self, role: Role) -> bool {
        role.is_person()
    }

    /// Whether the signed-in `subject` is a person who approves on this hub at all: on a
    /// hub that hosts account boards everyone is, on their own; elsewhere the people the
    /// config names are, and anyone the store makes a person on an organisation or a
    /// project.
    pub(crate) fn may_approve(&self, subject: &Subject) -> bool {
        if self.boards.hosts_accounts() || self.config.is_human(&subject.id) {
            return true;
        }
        let Some(orgs) = self.boards.orgs() else {
            return false;
        };
        let in_org = self
            .boards
            .block(orgs.of_subject(&subject.id))
            .is_ok_and(|held| held.iter().any(|(_, role)| role.is_person()));
        in_org
            || self
                .boards
                .block(orgs.projects_of(&subject.id))
                .is_ok_and(|held| held.iter().any(|(_, role)| role.is_person()))
    }

    /// The board named `project` on which the signed-in person `subject` decides
    /// approvals, or why not.
    pub(crate) fn approval_board(
        &self,
        subject: &Subject,
        project: &str,
    ) -> BridgeResult<Arc<Board>> {
        let (board, _, role) = self.board_access(subject, project)?;
        if !self.person_decides(role) {
            return Err(BridgeError::new(
                "humans_only",
                format!("{} is not a person on board {project:?}", subject.id),
            ));
        }
        Ok(board)
    }

    /// Every board on which the signed-in person `subject` decides approvals, by project.
    pub(crate) fn approval_boards(
        &self,
        subject: &Subject,
    ) -> BridgeResult<Vec<(String, Arc<Board>)>> {
        self.board_names(subject)?
            .into_iter()
            .filter_map(|project| match self.approval_board(subject, &project) {
                Ok(board) => Some(Ok((project, board))),
                Err(error) if matches!(error.code.as_str(), "humans_only" | "not_granted") => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    /// The people an approval on `project` goes to: the account that owns the board on
    /// the account's own (or org's) board; the configured people and the store's on a
    /// shared one; the people of an organisation's project.
    pub(super) fn approvers(&self, subject: &Subject, project: &str, kind: Kind) -> Vec<String> {
        let stored = || -> Vec<String> {
            let (Some(orgs), Ok(Some(at))) = (self.boards.orgs(), self.project_ref(project)) else {
                return Vec::new();
            };
            self.boards.block(orgs.people(&at)).unwrap_or_default()
        };
        match kind {
            Kind::Account | Kind::Org => vec![subject.id.clone()],
            Kind::Shared => {
                let mut people = self.config.people_for_board(project);
                for person in stored() {
                    if !people.contains(&person) {
                        people.push(person);
                    }
                }
                people
            }
            Kind::Workspace => stored(),
        }
    }
}
