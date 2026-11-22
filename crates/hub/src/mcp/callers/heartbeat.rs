//! Explicit renewal of the original process admission with request-local authority.
use super::*;
use crate::{
    identity::accounts::api_keys::Scope,
    machines::access::{Access, AdmissionMachine},
    mcp::tools::AccessCaller,
};
use devagotchi_contracts::{HeartbeatOutput, REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest};

impl Hub {
    pub(crate) async fn heartbeat_granted(
        &self,
        name: &str,
        operation: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<HeartbeatOutput, BridgeError> {
        let mut caller = AccessCaller::new(context).await?;
        caller.scope(Scope::MachinesRun)?;
        let Route::Link(name) = self.route(&caller.subject, name).await.map_err(|e| e.0)? else {
            return Err(BridgeError::new(
                "not_granted",
                "heartbeat requires the original enrolled broker admission",
            ));
        };
        let access = Access::new(caller.accounts.clone());
        let machine = access
            .request_machine(&caller.subject.id, &caller.actor(), &name)
            .await?;
        let target = access
            .operation_target(
                &caller.subject.id,
                &caller.actor(),
                &machine,
                operation,
                &self.links,
            )
            .await?;
        let project = target.project().map(str::to_owned);
        let shown = self.shown_name(&caller.subject, &machine.name);
        caller.request_scope(&shown, project.as_deref())?;
        let board = if let Some(project) = &project {
            let boards = self.boards.clone();
            let account = caller.subject.id.clone();
            let project = project.clone();
            Some(
                tokio::task::spawn_blocking(move || boards.open_account(&account, &project))
                    .await
                    .map_err(|_| BridgeError::new("board_store", "task board resolution failed"))??
                    .ok_or_else(|| BridgeError::new("not_found", "task project is unavailable"))?,
            )
        } else {
            None
        };
        let levels = match self
            .links
            .call_remote(&machine.name, RemoteRequest::Capabilities {})
            .await?
        {
            RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                sandbox_levels,
                ..
            } => sandbox_levels,
            _ => {
                return Err(BridgeError::new(
                    "invalid_response",
                    "broker did not prove its grant capabilities",
                ));
            }
        };
        // Never send a renewed permission using credentials from before a broker wait.
        caller.refresh().await?;
        caller.scope(Scope::MachinesRun)?;
        caller.request_scope(&shown, project.as_deref())?;
        let continuation = access
            .heartbeat_operation(
                &caller.subject.id,
                &caller.actor(),
                board.as_ref().map(|b| b.hosted()),
                &AdmissionMachine {
                    binding: &machine,
                    sandbox_levels: &levels,
                },
                &target,
            )
            .await?;
        let authority = continuation
            .admitted
            .authority(self.links.grant_origin(&machine))?;
        let expires_at_ms = authority.expires_at;
        let reply = self
            .links
            .call_remote(&machine.name, RemoteRequest::Refresh { authority })
            .await?;
        caller.refresh().await?;
        caller.scope(Scope::MachinesRun)?;
        caller.request_scope(&shown, project.as_deref())?;
        match reply {
            RemoteReply::Refreshed { operation_id }
                if operation_id == continuation.operation_id =>
            {
                Ok(HeartbeatOutput {
                    operation_id,
                    expires_at_ms,
                })
            }
            _ => Err(BridgeError::new(
                "invalid_response",
                "broker did not acknowledge this operation's heartbeat",
            )),
        }
    }
}
