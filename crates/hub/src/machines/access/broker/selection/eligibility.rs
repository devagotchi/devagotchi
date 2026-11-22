//! Candidate viability is not admission: no effect, use or lease is created here.
use super::*;
use crate::machines::access::{current::CurrentInput, grant_record::GrantRecord};

impl StartSelection<'_> {
    pub(super) fn project_allowed(&self, project: Option<&str>) -> bool {
        self.projects.is_empty()
            || project.is_some_and(|p| self.projects.iter().any(|allowed| allowed == p))
    }
}
impl Access {
    pub(super) async fn start_candidate_live(
        &self,
        selection: &StartSelection<'_>,
        id: &str,
        target: &ExecutionTarget,
        action: GrantAction,
    ) -> BridgeResult<bool> {
        if !selection.project_allowed(target.project.as_deref()) {
            return Ok(false);
        }
        let board = if let Some(project) = &target.project {
            let boards = selection
                .boards
                .ok_or_else(|| {
                    BridgeError::new(
                        "not_offered",
                        "task grant selection requires its board registry",
                    )
                })?
                .clone();
            let account = selection.account.to_owned();
            let project = project.clone();
            let board =
                tokio::task::spawn_blocking(move || boards.open_account(&account, &project))
                    .await
                    .map_err(|_| {
                        BridgeError::new("board_store", "task board resolution failed")
                    })??;
            let Some(board) = board else { return Ok(false) };
            Some(board)
        } else {
            None
        };
        let result: BridgeResult<bool> = transaction!(&self.accounts.store, |c| {
            let found = self
                .accounts
                .catalog_actor_in(c, selection.account, false)
                .await?;
            let grant = GrantRecord::load(c, &found.tenant, id).await?;
            if let (Some(task), Some(board)) = (&grant.task, &board) {
                let board = board.hosted();
                if task.tenant != board.tenant() {
                    return Err(BridgeError::new(
                        "authority_mismatch",
                        "task grant belongs to another environment",
                    )
                    .into());
                }
                if task.board != board.board_id() {
                    return Ok(false);
                }
            }
            let current = self
                .current_in(
                    c,
                    selection.account,
                    selection.actor,
                    board.as_ref().map(|b| b.hosted()),
                    selection.machine,
                    &CurrentInput {
                        grant_id: id,
                        action,
                    },
                )
                .await?;
            Ok(!current
                .grant
                .terms
                .uses_max
                .is_some_and(|limit| current.grant.uses >= i64::from(limit)))
        });
        match result {
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "grant_ended"
                        | "stale_fence"
                        | "not_holder"
                        | "lease_expired"
                        | "policy_denied"
                ) =>
            {
                Ok(false)
            }
            result => result,
        }
    }
}
