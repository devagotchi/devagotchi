//! Retry identity comes from typed tool input; authority comes from the live subject.
use super::{BoardError, BoardOut, refuse};
use crate::{
    agenthub::{Actor, Board},
    mcp::Hub,
};
use devagotchi_contracts::BridgeResult;
use rmcp::{Json, RoleServer, service::RequestContext};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

pub(super) struct MutationInput {
    key: Option<String>,
    body: Value,
}

impl MutationInput {
    /// A reconnect may have lost a derived fence. A matching receipt can still replay;
    /// a fresh mutation receives an impossible fence and fails its normal lease guard.
    pub(super) fn fence(&self, fence: BridgeResult<i64>) -> BridgeResult<i64> {
        fence.or_else(|error| {
            if self.key.is_some() && error.code == "stale_fence" {
                Ok(i64::MIN)
            } else {
                Err(error)
            }
        })
    }
    pub(super) fn of<T: Serialize>(input: &T) -> Result<Self, Json<BoardError>> {
        let mut body = serde_json::to_value(input).map_err(|_| {
            refuse(
                devagotchi_contracts::BridgeError::new("invalid", "request cannot be encoded"),
                None,
            )
        })?;
        let key = body
            .as_object_mut()
            .and_then(|body| body.remove("request_id"))
            .and_then(|value| value.as_str().map(str::to_owned));
        Ok(Self { key, body })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub(super) struct MutationOut<T> {
    #[serde(flatten)]
    pub result: T,
    /// Original request key, when supplied. Reuse it only for identical input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// True returns the original result. A replay never renews a lease.
    pub replayed: bool,
}

impl<T> MutationOut<T> {
    pub(super) fn map<U>(self, f: impl FnOnce(T) -> U) -> MutationOut<U> {
        MutationOut {
            result: f(self.result),
            request_id: self.request_id,
            replayed: self.replayed,
        }
    }
}

impl Hub {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn board_mutation<T, F>(
        &self,
        tool: &'static str,
        operation: &'static str,
        project: &str,
        ticket: Option<&str>,
        context: &RequestContext<RoleServer>,
        input: MutationInput,
        f: F,
    ) -> BoardOut<MutationOut<T>>
    where
        T: Send + 'static,
        F: FnOnce(&Board, &Actor<'_>) -> BridgeResult<T> + Send + 'static,
    {
        let subject = self.board_subject(context)?;
        let principal = subject.agent.unwrap_or(subject.id);
        self.board_call(tool, project, ticket, context, move |board, actor| {
            let requested = input
                .key
                .as_ref()
                .map(|key| board.requested(&principal, key, operation, &input.body))
                .transpose()?;
            let board = requested.as_ref().unwrap_or(board);
            let result = f(board, actor)?;
            Ok(MutationOut {
                result,
                request_id: input.key,
                replayed: board.hosted().replayed(),
            })
        })
        .await
    }
}
