//! `/approvals`: the requests agents are waiting on, one tap to approve or deny, laid out
//! for a phone. A signed-in person sees the pending approvals on every board they may use;
//! deciding names the version they saw, so a request that changed is shown again first.
//! Forms only, no script: the session cookie is `SameSite=Lax` and a decision must also
//! come from this hub's own pages (`Sec-Fetch-Site` / `Origin`). "Notify this phone" turns
//! on push for the browser it is tapped in (`notify.rs`); "Send me a test approval" then
//! puts a harmless approval on the person's first board and pushes it to their phones
//! only, so the whole loop is checked with one tap and no AI.
//!
//! Approving ahead of time (`agenthub::approval`, standing authorizations): a request that
//! names its kind gets a third button, approve it and allow this exact step (kind, target and
//! command) for as long as the person set (`crate::preferences`, 30 days until they say). Each
//! board's live ones are listed with their command and a button to end each at once, and a
//! step the person kept approving and never denied is offered with one button, unless they
//! turned offers off or set that kind to always ask them. Every one of these is a person's
//! act from this hub's own page, as deciding is.

mod ahead;
mod layout;
mod render;
pub(super) use render::phone_controls;
use render::{Flash, page};

pub use ahead::{grant, revoke};

use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

use super::{
    pages::{self, escape},
    routes::{Who, not_set_up, see_other, unavailable, who},
};
use crate::{
    AppState,
    agenthub::{
        Actor,
        approval::{self, ApprovalStatus, MAX_STANDING_DAYS, Standing, Suggestion, Verdict},
    },
    identity::{Subject, preferences::Preferences},
};

#[derive(Deserialize)]
pub struct Shown {
    done: Option<String>,
    test: Option<String>,
    ahead: Option<String>,
}

const TEST_TITLE: &str = "Test: tap Approve";
/// The board actor a test approval is asked by, shown as "<product> asks".
const TEST_ASKER: &str = "devagotchi";

#[derive(Deserialize)]
pub struct Decide {
    project: String,
    id: String,
    version: i64,
    verdict: Verdict,
    #[serde(default)]
    note: String,
    /// "Approve and allow": approve this one, and this exact step ahead of time for this many days.
    #[serde(default)]
    allow_days: Option<u32>,
}

/// One board's approvals given ahead of time, and the kinds worth giving.
pub(super) struct Ahead {
    project: String,
    standing: Vec<Standing>,
    suggestions: Vec<Suggestion>,
}

/// A decision must come from this hub's own page, not another site's form.
pub(super) fn same_origin(headers: &HeaderMap, public_url: &str) -> bool {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = header("sec-fetch-site") {
        return site == "same-origin";
    }
    match header("origin") {
        Some(origin) => public_url.trim_end_matches('/') == origin,
        None => true,
    }
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let subject = match who(&state, &headers).await {
        Who::Person(subject) => subject,
        Who::SignedOut => return see_other("/auth/login?return_to=/approvals", &[]),
        Who::Unavailable => return unavailable(),
        Who::NotSetUp => return not_set_up(),
    };
    if !state.hub.may_approve(&subject) {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Only people approve",
            "This account is not marked as a person on this hub, so it cannot approve or deny.",
            Some(("/account", "Your account")),
        );
    }
    let pending = match pending(&state, &subject).await {
        Ok(pending) => pending,
        Err(error) => {
            eprintln!("devagotchi hub: approvals: {error}");
            return unavailable();
        }
    };
    let key = state.hub.push.as_ref().map(|p| p.public_key.clone());
    let flash = Flash {
        done: shown.done.as_deref(),
        test: shown.test.as_deref(),
        ahead: shown.ahead.as_deref(),
    };
    let can_test = !pending.is_empty()
        || match state.hub.approval_boards(&subject) {
            Ok(boards) => !boards.is_empty(),
            Err(error) => {
                eprintln!("devagotchi hub: approval boards: {error}");
                return unavailable();
            }
        };
    let preferences = super::app::me::of(&state, &subject).await;
    let ahead = match ahead::ahead(&state, &subject, &preferences).await {
        Ok(ahead) => ahead,
        Err(error) => {
            eprintln!("devagotchi hub: approvals ahead of time: {error}");
            return unavailable();
        }
    };
    let access_count = match super::app::access::attention(&state, &subject).await {
        Ok(count) => count,
        Err(_) => return unavailable(),
    };
    page(
        &pending,
        &flash,
        key.as_deref(),
        &subject.id,
        can_test,
        &ahead,
        &preferences,
        access_count,
    )
}

