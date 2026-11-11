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
    preferences::Preferences,
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
    let ahead = match ahead(&state, &subject, &preferences).await {
        Ok(ahead) => ahead,
        Err(error) => {
            eprintln!("devagotchi hub: approvals ahead of time: {error}");
            return unavailable();
        }
    };
    page(
        &pending,
        &flash,
        key.as_deref(),
        &subject.id,
        can_test,
        &ahead,
        &preferences,
    )
}

/// On every board `subject` decides on: what is approved ahead of time there now, and what
/// the person keeps approving.
async fn ahead(
    state: &AppState,
    subject: &crate::auth::Subject,
    preferences: &Preferences,
) -> devagotchi_contracts::BridgeResult<Vec<Ahead>> {
    let boards = state.hub.approval_boards(subject)?;
    let preferences = preferences.clone();
    tokio::task::spawn_blocking(move || {
        let mut all = Vec::new();
        for (project, board) in boards {
            let now = board.now();
            let standing: Vec<Standing> = board
                .standing()?
                .into_iter()
                .filter(|s| s.live(now))
                .collect();
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
) -> Result<(crate::auth::Subject, std::sync::Arc<crate::agenthub::Board>), Response> {
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
    let preferences = super::app::me::of(&state, &subject).await;
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
    .await;
    match result {
        Ok(Ok(_)) => see_other("/approvals?ahead=granted", &[]),
        Ok(Err(error)) => pages::message(
            StatusCode::CONFLICT,
            "Not changed",
            &error.message,
            Some(("/approvals", "Back to approvals")),
        ),
        Err(_) => unavailable(),
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
    .await;
    match result {
        Ok(Ok(_)) => see_other("/approvals?ahead=revoked", &[]),
        Ok(Err(error)) => pages::message(
            StatusCode::CONFLICT,
            "Not changed",
            &error.message,
            Some(("/approvals", "Back to approvals")),
        ),
        Err(_) => unavailable(),
    }
}

/// The approvals waiting for `subject`, by project: on every board it decides on, its
/// account's own first (`Hub::approval_boards`). Also the phone app's Needs you
/// (`web/app/home.rs`).
pub(super) async fn pending(
    state: &AppState,
    subject: &crate::auth::Subject,
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
    // A tenant waiting to be let in writes no board (`tenant_access.rs`).
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
    let phones = push
        .devices()
        .push_for(&subject.id)
        .map(|p| p.len())
        .unwrap_or(0);
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
    .await;
    match made {
        Ok(Ok(approval)) => {
            state
                .hub
                .announce_approval_to(vec![subject.id.clone()], &project, &approval);
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
    // A tenant waiting to be let in writes no board (`tenant_access.rs`).
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
    let result = tokio::task::spawn_blocking(move || {
        let actor = Actor {
            id: &id,
            human: true,
            wip_limit: None,
        };
        let decided =
            board.decide_approval(&actor, &form.id, form.version, form.verdict, &form.note)?;
        // "Approve and allow": this exact step, ahead of time from now on.
        if let (Some(days), Verdict::Approved, Some(kind), Some(what)) = (
            form.allow_days,
            form.verdict,
            decided.kind.as_deref(),
            decided.what(),
        ) && !preferences.always_asks(kind)
        {
            board.grant_standing(
                &actor,
                kind,
                decided.target.as_deref(),
                what,
                days.min(MAX_STANDING_DAYS),
            )?;
        }
        Ok::<_, devagotchi_contracts::BridgeError>(decided)
    })
    .await;
    match result {
        Ok(Ok(decided)) => see_other(&format!("/approvals?done={}", decided.id), &[]),
        Ok(Err(error)) => pages::message(
            StatusCode::CONFLICT,
            "Not decided",
            &error.message,
            Some(("/approvals", "Back to approvals")),
        ),
        Err(_) => pages::message(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Not decided",
            "The board did not answer; nothing changed.",
            Some(("/approvals", "Back to approvals")),
        ),
    }
}

struct Flash<'a> {
    done: Option<&'a str>,
    test: Option<&'a str>,
    ahead: Option<&'a str>,
}

/// "restart", or "restart on dabo".
fn kind_words(kind: &str, target: Option<&str>) -> String {
    match target {
        Some(target) => format!("{kind} on {target}"),
        None => kind.to_string(),
    }
}

/// Who asked, as the person reads it: their own account, or its AI's seat on its own board.
fn asker(requested_by: &str, viewer: &str) -> String {
    if requested_by == viewer || requested_by == "ai" {
        "Your AI asks".into()
    } else if requested_by == TEST_ASKER {
        format!("{} asks", escape(super::pages::site().name()))
    } else {
        format!("{} asks", escape(requested_by))
    }
}

/// What, why and how to undo it, as the person reads them; a description that does not read
/// only one way, whole, as it was written.
fn facts(description: &str) -> String {
    match approval::parts(description) {
        Some((what, why, undo)) => format!(
            "<dl class=\"ask-facts\"><dt>What</dt><dd class=\"cmd\">{}</dd><dt>Why</dt><dd>{}</dd>\
<dt>Undo</dt><dd>{}</dd></dl>",
            escape(what),
            escape(why),
            escape(undo)
        ),
        None => format!("<pre class=\"what\">{}</pre>", escape(description)),
    }
}

/// "Notify this phone" and "Send me a test approval"; nothing on a hub without push.
pub(super) fn phone_controls(push_key: Option<&str>, can_test: bool) -> String {
    push_key
        .map(|key| {
            format!(
                "<p><button id=\"notify\" type=\"button\" data-key=\"{}\">Notify this phone</button> \
<span id=\"notify-status\" class=\"note\" role=\"status\"></span></p>{}",
                escape(key),
                if can_test {
                    "<form method=\"post\" action=\"/approvals/test\"><p><button class=\"test\" type=\"submit\">\
Send me a test approval</button></p></form>"
                } else {
                    ""
                }
            )
        })
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn page(
    pending: &[(String, ApprovalStatus)],
    flash: &Flash<'_>,
    push_key: Option<&str>,
    viewer: &str,
    can_test: bool,
    ahead: &[Ahead],
    preferences: &Preferences,
) -> Response {
    let allow_days = preferences.allow_days;
    let decided = flash
        .done
        .map(|id| {
            format!(
                "<p class=\"note\">Decided {}. The agent continues on its own.</p>",
                escape(id)
            )
        })
        .unwrap_or_default();
    let tested = match flash.test {
        Some("sent") => {
            "<p class=\"note\">Test sent. Your phone should buzz within a few seconds: approve it from the notification.</p>"
        }
        Some("no-phone") => {
            "<p class=\"note\">Tap Notify this phone first, then send the test.</p>"
        }
        _ => "",
    };
    let changed = match flash.ahead {
        Some("granted") => format!(
            "<p class=\"note\">Done: that exact step is approved ahead of time for {allow_days} days. You can stop it below.</p>"
        ),
        Some("revoked") => "<p class=\"note\">Stopped: it needs you again.</p>".to_string(),
        _ => String::new(),
    };
    let flash = format!("{decided}{tested}{changed}");
    let cards: String = pending
        .iter()
        .map(|(project, a)| {
            format!(
                "<section class=\"card ask\" id=\"{p}-{i}\"><p class=\"ask-tags\">{kind}<span class=\"tag\">{p}</span></p>\
<h2>{t}</h2><p class=\"meta\">{by} · {when}</p>{d}<form method=\"post\" action=\"/approvals/decide\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"id\" value=\"{i}\">\
<input type=\"hidden\" name=\"version\" value=\"{v}\">\
<details class=\"note-box\"><summary>Add a note</summary>\
<label>Note (optional)<input type=\"text\" name=\"note\" maxlength=\"2000\"></label></details>\
<p class=\"actions\"><button class=\"approve\" name=\"verdict\" value=\"approved\">Approve</button> \
<button class=\"deny\" name=\"verdict\" value=\"denied\">Deny</button></p></form>{allow}</section>",
                p = escape(project),
                i = escape(&a.id),
                t = escape(&a.title),
                by = asker(&a.requested_by, viewer),
                kind = a
                    .kind
                    .as_deref()
                    .map(|kind| format!(
                        "<span class=\"tag kind\">{}</span>",
                        escape(&kind_words(kind, a.target.as_deref()))
                    ))
                    .unwrap_or_default(),
                when = pages::ago((a.requested_at / 1000).max(0) as u64),
                d = facts(&a.description),
                v = a.version,
                allow = a
                    .kind
                    .as_deref()
                    .map(|kind| if preferences.always_asks(kind) {
                        format!(
                            "<p class=\"note always\">You always decide {} yourself.</p>",
                            escape(kind)
                        )
                    } else if a.what().is_none() {
                        String::new()
                    } else { format!(
                        "<form method=\"post\" action=\"/approvals/decide\" class=\"allow\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"id\" value=\"{i}\">\
<input type=\"hidden\" name=\"version\" value=\"{v}\"><input type=\"hidden\" name=\"verdict\" value=\"approved\">\
<input type=\"hidden\" name=\"allow_days\" value=\"{allow_days}\">\
<p class=\"actions\"><button class=\"approve allow\" type=\"submit\">Approve, and allow this exact {k} for {allow_days} days</button></p></form>",
                        p = escape(project),
                        i = escape(&a.id),
                        v = a.version,
                        k = escape(&kind_words(kind, a.target.as_deref()))
                    ) })
                    .unwrap_or_default(),
            )
        })
        .collect();
    let body = if pending.is_empty() {
        "<p class=\"empty\">Nothing is waiting for you.</p>".to_string()
    } else {
        cards
    };
    let notify = phone_controls(push_key, can_test);
    let notify = if notify.is_empty() {
        notify
    } else {
        format!("<section class=\"card\" id=\"phone\"><h2>This phone</h2>{notify}</section>")
    };
    let ahead_cards: String = ahead
        .iter()
        .map(|board| {
            let p = escape(&board.project);
            let live: String = board
                .standing
                .iter()
                .map(|s| {
                    format!(
                        "<li id=\"standing-{id}\">{k} <code>{what}</code> <span class=\"meta\">by {by}, until {until}</span>\
<form method=\"post\" action=\"/approvals/standing/revoke\" class=\"inline\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"id\" value=\"{id}\">\
<button class=\"deny\" type=\"submit\">Stop</button></form></li>",
                        id = escape(&s.id),
                        k = escape(&kind_words(&s.kind, s.target.as_deref())),
                        what = escape(&s.what),
                        by = escape(&s.granted_by),
                        until = pages::utc((s.expires_at / 1000).max(0) as u64),
                    )
                })
                .collect();
            let offered: String = board
                .suggestions
                .iter()
                .map(|s| {
                    format!(
                        "<li class=\"suggestion\">You approved {k} {n} times in 30 days and never denied it: <code>{what}</code>\
<form method=\"post\" action=\"/approvals/standing\" class=\"inline\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"kind\" value=\"{kind}\">\
<input type=\"hidden\" name=\"target\" value=\"{target}\"><input type=\"hidden\" name=\"what\" value=\"{what}\">\
<button class=\"approve\" type=\"submit\">Approve it automatically for {allow_days} days</button></form></li>",
                        k = escape(&kind_words(&s.kind, s.target.as_deref())),
                        n = s.approved,
                        what = escape(&s.what),
                        kind = escape(&s.kind),
                        target = escape(s.target.as_deref().unwrap_or_default()),
                    )
                })
                .collect();
            format!(
                "<section class=\"card\" id=\"ahead-{p}\"><h2>Approved ahead of time · {p}</h2>{}{}</section>",
                if live.is_empty() {
                    "<p class=\"empty\">Nothing yet: every step here asks you.</p>".to_string()
                } else {
                    format!("<ul class=\"rows\">{live}</ul>")
                },
                if offered.is_empty() {
                    String::new()
                } else {
                    format!("<ul class=\"rows\">{offered}</ul>")
                }
            )
        })
        .collect();
    pages::app(
        StatusCode::OK,
        "Approvals",
        &format!("<h1>Approvals</h1>{flash}{body}{ahead_cards}{notify}"),
        pages::Tab::Approvals,
    )
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
