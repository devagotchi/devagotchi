//! Closed operations carried by the shared, private preparation and selection owner.
use super::*;

#[derive(Debug, Clone)]
pub(super) enum PreparedInput {
    Start(StartInput),
    Files(FileInvocation),
}
impl PreparedInput {
    pub(super) fn key(&self) -> &str {
        match self {
            Self::Start(input) => &input.idempotency_key,
            Self::Files(input) => &input.key,
        }
    }
    pub(super) fn action(&self) -> GrantAction {
        match self {
            Self::Start(input) => {
                if input.interactive.is_some() {
                    GrantAction::ProcessStart
                } else {
                    GrantAction::Exec
                }
            }
            Self::Files(input) => input.action(),
        }
    }
    pub(super) fn capabilities(&self) -> RemoteRequest {
        match self {
            Self::Start(_) => RemoteRequest::Capabilities {},
            Self::Files(_) => RemoteRequest::FileCapabilities {},
        }
    }
    pub(super) fn prepare(
        &self,
        origin: GrantOrigin,
        actor: GrantActor,
        level: GrantLevel,
    ) -> RemoteRequest {
        match self {
            Self::Start(input) => RemoteRequest::Prepare {
                origin,
                actor,
                level,
                request: RequestBody::OperationStart(input.clone()),
            },
            Self::Files(invocation) => RemoteRequest::PrepareFiles {
                origin,
                actor,
                level,
                invocation: invocation.clone(),
            },
        }
    }
}
