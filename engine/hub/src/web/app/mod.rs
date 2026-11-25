//! The phone control app (`docs/spec-phone-app.md`): server-rendered pages in one fixed
//! frame (`pages::app_page`: the bar on top, the tabs at the bottom, the page scrolling in
//! between), installable from the hub's manifest. Every word is the site's
//! (`content/messages.json` `app`). The one script, `/assets/app.js`, shows the install
//! banner, refreshes on a pull and registers the service worker, which keeps the frame and
//! the stylesheet so the app opens offline on `/app/offline`.

pub(crate) mod home;
pub(crate) mod requests;

use std::sync::Arc;

use axum::{
    Router,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};

use super::{
    pages::{self, escape},
    routes::{Who, not_set_up, see_other, unavailable, who},
};
use crate::{AppState, agenthub::Board, identity::Subject, site::Site};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/app", get(home::home))
        .route("/app/machines", get(home::machines))
        .route("/app/boards", get(home::boards))
        .route("/app/attention", get(attention))
        .route("/app/offline", get(offline))
        .route("/assets/app.js", get(script))
        .route("/assets/reveal.js", get(reveal))
        .route("/app/focus", get(focus))
}

/// The person signed in to this browser, or the page that answers instead: signed out,
/// the sign-in page that comes back to `return_to`.
pub(crate) async fn person(
    state: &AppState,
    headers: &HeaderMap,
    return_to: &str,
) -> Result<Subject, Response> {
    match who(state, headers).await {
        Who::Person(subject) => Ok(subject),
        Who::SignedOut => Err(see_other(
            &format!("/auth/login?return_to={}", super::encode(return_to)),
            &[],
        )),
        Who::Unavailable => Err(unavailable()),
        Who::NotSetUp => Err(not_set_up()),
    }
}

/// The boards the person may work on, by project, as `whoami` lists them: the account's
/// own (or its org's) first, then the shared ones it holds a grant for.
pub(crate) fn board_names(
    state: &AppState,
    subject: &Subject,
) -> devagotchi_contracts::BridgeResult<Vec<String>> {
    state.hub.board_names(subject)
}

/// The board `project` names for this person, as the board tools open it
/// (`Hub::board_for`): the account's own first, then a shared one by grant. None when it
/// is not theirs.
pub(crate) fn open_board(
    state: &AppState,
    subject: &Subject,
    project: &str,
) -> devagotchi_contracts::BridgeResult<Option<Arc<Board>>> {
    match state.hub.board_for(subject, project) {
        Ok((board, _)) => Ok(Some(board)),
        Err(error) if matches!(error.code.as_str(), "unknown_board" | "not_granted") => Ok(None),
        Err(error) => Err(error),
    }
}

/// The site's words for the app.
pub(crate) fn words() -> Arc<Site> {
    pages::site()
}

/// A card: a heading and what is under it, or the line that says it is empty.
pub(crate) fn card(id: &str, title: &str, rows: &[String], empty: &str) -> String {
    let inner = if rows.is_empty() {
        format!("<p class=\"empty\">{}</p>", escape(empty))
    } else {
        format!("<ul class=\"rows\">{}</ul>", rows.concat())
    };
    format!(
        "<section class=\"card\" id=\"{id}\"><h2>{}</h2>{inner}</section>",
        escape(title)
    )
}

/// `/app/attention`: how many approvals wait for this person, for the count on the Approvals
/// tab and on the installed app's icon (`app.js`).
async fn attention(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Response {
    use axum::response::IntoResponse;
    let subject = match who(&state, &headers).await {
        Who::Person(subject) => subject,
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let approvals = if state.hub.may_approve(&subject) {
        match crate::agenthub::web::approvals::pending(&state, &subject).await {
            Ok(waiting) => waiting.len(),
            Err(error) => {
                eprintln!("devagotchi hub: attention: {error}");
                return unavailable();
            }
        }
    } else {
        0
    };
    let machine_requests = match crate::machines::web::access::attention(&state, &subject).await {
        Ok(count) => count,
        Err(_) => return unavailable(),
    };
    let Some(approvals) = (approvals as u64).checked_add(machine_requests.unwrap_or(0)) else {
        return unavailable();
    };
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({ "approvals": approvals })),
    )
        .into_response()
}

/// `/app/focus`: what the Go to sheet lists for this person (`app.js`): their boards, their
/// machines with how each is, and whether they own the hub (Hub settings).
async fn focus(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Response {
    use axum::response::IntoResponse;
    let subject = match who(&state, &headers).await {
        Who::Person(subject) => subject,
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let boards = match board_names(&state, &subject) {
        Ok(boards) => boards,
        Err(error) => {
            eprintln!("devagotchi hub: focus boards: {error}");
            return unavailable();
        }
    };
    let machines: Vec<serde_json::Value> = state
        .hub
        .machine_status(&subject)
        .await
        .iter()
        .map(|machine| {
            serde_json::json!({
                "name": machine.name,
                "status": home::status_word(machine.status),
            })
        })
        .collect();
    let owner = crate::settings::web::admin::is_owner(&state, &subject);
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({ "boards": boards, "machines": machines, "owner": owner })),
    )
        .into_response()
}

/// `/app/offline`: what the service worker shows when the hub cannot be reached.
async fn offline() -> Response {
    let site = words();
    let words = &site.messages.app;
    pages::app_page(
        StatusCode::OK,
        &words.offline_title,
        &format!(
            "<h1>{}</h1><p class=\"note\">{}</p>",
            escape(&words.offline_title),
            escape(&words.offline_text)
        ),
        pages::Tab::Home,
    )
}

/// `/assets/app.js`: the app's one script.
async fn script() -> Response {
    crate::notify::web::asset(
        include_str!("../assets/app.js").to_string(),
        "text/javascript; charset=utf-8",
        "no-cache",
    )
}

/// `/assets/reveal.js`: run before an app page is first drawn, so it slides in from the side
/// the person swiped or tapped towards.
async fn reveal() -> Response {
    crate::notify::web::asset(
        include_str!("../assets/reveal.js").to_string(),
        "text/javascript; charset=utf-8",
        "no-cache",
    )
}

#[cfg(test)]
pub(crate) mod tests;
