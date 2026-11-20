//! Approving ahead of time from `/approvals`: what each board the person decides on approves
//! ahead of time now and what they keep approving (`ahead`), taking one of those offers
//! (`grant`), and ending one at once (`revoke`). Each is a person's act from this hub's own
//! page, one store call, one audit line.

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

use super::{Ahead, audited, not_answered, same_origin, waiting_page};
use crate::{
    AppState,
    agenthub::{
        Actor,
        approval::{Standing, Suggestion},
    },
    identity::{Subject, preferences::Preferences},
    web::{
        pages,
        routes::{Who, not_set_up, see_other, unavailable, who},
    },
};

#[derive(Deserialize)]
pub struct Grant {
    project: String,
    kind: String,
    #[serde(default)]
    target: Option<String>,
    what: String,
}

#[derive(Deserialize)]
pub struct Revoke {
    project: String,
    id: String,
}

/// On every board `subject` decides on: what is approved ahead of time there now, and what
/// the person keeps approving.
pub(super) async fn ahead(
    state: &AppState,
    subject: &Subject,
    preferences: &Preferences,
) -> devagotchi_contracts::BridgeResult<Vec<Ahead>> {
    let boards = state.hub.approval_boards(subject)?;
    let preferences = preferences.clone();
    tokio::task::spawn_blocking(move || {
        let mut all = Vec::new();
        for (project, board) in boards {
            let standing: Vec<Standing> = board.live_standing()?;
            let suggestions: Vec<Suggestion> = if preferences.offer_ahead {
                board
                    .suggestions()?
                    .into_iter()
                    .filter(|s| !preferences.always_asks(&s.kind))
                    .collect()
            } else {
                Vec::new()
            };
            if !standing.is_empty() || !suggestions.is_empty() {
                all.push(Ahead {
                    project,
                    standing,
                    suggestions,
                });
            }
        }
        Ok(all)
    })
    .await
    .map_err(|error| devagotchi_contracts::BridgeError::new("board_store", error.to_string()))?
}

/// The person on a board they decide on, for a change to what is approved ahead of time:
/// from this hub's page, signed in, let in. The page that answers instead otherwise.
async fn decider(
    state: &AppState,
    headers: &HeaderMap,
    project: &str,
) -> Result<(Subject, std::sync::Arc<crate::agenthub::Board>), Response> {
    if !same_origin(headers, &state.hub.config.public_url) {
        return Err(pages::message(
            StatusCode::FORBIDDEN,
            "Not from this hub",
            "Change this from the approvals page itself.",
            Some(("/approvals", "Approvals")),
        ));
    }
    let subject = match who(state, headers).await {
        Who::Person(subject) => subject,
        Who::SignedOut => return Err(see_other("/auth/login?return_to=/approvals", &[])),
        Who::Unavailable => return Err(unavailable()),
        Who::NotSetUp => return Err(not_set_up()),
    };
    if let Some(refused) = state.hub.waiting_refusal(&subject) {
        return Err(waiting_page(&refused));
    }
    match state.hub.approval_board(&subject, project) {
        Ok(board) => Ok((subject, board)),
        Err(_) => Err(pages::message(
            StatusCode::FORBIDDEN,
            "Not yours to decide",
            "This account cannot decide approvals on that board.",
            Some(("/approvals", "Approvals")),
        )),
    }
}

/// "Approve automatically for 30 days": a suggestion taken.
pub async fn grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Grant>,
) -> Response {
    let (subject, board) = match decider(&state, &headers, &form.project).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let preferences = crate::web::app::me::of(&state, &subject).await;
    if preferences.always_asks(&form.kind) {
        return pages::message(
            StatusCode::CONFLICT,
            "Not changed",
            "You set this kind of step to always ask you. Change that on your page first.",
            Some(("/app/me", "Your page")),
        );
    }
    let days = preferences.allow_days;
    // A browser sends a multi-line field with CRLF; the command was stored with LF.
    let what = form.what.replace("\r\n", "\n");
    let id = subject.id.clone();
    let project = form.project.clone();
    let result = tokio::task::spawn_blocking(move || {
        board.grant_standing(
            &Actor {
                id: &id,
                human: true,
                wip_limit: None,
            },
            &form.kind,
            form.target.as_deref().filter(|t| !t.is_empty()),
            &what,
            days,
        )
    })
    .await
    .unwrap_or_else(|_| Err(not_answered()));
    let target = result.as_ref().map(|s| s.id.clone()).unwrap_or_default();
    audited(&state, &subject, "approval_allow", &project, &target, &result).await;
    match result {
        Ok(_) => see_other("/approvals?ahead=granted", &[]),
        Err(error) if error.code == "board_store" => unavailable(),
        Err(error) => pages::message(
            StatusCode::CONFLICT,
            "Not changed",
            &error.message,
            Some(("/approvals", "Back to approvals")),
        ),
    }
}

/// "Stop": a standing authorization ended at once.
pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Revoke>,
) -> Response {
    let (subject, board) = match decider(&state, &headers, &form.project).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let id = subject.id.clone();
    let (project, standing) = (form.project.clone(), form.id.clone());
    let result = tokio::task::spawn_blocking(move || {
        board.revoke_standing(
            &Actor {
                id: &id,
                human: true,
                wip_limit: None,
            },
            &form.id,
        )
    })
    .await
    .unwrap_or_else(|_| Err(not_answered()));
    audited(&state, &subject, "approval_revoke", &project, &standing, &result).await;
    match result {
        Ok(_) => see_other("/approvals?ahead=revoked", &[]),
        Err(error) if error.code == "board_store" => unavailable(),
        Err(error) => pages::message(
            StatusCode::CONFLICT,
            "Not changed",
            &error.message,
            Some(("/approvals", "Back to approvals")),
        ),
    }
}

