//! Account project lifecycle shared by HTTP and browser, with existing plan limits.
use crate::{
    agenthub::{
        Actor,
        hosted::{ProjectEdit, ProjectRecord},
    },
    identity::Subject,
    mcp::Hub,
    site::Cap,
};
use devagotchi_contracts::{BridgeError, BridgeResult};

impl Hub {
    fn project_person(&self, subject: &Subject) -> BridgeResult<()> {
        if let Some(error) = self.waiting_refusal(subject) {
            return Err(error);
        }
        if matches!(subject.via, "key" | "hub") || subject.agent.is_some() || subject.org.is_some()
        {
            return Err(BridgeError::new(
                "humans_only",
                "project lifecycle requires the account's person",
            ));
        }
        if !self.boards.hosts_accounts() {
            return Err(BridgeError::new(
                "not_offered",
                "this hub does not host account projects",
            ));
        }
        Ok(())
    }

    pub(crate) async fn owned_projects(
        &self,
        subject: &Subject,
        retired: bool,
        after: Option<&str>,
    ) -> BridgeResult<Vec<ProjectRecord>> {
        self.project_person(subject)?;
        self.boards
            .hosted
            .projects(&subject.id, retired, after)
            .await
    }

    pub(crate) async fn owned_project(
        &self,
        subject: &Subject,
        project: &str,
    ) -> BridgeResult<ProjectRecord> {
        self.project_person(subject)?;
        let (board, _) = self
            .boards
            .hosted
            .open(&subject.id, project, false, None)
            .await?
            .ok_or_else(|| BridgeError::new("not_found", "project does not exist"))?;
        board.hosted().project_record().await
    }

    pub(crate) async fn save_owned_project(
        &self,
        subject: &Subject,
        edit: &ProjectEdit,
        key: Option<&str>,
    ) -> BridgeResult<(ProjectRecord, bool)> {
        self.project_person(subject)?;
        if self
            .config
            .boards_for(&subject.id)
            .iter()
            .any(|board| board.project == edit.id)
        {
            return Err(BridgeError::new(
                "name_taken",
                "a shared board already uses this name",
            ));
        }
        let accounts =
            self.boards.accounts.as_ref().ok_or_else(|| {
                BridgeError::new("not_offered", "account projects are unavailable")
            })?;
        let unlimited = accounts.unlimited.contains(&subject.id)
            || accounts
                .owner
                .get()
                .is_some_and(|owner| owner == &subject.id);
        let limit = if unlimited {
            None
        } else {
            match self.plan_cap(subject, "projects") {
                Some((_, Cap::AtMost(limit))) => Some(limit as usize),
                Some((_, Cap::Unlimited)) => None,
                _ => Some(accounts.free_limit),
            }
        };
        let board = self
            .boards
            .hosted
            .project_candidate(&subject.id, &edit.id)
            .await?;
        let result = board
            .put_owned_project(&Actor::human(&subject.id), edit, key, limit)
            .await;
        self.record_completion(
            subject,
            "project_put",
            &edit.id,
            "",
            result
                .as_ref()
                .err()
                .map_or("ok", |error| error.code.as_str()),
        )
        .await;
        result
    }

    pub(crate) async fn remove_owned_project(
        &self,
        subject: &Subject,
        project: &str,
        version: i64,
        key: Option<&str>,
    ) -> BridgeResult<(ProjectRecord, bool)> {
        self.project_person(subject)?;
        let board = self
            .boards
            .hosted
            .retirement_target(&subject.id, project)
            .await?
            .ok_or_else(|| BridgeError::new("not_found", "project does not exist"))?;
        let result = board
            .retire_owned_project(&Actor::human(&subject.id), version, key)
            .await;
        self.record_completion(
            subject,
            "project_retire",
            project,
            "",
            result
                .as_ref()
                .err()
                .map_or("ok", |error| error.code.as_str()),
        )
        .await;
        result
    }

    pub(crate) async fn owned_project_history(
        &self,
        subject: &Subject,
        id: &str,
        after: i64,
    ) -> BridgeResult<(ProjectRecord, Vec<crate::agenthub::Event>)> {
        self.project_person(subject)?;
        let board = self
            .boards
            .hosted
            .archived_project(&subject.id, id)
            .await?
            .ok_or_else(|| BridgeError::new("not_found", "archived project does not exist"))?;
        board
            .archived_project_events(&Actor::human(&subject.id), after, 100)
            .await
    }
}
