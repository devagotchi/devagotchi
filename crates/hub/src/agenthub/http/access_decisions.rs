//! Independent human approval uses the requester's stored original task context.
use super::{Body, Caller, Reply, invalid, machine_access::owner, response::answer};
use crate::{
    AppState,
    machines::access::{Decision, Narrow},
};
use axum::{
    Router,
    extract::{Path, State},
    routing::post,
};
use serde::Deserialize;
use serde_json::json;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/access-requests/{id}/approve", post(approve))
        .route("/api/v1/access-requests/{id}/deny", post(deny))
}
async fn approve(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    decide(state, caller, id, value, true).await
}
async fn deny(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    decide(state, caller, id, value, false).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Denial {
    note: String,
}
async fn decide(
    state: AppState,
    mut caller: Caller,
    id: String,
    value: serde_json::Value,
    approve: bool,
) -> Reply {
    let access = owner(&state, &caller, true)?;
    let (mut value, key) = super::access_admin::prepared(value, &caller, Some(&id), true)?;
    let version = value
        .as_object_mut()
        .unwrap()
        .remove("version")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| invalid("decision requires a positive version"))?;
    let decision = if approve {
        Decision::Approve {
            narrow: serde_json::from_value::<Narrow>(value)
                .map_err(|_| invalid("invalid approval narrowing"))?,
        }
    } else {
        Decision::Deny {
            note: serde_json::from_value::<Denial>(value)
                .map_err(|_| invalid("denial accepts only a note and version"))?
                .note,
        }
    };
    let (account, project) = access.decision_target(&caller.subject.id, &id).await?;
    let board = if approve && let Some(project) = project {
        let boards = caller.hub.boards.clone();
        Some(
            tokio::task::spawn_blocking(move || boards.open_account(&account, &project))
                .await
                .map_err(|_| invalid("request task board could not be resolved"))??
                .ok_or_else(|| invalid("request's original project is no longer available"))?,
        )
    } else {
        None
    };
    caller.refresh(&state).await?;
    owner(&state, &caller, true)?;
    let result = access
        .decide(
            &caller.subject.id,
            board.as_ref().map(|b| b.hosted()),
            &id,
            version,
            &decision,
            &key,
        )
        .await?;
    let replayed = result.replayed;
    Ok(answer(json!(result), replayed))
}
