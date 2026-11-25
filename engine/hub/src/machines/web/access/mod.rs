//! Human machine requests and grants, in the same app and domain owners as HTTP/MCP.
use crate::web::app::{person, requests};
use crate::{
    AppState,
    identity::Subject,
    machines::access::{Access, AccessPageQuery, AccessReview, Decision, Narrow},
    web::{
        encode,
        pages::{self, Tab, escape},
    },
};
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use devagotchi_agenthub::Actor;
use devagotchi_contracts::{BridgeError, BridgeResult, GrantDuration, GrantItem, GrantLevel};
use serde::Deserialize;
use serde_json::Value;
mod form;
mod render;
#[cfg(test)]
mod tests;
use form::Draft;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/access", get(index))
        .route("/app/access/requests/{id}", get(review).post(decide))
        .route("/app/access/grants", get(grants))
        .route(
            "/app/access/grants/{id}/revoke",
            axum::routing::post(revoke),
        )
}
fn invalid(message: &str) -> BridgeError {
    BridgeError::new("invalid", message)
}
fn status(error: &BridgeError) -> StatusCode {
    match error.code.as_str() {
        "not_owner" | "not_granted" | "humans_only" | "policy_denied" | "own_request" => {
            StatusCode::FORBIDDEN
        }
        "not_found" => StatusCode::NOT_FOUND,
        "version_conflict" | "request_conflict" | "not_pending" | "stale_fence"
        | "machine_reenrolled" => StatusCode::CONFLICT,
        "invalid" | "scope_wider_than_request" | "outside_grant" | "invalid_grant" => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}
fn message(error: &BridgeError) -> String {
    if status(error) == StatusCode::SERVICE_UNAVAILABLE {
        pages::site().messages.app.access.unavailable.clone()
    } else {
        error.message.clone()
    }
}
fn refused(error: &BridgeError) -> Response {
    let site = pages::site();
    let c = &site.messages.app.access;
    pages::message(
        status(error),
        &c.refused,
        &message(error),
        Some(("/app/access", &c.back)),
    )
}
async fn owner(
    state: &AppState,
    headers: &HeaderMap,
    changing: bool,
) -> Result<(Subject, Access), Response> {
    let subject = person(state, headers, "/app/access").await?;
    if subject.org.is_some()
        || subject.agent.is_some()
        || headers.contains_key(crate::agenthub::mcp::SEAT_HEADER)
        || changing && !crate::web::from_this_hub(headers, &state.hub.config.public_url)
    {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    let accounts = state
        .passkeys
        .as_ref()
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?
        .accounts()
        .clone();
    crate::catalog::Catalog::new(accounts.clone())
        .require_owner(&subject.id)
        .await
        .map_err(|e| refused(&e))?;
    Ok((subject, Access::new(accounts)))
}
/// Optional Home/Approvals entry: ordinary users never see an owner-only action.
pub(crate) async fn attention(state: &AppState, subject: &Subject) -> BridgeResult<Option<u64>> {
    let Some(sign_in) = &state.passkeys else {
        return Ok(None);
    };
    let Some(account) = sign_in.accounts().get(&subject.id).await? else {
        return Ok(None);
    };
    if !account.owner || subject.agent.is_some() || subject.org.is_some() {
        return Ok(None);
    }
    Access::new(sign_in.accounts().clone())
        .pending_count(&subject.id)
        .await
        .map(Some)
}
pub(crate) fn attention_link(count: Option<u64>) -> String {
    let Some(count) = count else {
        return String::new();
    };
    let site = pages::site();
    let c = &site.messages.app.access;
    format!(
        "<p class=\"note\"><a href=\"/app/access\">{}</a></p>",
        escape(&if count == 0 {
            c.title.clone()
        } else {
            c.request_count.replace("{count}", &count.to_string())
        })
    )
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<String>,
    #[serde(default)]
    history: bool,
    #[serde(default)]
    ended: bool,
}
async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let (subject, access) = match owner(&state, &headers, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let result = access
        .access_requests(
            &subject.id,
            &Actor::human(&subject.id),
            &AccessPageQuery {
                status: (!page.history).then(|| "pending".into()),
                after: page.after,
                limit: Some(25),
                ..Default::default()
            },
        )
        .await;
    match result {
        Ok(rows) => render::requests(&rows.rows, rows.next.as_deref(), page.history),
        Err(e) => refused(&e),
    }
}
async fn review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let (subject, access) = match owner(&state, &headers, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match access.review_request(&subject.id, &id).await {
        Ok(review) => render::review(&review, &Draft::new(&review), None),
        Err(e) => refused(&e),
    }
}
async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let (subject, access) = match owner(&state, &headers, true).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let review = match access.review_request(&subject.id, &id).await {
        Ok(v) => v,
        Err(e) => return refused(&e),
    };
    let (draft, parse_error) = Draft::submitted(fields);
    let result = async {
        if let Some(e) = parse_error {
            return Err(e);
        }
        let decision = draft.decision(&review.terms)?;
        let board = if matches!(decision, Decision::Approve { .. }) && review.project.is_some() {
            let boards = state.hub.boards.clone();
            let account = review.account.clone();
            let project = review.project.clone().unwrap();
            Some(
                tokio::task::spawn_blocking(move || boards.open_account(&account, &project))
                    .await
                    .map_err(|_| invalid("task board resolution failed"))??
                    .ok_or_else(|| invalid("original task project is unavailable"))?,
            )
        } else {
            None
        };
        // Board lookup may wait. Re-read the session, retaining the original identity.
        let live = crate::web::session_subject(&state, &headers)
            .await
            .ok_or_else(|| BridgeError::new("not_granted", "session is no longer valid"))?;
        if live.id != subject.id || live.agent != subject.agent || live.tenant != subject.tenant {
            return Err(BridgeError::new("not_granted", "session identity changed"));
        }
        access
            .decide(
                &subject.id,
                board.as_ref().map(|b| b.hosted()),
                &id,
                draft.version()?,
                &decision,
                &draft.request_id,
            )
            .await
    }
    .await;
    match result {
        Ok(changed) => requests::redirect(
            &format!("/app/access/requests/{}", encode(&id)),
            changed.replayed,
        ),
        Err(e) => render::review(&review, &draft, Some(&e)),
    }
}
async fn grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let (subject, access) = match owner(&state, &headers, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match access
        .access_grants(
            &subject.id,
            &Actor::human(&subject.id),
            &AccessPageQuery {
                live: !page.ended,
                ended: page.ended,
                after: page.after,
                limit: Some(25),
                ..Default::default()
            },
        )
        .await
    {
        Ok(rows) => render::grants(&rows.rows, rows.next.as_deref(), page.ended, None),
        Err(e) => refused(&e),
    }
}
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let (subject, access) = match owner(&state, &headers, true).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (draft, parse_error) = Draft::submitted(fields);
    let result = async {
        if let Some(e) = parse_error {
            return Err(e);
        }
        if draft.action != "revoke" {
            return Err(invalid("invalid grant action"));
        }
        access
            .revoke(&subject.id, &id, draft.version()?, &draft.request_id)
            .await
    }
    .await;
    match result {
        Ok(changed) => requests::redirect("/app/access/grants?ended=true", changed.replayed),
        Err(e) => match access
            .access_grants(
                &subject.id,
                &Actor::human(&subject.id),
                &AccessPageQuery {
                    limit: Some(25),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(rows) => render::grants(
                &rows.rows,
                rows.next.as_deref(),
                false,
                Some((&id, &draft, &e)),
            ),
            Err(read) => refused(&read),
        },
    }
}
