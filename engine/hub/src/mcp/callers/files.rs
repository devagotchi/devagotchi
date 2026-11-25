//! One account-backed file intent across ordinary, batch and image tool adapters.
use super::*;
use crate::identity::{accounts::api_keys::Scope, bearer::BearerVerifier};
use crate::machines::access::{Access, Admitted, LinkExecution, PreparedCall, StartSelection};
use crate::mcp::tools::AccessCaller;
use devagotchi_contracts::{FileIntent, FileInvocation, FileOperation, FileStep, GrantAction};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct FileRetryKey(pub Option<String>);

pub(crate) fn account_backed(context: &RequestContext<RoleServer>) -> bool {
    context
        .extensions
        .get::<http::request::Parts>()
        .and_then(|p| p.extensions.get::<BearerVerifier>())
        .is_some_and(|v| v.accounts.is_some())
}
pub(crate) fn read_key() -> Result<String, BridgeError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| BridgeError::new("random", e.to_string()))?;
    Ok(hex::encode(bytes))
}

pub(crate) struct FileSession {
    access: Access,
    prepared: PreparedCall,
    board: Option<Arc<devagotchi_agenthub::Board>>,
    project: Option<String>,
    shown: String,
    grant: String,
    client: String,
    scope: Scope,
    last: tokio::sync::Mutex<Option<Admitted>>,
}
impl FileSession {
    async fn caller(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<AccessCaller, BridgeError> {
        let caller = AccessCaller::new(context).await?;
        caller.scope(self.scope)?;
        caller.request_scope(&self.shown, self.project.as_deref())?;
        Ok(caller)
    }
    fn execution<'a>(
        &'a self,
        hub: &'a Hub,
        caller: &'a AccessCaller,
        actor: &'a devagotchi_agenthub::Actor<'a>,
    ) -> LinkExecution<'a> {
        LinkExecution {
            account: &caller.subject.id,
            actor,
            board: self.board.as_ref().map(|b| b.hosted()),
            links: &hub.links,
            grant_id: &self.grant,
            client: &self.client,
        }
    }
    pub(crate) async fn send(
        &self,
        hub: &Hub,
        context: &RequestContext<RoleServer>,
        step: FileStep,
    ) -> Result<ResponseBody, BridgeError> {
        let mut caller = self.caller(context).await?;
        let (admitted, outcome) = self
            .access
            .execute_prepared_files(
                &self.execution(hub, &caller, &caller.actor()),
                &self.prepared,
                step,
            )
            .await?;
        *self.last.lock().await = Some(admitted);
        caller.refresh().await?;
        caller.scope(self.scope)?;
        caller.request_scope(&self.shown, self.project.as_deref())?;
        self.access
            .check_prepared_files(
                &self.execution(hub, &caller, &caller.actor()),
                &self.prepared,
            )
            .await?;
        outcome.into_result()
    }
    pub(crate) async fn page(
        &self,
        hub: &Hub,
        context: &RequestContext<RoleServer>,
        request: RequestBody,
    ) -> Result<ResponseBody, BridgeError> {
        let RequestBody::FileRead(input) = request else {
            return Err(BridgeError::new(
                "authority_mismatch",
                "composite file reader sent a non-read request",
            ));
        };
        self.send(hub, context, FileStep::Read { input }).await
    }
    pub(crate) async fn finish(
        &self,
        hub: &Hub,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), BridgeError> {
        let admitted = self.last.lock().await.clone();
        let Some(admitted) = admitted else {
            return Ok(());
        };
        let mut caller = self.caller(context).await?;
        self.access
            .finish_prepared_files(
                &self.execution(hub, &caller, &caller.actor()),
                &self.prepared,
                &admitted,
            )
            .await?;
        caller.refresh().await?;
        caller.scope(self.scope)?;
        caller.request_scope(&self.shown, self.project.as_deref())?;
        self.access
            .check_finished_files(
                &self.execution(hub, &caller, &caller.actor()),
                &self.prepared,
                &admitted,
            )
            .await
    }
}

impl Hub {
    pub(crate) async fn file_session_for_route(
        &self,
        route: &Route,
        invocation: FileInvocation,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<FileSession>, BridgeError> {
        if !account_backed(context) {
            return Ok(None);
        }
        match route {
            Route::Link(machine) => self
                .prepare_file_session(machine, invocation, context)
                .await
                .map(Some),
            Route::Local(_) => Err(BridgeError::new(
                "not_granted",
                "account file access requires an enrolled broker and current grant",
            )),
        }
    }
    pub(crate) async fn prepare_file_session(
        &self,
        machine: &str,
        invocation: FileInvocation,
        context: &RequestContext<RoleServer>,
    ) -> Result<FileSession, BridgeError> {
        let mut caller = AccessCaller::new(context).await?;
        let scope = if invocation.action() == GrantAction::FileWrite {
            Scope::FilesWrite
        } else {
            Scope::FilesRead
        };
        caller.scope(scope)?;
        let access = Access::new(caller.accounts.clone());
        let binding = access
            .request_machine(&caller.subject.id, &caller.actor(), machine)
            .await?;
        let client = Self::caller(&caller.subject, context).client;
        let prepared = access
            .prepare_selected_files(
                &StartSelection {
                    account: &caller.subject.id,
                    actor: &caller.actor(),
                    machine: &binding,
                    links: &self.links,
                    client: &client,
                    boards: Some(&self.boards),
                    projects: caller.allowed_projects(),
                },
                invocation,
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
        caller.refresh().await?;
        caller.scope(scope)?;
        caller.request_scope(&shown, project.as_deref())?;
        Ok(FileSession {
            access,
            prepared,
            board,
            project,
            shown,
            grant,
            client,
            scope,
            last: Default::default(),
        })
    }

    pub(super) async fn files_granted(
        &self,
        machine: &str,
        body: RequestBody,
        context: &RequestContext<RoleServer>,
    ) -> Result<ResponseBody, BridgeError> {
        let operation = FileOperation::from_request(body)?;
        let mutation = operation.action() == GrantAction::FileWrite;
        let key = if mutation {
            context.extensions.get::<FileRetryKey>().and_then(|v| v.0.clone())
                .ok_or_else(|| BridgeError::new("retry_key_required", "account file mutations require idempotency_key; reuse it after a lost response"))?
        } else {
            read_key()?
        };
        let session = self
            .prepare_file_session(
                machine,
                FileInvocation {
                    key,
                    intent: FileIntent::Operation { operation },
                },
                context,
            )
            .await?;
        let result = session.send(self, context, FileStep::Operation {}).await;
        if !mutation {
            session.finish(self, context).await?;
        }
        result
    }
}
