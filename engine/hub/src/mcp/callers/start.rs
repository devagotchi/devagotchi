//! Enrolled account-backed starts use source grants and fresh request credentials.
use super::*;
use crate::{
    identity::{accounts::api_keys::Scope, bearer::BearerVerifier},
    machines::access::{Access, LinkExecution, StartSelection},
    mcp::tools::AccessCaller,
};
use devagotchi_contracts::StartInput;

impl Hub {
    pub(crate) async fn send_for_request(
        &self,
        tool: &str,
        route: &Route,
        body: RequestBody,
        context: &RequestContext<RoleServer>,
    ) -> Result<ResponseBody, BridgeError> {
        let account_backed = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|p| p.extensions.get::<BearerVerifier>())
            .is_some_and(|v| v.accounts.is_some());
        match (route, body) {
            (
                route,
                body @ (RequestBody::FileRead(_)
                | RequestBody::FileList(_)
                | RequestBody::FileInfo(_)
                | RequestBody::FileSearch(_)
                | RequestBody::FileWrite(_)
                | RequestBody::FileEdit(_)
                | RequestBody::FileMkdir(_)
                | RequestBody::FileMove(_)
                | RequestBody::FileDelete(_)),
            ) if account_backed => match route {
                Route::Link(machine) => self.files_granted(machine, body, context).await,
                Route::Local(_) => Err(BridgeError::new(
                    "not_granted",
                    "account file access requires an enrolled broker and current grant",
                )),
            },
            (Route::Link(machine), RequestBody::OperationStart(input)) if account_backed => self
                .start_granted(machine, input, context)
                .await
                .map(ResponseBody::Start),
            (Route::Local(_), RequestBody::OperationStart(_)) if account_backed => {
                Err(BridgeError::new(
                    "not_granted",
                    "account machine execution requires an enrolled broker and current access grant; configured local IPC is not an account grant",
                ))
            }
            (
                route,
                body @ (RequestBody::OperationGet(_)
                | RequestBody::OperationRead(_)
                | RequestBody::OperationWait(_)),
            ) if account_backed => match route {
                Route::Link(machine) => {
                    self.read_granted(
                        machine,
                        body,
                        context,
                        if tool == "run" {
                            Scope::MachinesRun
                        } else {
                            Scope::MachinesRead
                        },
                    )
                    .await
                }
                Route::Local(_) => Err(BridgeError::new(
                    "not_granted",
                    "account operation reads require the original enrolled admission",
                )),
            },
            (
                route,
                body @ (RequestBody::OperationInput(_)
                | RequestBody::OperationResize(_)
                | RequestBody::OperationCancel(_)),
            ) if account_backed => match route {
                Route::Link(machine) => self.control_granted(machine, body, context).await,
                Route::Local(_) => Err(BridgeError::new(
                    "not_granted",
                    "account process control requires the original enrolled admission",
                )),
            },
            (_, body) => self.send_hinted(route, body).await,
        }
    }

    async fn start_granted(
        &self,
        machine: &str,
        input: StartInput,
        context: &RequestContext<RoleServer>,
    ) -> Result<devagotchi_contracts::StartOutput, BridgeError> {
        let mut caller = AccessCaller::new(context).await?;
        caller.scope(Scope::MachinesRun)?;
        let access = Access::new(caller.accounts.clone());
        let binding = access
            .request_machine(&caller.subject.id, &caller.actor(), machine)
            .await?;
        let client = Self::caller(&caller.subject, context).client;
        let prepared = access
            .prepare_selected_start(
                &StartSelection {
                    account: &caller.subject.id,
                    actor: &caller.actor(),
                    machine: &binding,
                    links: &self.links,
                    client: &client,
                    boards: Some(&self.boards),
                    projects: caller.allowed_projects(),
                },
                input,
            )
            .await?;
        let project = prepared.project().map(str::to_owned);
        let grant = prepared.grant_id().to_owned();
        let shown = self.shown_name(&caller.subject, machine);
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
        // Both broker preparation and board resolution can wait. Never consume a grant
        // or dispatch using the credential/profile snapshot from before those waits.
        caller.refresh().await?;
        caller.scope(Scope::MachinesRun)?;
        caller.request_scope(&shown, project.as_deref())?;
        let (_, output) = access
            .execute_prepared(
                &LinkExecution {
                    account: &caller.subject.id,
                    actor: &caller.actor(),
                    board: board.as_ref().map(|b| b.hosted()),
                    links: &self.links,
                    grant_id: &grant,
                    client: &client,
                },
                prepared,
            )
            .await?;
        Ok(output)
    }
}