/// The approvals waiting for `subject`, by project: on every board it decides on, its
/// account's own first (`Hub::approval_boards`). Also the phone app's Needs you
/// (`web/app/home.rs`).
pub(super) async fn pending(
    state: &AppState,
    subject: &Subject,
) -> devagotchi_contracts::BridgeResult<Vec<(String, ApprovalStatus)>> {
    let boards = state.hub.approval_boards(subject)?;
    tokio::task::spawn_blocking(move || {
        let mut all = Vec::new();
        for (project, board) in boards {
            for approval in board.pending_approvals()? {
                all.push((project.clone(), approval));
            }
        }
        Ok(all)
    })
    .await
    .map_err(|error| devagotchi_contracts::BridgeError::new("board_store", error.to_string()))?
}

/// "Send me a test approval": a harmless approval on the person's first board, pushed to
/// their own phones only.
pub async fn test(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !same_origin(&headers, &state.hub.config.public_url) {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Not from this hub",
            "Send the test from the approvals page itself.",
            Some(("/approvals", "Approvals")),
        );
    }
    let subject = match who(&state, &headers).await {
        Who::Person(subject) => subject,
        Who::SignedOut => return see_other("/auth/login?return_to=/approvals", &[]),
        Who::Unavailable => return unavailable(),
        Who::NotSetUp => return not_set_up(),
    };
    // A tenant waiting to be let in writes no board (`tenancy/gate.rs`).
    if let Some(refused) = state.hub.waiting_refusal(&subject) {
        return waiting_page(&refused);
    }
    let Some(push) = state.hub.push.clone() else {
        return see_other("/approvals", &[]);
    };
    let boards = match state.hub.approval_boards(&subject) {
        Ok(boards) => boards,
        Err(error) => {
            eprintln!("devagotchi hub: approval boards: {error}");
            return unavailable();
        }
    };
    let Some((project, board)) = boards.into_iter().next() else {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Not yours to test",
            "This account has no board it decides approvals on.",
            Some(("/account", "Your account")),
        );
    };
    let phones = push.phones_for(std::slice::from_ref(&subject.id)).await;
    if phones == 0 {
        return see_other("/approvals?test=no-phone", &[]);
    }
    let made = tokio::task::spawn_blocking(move || {
        board.request_approval(
            &Actor::agent(TEST_ASKER),
            TEST_TITLE,
            "Nothing runs. This checks that your phone gets the notification and that your answer comes back.",
            "You tapped Send me a test approval.",
            "Nothing to undo.",
        )
    })
    .await
    .unwrap_or_else(|_| Err(not_answered()));
    let target = made.as_ref().map(|a| a.id.clone()).unwrap_or_default();
    audited(
        &state,
        &subject,
        "approval_request",
        &project,
        &target,
        &made,
    )
    .await;
    match made {
        Ok(approval) => {
            state
                .hub
                .announce_approval_to(vec![subject.id.clone()], &project, &approval)
                .await;
            see_other("/approvals?test=sent", &[])
        }
        _ => pages::message(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Test not sent",
            "The board did not answer; nothing was sent.",
            Some(("/approvals", "Back to approvals")),
        ),
    }
}

