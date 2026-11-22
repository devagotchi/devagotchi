//! Observe the original admission under current permission, without renewing execution.
use super::*;
use crate::{
    identity::accounts::api_keys::Scope, machines::access::Access, mcp::tools::AccessCaller,
};
use devagotchi_contracts::{RemoteReply, RemoteRequest};

impl Hub {
    pub(super) async fn read_granted(
        &self,
        machine: &str,
        request: RequestBody,
        context: &RequestContext<RoleServer>,
        scope: Scope,
    ) -> Result<ResponseBody, BridgeError> {
        let output = matches!(&request, RequestBody::OperationRead(_));
        let id = request.target();
        let mut caller = AccessCaller::new(context).await?;
        caller.scope(scope)?;
        let access = Access::new(caller.accounts.clone());
        let machine = access
            .request_machine(&caller.subject.id, &caller.actor(), machine)
            .await?;
        let target = access
            .operation_read_target(
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
        let waiting = match &request {
            RequestBody::OperationWait(input) => {
                if !(1..=devagotchi_contracts::MAX_WAIT_MS).contains(&input.timeout_ms) {
                    return Err(BridgeError::new(
                        "invalid_request",
                        "timeout_ms is outside its supported range",
                    ));
                }
                Some(input.timeout_ms)
            }
            _ => None,
        };
        let began = tokio::time::Instant::now();
        loop {
            caller.refresh().await?;
            caller.scope(scope)?;
            caller.request_scope(&shown, project.as_deref())?;
            let observation = access
                .operation_observation(
                    &caller.subject.id,
                    &caller.actor(),
                    board.as_ref().map(|b| b.hosted()),
                    &target,
                )
                .await?;
            let mut part = request.clone();
            if let RequestBody::OperationWait(input) = &mut part {
                input.timeout_ms = input
                    .timeout_ms
                    .saturating_sub(began.elapsed().as_millis() as u64)
                    .max(1);
            }
            let response = self
                .links
                .call_remote(
                    &machine.name,
                    RemoteRequest::Observe {
                        observation,
                        client: Self::caller(&caller.subject, context).client,
                        request: part,
                    },
                )
                .await?;
            // Recheck after every bounded broker wait before disclosing any result.
            caller.refresh().await?;
            caller.scope(scope)?;
            caller.request_scope(&shown, project.as_deref())?;
            access
                .operation_observation(
                    &caller.subject.id,
                    &caller.actor(),
                    board.as_ref().map(|b| b.hosted()),
                    &target,
                )
                .await?;
            let (response, ready) = match response {
                RemoteReply::Observed {
                    response: ResponseBody::Output(page),
                    ready,
                } if output && page.operation_id == id && page.operation.operation_id == id => {
                    (ResponseBody::Output(page), ready)
                }
                RemoteReply::Observed {
                    response: ResponseBody::Operation(view),
                    ready,
                } if !output && view.operation_id == id => (ResponseBody::Operation(view), ready),
                _ => {
                    return Err(BridgeError::new(
                        "invalid_response",
                        "broker did not return this admission's requested result",
                    ));
                }
            };
            if !ready
                && waiting.is_some_and(|limit| began.elapsed().as_millis() < u128::from(limit))
            {
                continue;
            }
            return Ok(response);
        }
    }
}
