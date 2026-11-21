use super::{Failure, Reply, invalid, response::answer};
use crate::agenthub::{Actor, Board, mcp::SEAT_HEADER};
use crate::{
    AppState,
    identity::{Subject, accounts::api_keys::KeyLimits},
    mcp::Hub,
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};
use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Serialize;
use serde_json::{Value, json};

pub(super) struct Caller {
    pub hub: Hub,
    pub subject: Subject,
    pub headers: HeaderMap,
    limits: Option<KeyLimits>,
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = Failure;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Failure> {
        Ok(Self {
            hub: state.hub.clone(),
            subject: parts.extensions.get::<Subject>().cloned().ok_or_else(|| {
                Failure(BridgeError::new(
                    "unauthenticated",
                    "sign in to use this board",
                ))
            })?,
            headers: parts.headers.clone(),
            limits: parts.extensions.get::<KeyLimits>().cloned(),
        })
    }
}

impl Caller {
    pub fn seat(&self) -> Result<Option<String>, Failure> {
        self.headers
            .get(SEAT_HEADER)
            .map(|value| {
                value
                    .to_str()
                    .ok()
                    .filter(|seat| crate::config::valid_name(seat))
                    .map(str::to_string)
                    .ok_or_else(|| invalid("X-Devagotchi-Seat must name a valid seat"))
            })
            .transpose()
    }

    /// One authenticated board operation; the MCP face uses this same grant/profile path.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<T, F>(
        &self,
        tool: &'static str,
        operation: Option<&'static str>,
        project: &str,
        ticket: Option<&str>,
        mut input: Value,
        f: F,
    ) -> Reply
    where
        T: Serialize + Send + 'static,
        F: FnOnce(&Board, &Actor<'_>) -> BridgeResult<T> + Send + 'static,
    {
        let principal = self
            .subject
            .agent
            .as_ref()
            .unwrap_or(&self.subject.id)
            .clone();
        let key = input
            .as_object_mut()
            .and_then(|body| body.remove("request_id"))
            .and_then(|value| value.as_str().map(str::to_string));
        let refusal = self.hub.waiting_refusal(&self.subject).or_else(|| {
            self.limits
                .as_ref()
                .and_then(|limits| limits.check(tool, json!({"project":project}).as_object()))
                .map(|why| BridgeError::new("key_denied", why))
        });
        let result = if let Some(error) = refusal {
            Err(error)
        } else {
            match self.seat() {
                Err(error) => Err(error.0),
                Ok(seat) => {
                    self.hub
                        .run_scoped(
                            &self.subject,
                            seat,
                            tool,
                            project,
                            None,
                            move |board, actor| {
                                let requested = match (operation, key.as_deref()) {
                                    (Some(operation), Some(key)) => {
                                        Some(board.requested(&principal, key, operation, &input)?)
                                    }
                                    _ => None,
                                };
                                let board = requested.as_ref().unwrap_or(board);
                                let result = f(board, actor)?;
                                let replayed = board.hosted().replayed();
                                let mut value = serde_json::to_value(result).map_err(|_| {
                                    BridgeError::new("board_store", "cannot encode board response")
                                })?;
                                if operation.is_some() {
                                    if !value.is_object() {
                                        value = json!({"result":value});
                                    }
                                    value["replayed"] = replayed.into();
                                    if let Some(key) = key {
                                        value["request_id"] = key.into();
                                    }
                                }
                                Ok((value, replayed))
                            },
                        )
                        .await
                }
            }
        };
        let outcome = result
            .as_ref()
            .err()
            .map(|error| error.code.as_str())
            .unwrap_or("ok");
        self.hub
            .record_completion(
                &self.subject,
                tool,
                project,
                ticket.unwrap_or_default(),
                outcome,
            )
            .await;
        result
            .map(|(value, replayed)| answer(value, replayed))
            .map_err(Failure)
    }
}