pub async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Decide>,
) -> Response {
    if !same_origin(&headers, &state.hub.config.public_url) {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Not from this hub",
            "Decide from the approvals page itself.",
            Some(("/approvals", "Approvals")),
        );
    }
    let subject = match who(&state, &headers).await {
        Who::Person(subject) => subject,
        Who::SignedOut => return see_other("/auth/login?return_to=/approvals", &[]),
        Who::Unavailable => return unavailable(),
        Who::NotSetUp => return not_set_up(),
    };
    // A tenant waiting to be let in writes no board (`tenancy/gate.rs`).
    if let Some(refused) = state.hub.waiting_refusal(&subject) {
        return waiting_page(&refused);
    }
    // Its account's own board, or a configured one it is a person on (`Hub::approval_board`).
    let Ok(board) = state.hub.approval_board(&subject, &form.project) else {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Not yours to decide",
            "This account cannot decide approvals on that board.",
            Some(("/approvals", "Approvals")),
        );
    };
    let preferences = super::app::me::of(&state, &subject).await;
    let id = subject.id.clone();
    let (project, ticket) = (form.project.clone(), form.id.clone());
    let result = tokio::task::spawn_blocking(move || {
        let actor = Actor::human(&id);
        // "Approve and allow": this one, and this exact step ahead of time from now on, in
        // one transaction, unless the person set its kind to always ask them. Whether it has
        // a kind is the store's to read; the preference is the person's.
        match form.allow_days {
            Some(days) if form.verdict == Verdict::Approved => {
                let asked = board.approval_status(&form.id)?;
                if asked
                    .kind
                    .as_deref()
                    .is_some_and(|kind| preferences.always_asks(kind))
                {
                    board.decide_approval(&actor, &form.id, form.version, form.verdict, &form.note)
                } else {
                    board
                        .decide_and_allow(
                            &actor,
                            &form.id,
                            form.version,
                            &form.note,
                            days.min(MAX_STANDING_DAYS),
                        )
                        .map(|(decided, _)| decided)
                }
            }
            _ => board.decide_approval(&actor, &form.id, form.version, form.verdict, &form.note),
        }
    })
    .await
    .unwrap_or_else(|_| Err(not_answered()));
    let target = match &result {
        Ok(decided) => format!("{ticket} {}", decided.status),
        Err(_) => ticket,
    };
    audited(
        &state,
        &subject,
        "approval_decide",
        &project,
        &target,
        &result,
    )
    .await;
    match result {
        Ok(decided) => see_other(&format!("/approvals?done={}", decided.id), &[]),
        Err(error) if error.code == "board_store" => pages::message(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Not decided",
            "The board did not answer; nothing changed.",
            Some(("/approvals", "Back to approvals")),
        ),
        Err(error) => pages::message(
            StatusCode::CONFLICT,
            "Not decided",
            &error.message,
            Some(("/approvals", "Back to approvals")),
        ),
    }
}

/// A board call that never came back (its blocking task ended).
fn not_answered() -> devagotchi_contracts::BridgeError {
    devagotchi_contracts::BridgeError::new("board_store", "the board did not answer")
}

/// One audit line for a write made from these pages, after it (rule 12, G7): `tool` is the
/// MCP tool for the same intent, the project the place, `target` what it touched.
async fn audited<T>(
    state: &AppState,
    subject: &Subject,
    tool: &str,
    project: &str,
    target: &str,
    result: &devagotchi_contracts::BridgeResult<T>,
) {
    let outcome = match result {
        Ok(_) => "ok",
        Err(error) => error.code.as_str(),
    };
    state
        .hub
        .record_completion(subject, tool, project, target, outcome)
        .await;
}

/// What a tenant waiting to be let in sees instead of a board change.
fn waiting_page(refused: &devagotchi_contracts::BridgeError) -> Response {
    pages::message(
        StatusCode::FORBIDDEN,
        &pages::site().messages.access.waiting_title,
        &refused.message,
        Some(("/account", "Your account")),
    )
}
