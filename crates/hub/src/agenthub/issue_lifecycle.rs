//! Signed ingress and authorized projections of the canonical board; no separate task store.

use axum::{
    Extension, Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use devagotchi_contracts::{BridgeError, BridgeResult};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::sync::Arc;

use crate::{
    AppState,
    agenthub::Board,
    auth::Subject,
};

pub use devagotchi_agenthub::issue_lifecycle::*;

const SECRET_ENV: &str = "DEVAGOTCHI_GITHUB_WEBHOOK_SECRET";

pub struct IssueLifecycleStore {
    secret: Option<String>,
}

impl IssueLifecycleStore {
    pub fn from_config(config: &crate::config::GithubConfig) -> BridgeResult<Self> {
        if config.repositories.is_empty() {
            return Ok(Self::disabled());
        }
        let secret = std::env::var(SECRET_ENV).map_err(|_| {
            BridgeError::new(
                "invalid_config",
                format!("{SECRET_ENV} is required when GitHub repositories are configured"),
            )
        })?;
        Self::with_secret(secret)
    }

    pub fn disabled() -> Self {
        Self { secret: None }
    }

    pub(crate) fn with_secret(secret: String) -> BridgeResult<Self> {
        if secret.len() < 32 {
            return Err(BridgeError::new(
                "invalid_config",
                format!("{SECRET_ENV} must contain at least 32 bytes"),
            ));
        }
        Ok(Self {
            secret: Some(secret),
        })
    }

    fn verify(&self, headers: &HeaderMap, body: &[u8]) -> BridgeResult<(String, String)> {
        let secret = self.secret.as_ref().ok_or_else(|| {
            BridgeError::new("not_offered", "GitHub issue ingress is not configured")
        })?;
        if body.len() > MAX_BODY {
            return Err(BridgeError::new(
                "payload_too_large",
                "webhook exceeds 256 KiB",
            ));
        }
        let signature = header(headers, "x-hub-signature-256")?;
        let signature = signature
            .strip_prefix("sha256=")
            .and_then(|hex| hex::decode(hex).ok())
            .filter(|bytes| bytes.len() == 32)
            .ok_or_else(|| BridgeError::new("invalid_signature", "expected a SHA256 signature"))?;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
        mac.update(body);
        mac.verify_slice(&signature).map_err(|_| {
            BridgeError::new("invalid_signature", "webhook signature did not match")
        })?;
        let event = header(headers, "x-github-event")?.to_owned();
        let delivery = header(headers, "x-github-delivery")?;
        if !request_key(delivery) {
            return Err(invalid("invalid x-github-delivery"));
        }
        Ok((event, delivery.to_owned()))
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> BridgeResult<&'a str> {
    if headers.get_all(name).iter().count() != 1 {
        return Err(invalid(format!("exactly one {name} header is required")));
    }
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(format!("missing or invalid {name}")))
}

fn error_response(error: BridgeError) -> Response {
    let status = match error.code.as_str() {
        "invalid" | "unsupported_action" => StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_signature" => StatusCode::UNAUTHORIZED,
        "not_offered" | "not_found" | "unknown_board" => StatusCode::NOT_FOUND,
        "not_granted" | "waiting" | "read_only" => StatusCode::FORBIDDEN,
        "version_conflict" | "request_conflict" | "delivery_conflict" | "stale_fence"
        | "lease_expired" | "not_holder" => StatusCode::CONFLICT,
        "payload_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status == StatusCode::SERVICE_UNAVAILABLE {
        eprintln!("devagotchi hub: issue integration: {error}");
        return (status,Json(serde_json::json!({"error":"issue_store_unavailable","message":"issue storage is unavailable; retry the same delivery/request ID"}))).into_response();
    }
    (status, Json(error)).into_response()
}

pub(crate) async fn webhooks_routes(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let result = async {
        let (event, delivery) = state.issue_store.verify(&headers, &body)?;
        if event != "issues" {
            return Ok(Json(serde_json::json!({"status":"ignored","event":event})).into_response());
        }
        let event: IssueWebhookEvent =
            serde_json::from_slice(&body).map_err(|error| invalid(error.to_string()))?;
        let mapped = state
            .hub
            .config
            .github
            .mapping(&event.repository.full_name)?;
        if event.repository.id != mapped.repository_id {
            return Err(invalid("repository ID does not match configured mapping"));
        }
        if !matches!(
            event.action.as_str(),
            "opened" | "reopened" | "closed" | "edited"
        ) {
            return Ok(
                Json(serde_json::json!({"status":"ignored","action":event.action})).into_response(),
            );
        }
        event.validate()?;
        let boards = state.hub.boards.clone();
        let project = mapped.project.clone();
        let receipt = tokio::time::timeout(std::time::Duration::from_secs(8), async {
            let board = tokio::task::spawn_blocking(move || boards.open(&project))
                .await
                .map_err(|error| {
                    BridgeError::new("board_store", format!("open source board: {error}"))
                })??;
            board
                .hosted_backend()?
                .ingest_issue(&event, &delivery, &hash(&body))
                .await
        })
        .await
        .map_err(|_| {
            BridgeError::new(
                "timeout",
                "webhook transaction timed out; retry this delivery",
            )
        })??;
        Ok(Json(receipt).into_response())
    }
    .await;
    result.unwrap_or_else(error_response)
}

async fn authorized_board(
    state: &AppState,
    subject: &Subject,
    headers: &HeaderMap,
    repository: &str,
    write: bool,
) -> BridgeResult<(Arc<Board>, u64, String)> {
    if let Some(refusal) = state.hub.waiting_refusal(subject) {
        return Err(BridgeError::new("waiting", refusal.message));
    }
    let mapped = state.hub.config.github.mapping(repository)?;
    let seat = headers
        .get(crate::agenthub::mcp::SEAT_HEADER)
        .map(|value| value.to_str().map_err(|_| invalid("invalid seat header")))
        .transpose()?;
    if seat.is_some_and(|seat| !crate::config::valid_name(seat)) {
        return Err(invalid("invalid seat header"));
    }
    let (board, kind, role) = state.hub.board_access(subject, &mapped.project)?;
    // A personal board with the same project name is not the configured source target.
    if kind != crate::agenthub::mcp::Kind::Shared {
        return Err(BridgeError::new(
            "not_granted",
            "issue repository belongs to the configured shared board",
        ));
    }
    if write && !role.may_work() {
        return Err(BridgeError::new(
            "read_only",
            format!(
                "{} is a viewer on {:?}: it reads this board and changes nothing",
                subject.id, mapped.project
            ),
        ));
    }
    let (actor, _) = state.hub.actor_for(subject, seat, kind, role);
    Ok((board, mapped.repository_id, actor))
}

pub(crate) async fn issue_state_route(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Query(query): Query<IssueLookup>,
) -> Response {
    let result = async {
        if query.issue_number == 0
            || query.issue_number > i64::MAX as u64
            || query.after < 0
            || !(1..=100).contains(&query.limit)
        {
            return Err(invalid("invalid issue number or notes page"));
        }
        let (board, repository, _) =
            authorized_board(&state, &subject, &headers, &query.repository, false).await?;
        let context = board
            .hosted_backend()?
            .issue_context(repository, query.issue_number, query.after, query.limit)
            .await?;
        Ok(Json(context).into_response())
    }
    .await;
    result.unwrap_or_else(error_response)
}

pub(crate) async fn shared_state_read_route(
    state: State<AppState>,
    subject: Extension<Subject>,
    headers: HeaderMap,
    query: Query<IssueLookup>,
) -> Response {
    issue_state_route(state, subject, headers, query).await
}

pub(crate) async fn shared_state_write_route(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Json(payload): Json<ContextWrite>,
) -> Response {
    let result = async {
        payload.validate()?;
        let (board, repository, actor) =
            authorized_board(&state, &subject, &headers, &payload.repository, true).await?;
        let receipt = board
            .hosted_backend()?
            .write_issue_context(&actor, repository, &payload)
            .await?;
        Ok(Json(receipt).into_response())
    }
    .await;
    result.unwrap_or_else(error_response)
}

#[cfg(test)]
mod tests;
