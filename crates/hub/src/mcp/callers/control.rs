//! Process effects use current permission on the original admission, never a new grant.
use super::*;
use crate::{
    identity::accounts::api_keys::Scope, machines::access::Access, mcp::tools::AccessCaller,
};
use devagotchi_contracts::{ProcessControl, RemoteReply, RemoteRequest};

impl Hub {
    pub(super) async fn control_granted(
        &self,
        machine: &str,
        request: RequestBody,
        context: &RequestContext<RoleServer>,
    ) -> Result<ResponseBody, BridgeError> {
        let action = match &request {
            RequestBody::OperationInput(_) => ProcessControl::Input,
            RequestBody::OperationResize(_) => ProcessControl::Resize,
            RequestBody::OperationCancel(_) => ProcessControl::Cancel,
            _ => {
                return Err(BridgeError::new(
                    "invalid_request",
                    "request is not process control",
                ));
            }
        };
        let id = request.target();
        let mut caller = AccessCaller::new(context).await?;
        caller.scope(Scope::MachinesRun)?;
        let access = Access::new(caller.accounts.clone());
        let machine = access
            .request_machine(&caller.subject.id, &caller.actor(), machine)
            .await?;
        let target = access
            .operation_target(
                &caller.subject.id,
                &caller.actor(),
                &machine,
                &id,
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
        caller.refresh().await?;
        caller.scope(Scope::MachinesRun)?;
        caller.request_scope(&shown, project.as_deref())?;
        let control = access
            .operation_control(
                &caller.subject.id,
                &caller.actor(),
                board.as_ref().map(|b| b.hosted()),
                &target,
                action,
            )
            .await?;
        let reply = self
            .links
            .call_remote(
                &machine.name,
                RemoteRequest::Control {
                    control,
                    client: Self::caller(&caller.subject, context).client,
                    request,
                },
            )
            .await?;
        // An effect may already have happened; refresh before disclosing its receipt.
        caller.refresh().await?;
        caller.scope(Scope::MachinesRun)?;
        caller.request_scope(&shown, project.as_deref())?;
        match reply {
            RemoteReply::Controlled(ResponseBody::InputSent(sent))
                if action == ProcessControl::Input && sent.operation.operation_id == id =>
            {
                Ok(ResponseBody::InputSent(sent))
            }
            RemoteReply::Controlled(ResponseBody::Operation(view))
                if action != ProcessControl::Input && view.operation_id == id =>
            {
                Ok(ResponseBody::Operation(view))
            }
            _ => Err(BridgeError::new(
                "invalid_response",
                "broker did not acknowledge this admission's requested effect",
            )),
        }
    }
}
