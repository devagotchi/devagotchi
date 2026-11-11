//! `/app/tickets/<project>/<id>`: one ticket — title, state, priority, labels, who holds it
//! and who it is assigned to, its description, notes and history — and what a person does
//! with it from the phone: **Comment**, **Assign to me**, **Close** (at once, with Undo for
//! 10 s) and **Hand to AI** (back to the board's ready queue, unassigned). Each action is
//! the board call the MCP tools make (`Board::add_note`, `Board::update`), with the person
//! as the actor, audited as they audit it (`ticket_actions.rs`); every form posts from this
//! hub's pages only.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use devagotchi_contracts::BridgeResult;
use serde::Deserialize;

use super::{open_board, person, words};
use crate::{
    AppState,
    site::Ctx,
    web::{
        encode,
        pages::{self, Tab, escape},
    },
};

/// How long Close can be undone.
pub const UNDO_MS: i64 = 10_000;

#[derive(Deserialize, Default)]
pub struct Shown {
    done: Option<String>,
    why: Option<String>,
    /// After Close: the state it was closed from, for Undo.
    from: Option<String>,
}

pub(super) fn unknown() -> Response {
    let site = words();
    let copy = &site.messages.app;
    pages::app_page(
        StatusCode::NOT_FOUND,
        &copy.ticket_unknown,
        &format!(
            "<h1>{}</h1><p class=\"note\">{}</p>",
            escape(&copy.ticket_unknown),
            escape(&copy.ticket_unknown_text)
        ),
        Tab::Boards,
    )
}

pub(super) fn now_ms() -> i64 {
    (crate::board::system_clock())()
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
    Query(shown): Query<Shown>,
) -> Response {
    let here = format!("/app/tickets/{}/{}", encode(&project), encode(&id));
    let subject = match person(&state, &headers, &here).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let board = match open_board(&state, &subject, &project) {
        Ok(Some(board)) => board,
        Ok(None) => return unknown(),
        Err(error) => {
            eprintln!("devagotchi hub: open ticket board: {error}");
            return crate::web::routes::unavailable();
        }
    };
    let wanted = id.clone();
    let read = tokio::task::spawn_blocking(move || {
        let view = board.get(&wanted)?;
        let source = board.issue_source(&wanted)?;
        // The ticket's own history: the board's chain, read a page at a time.
        let mut events = Vec::new();
        let mut after = 0;
        loop {
            let page = board.events(after, 500)?;
            let Some(last) = page.last() else { break };
            after = last.seq;
            events.extend(
                page.into_iter()
                    .filter(|e| e.ticket.as_deref() == Some(wanted.as_str())),
            );
        }
        BridgeResult::Ok((view, events, source))
    })
    .await;
    let (view, events, source) = match read {
        Ok(Ok(value)) => value,
        Ok(Err(error)) if error.code == "not_found" => return unknown(),
        other => {
            eprintln!("devagotchi hub: read ticket: {other:?}");
            return crate::web::routes::unavailable();
        }
    };
    let site = words();
    let copy = &site.messages.app;
    let ticket = &view.ticket;
    if ticket.kind == crate::board::approval::KIND {
        return unknown();
    }
    let me = subject.id.as_str();
    let who = |id: Option<&str>| match id {
        Some(id) if id == me => escape(&copy.you),
        Some(id) => escape(id),
        None => escape(&copy.nobody),
    };
    let flash = match shown.done.as_deref() {
        Some("comment") => escape(&copy.done_comment),
        Some("assign") => escape(&copy.done_assign),
        Some("close") => escape(&copy.done_close),
        Some("undo") => escape(&copy.done_undo),
        Some("hand") => escape(&copy.done_hand),
        Some("late") => escape(&copy.undo_late),
        Some("no") => Ctx::new(&site)
            .with("why", shown.why.unwrap_or_default())
            .inline(&copy.not_changed),
        _ => String::new(),
    };
    let form = |action: &str, inner: &str| {
        format!(
            "<form method=\"post\" action=\"{here}/{action}\"><input type=\"hidden\" name=\"version\" \
value=\"{}\">{inner}</form>",
            ticket.version
        )
    };
    // Undo shows for what is left of its 10 s; app.js takes it away when they run out.
    let undo = match (shown.done.as_deref(), ticket.closed_at) {
        (Some("close"), Some(closed)) if now_ms() < closed + UNDO_MS => format!(
            "<div class=\"undo\" id=\"undo\" data-until=\"{}\">{}</div>",
            closed + UNDO_MS,
            form(
                "undo",
                &format!(
                    "<input type=\"hidden\" name=\"from\" value=\"{}\"><button type=\"submit\">{}</button>",
                    escape(shown.from.as_deref().unwrap_or_default()),
                    escape(&copy.undo)
                )
            )
        ),
        _ => String::new(),
    };
    let flash = if flash.is_empty() {
        String::new()
    } else {
        format!("<p class=\"note\" id=\"flash\" role=\"status\">{flash}</p>")
    };
    let me_copy = &site.messages.me;
    let sections = [
        ("question", &me_copy.notes_questions),
        ("handoff", &me_copy.notes_handoffs),
        ("decision", &me_copy.notes_decisions),
        ("review", &me_copy.notes_reviews),
        ("note", &me_copy.notes_general),
    ];
    let known = |kind: &str| sections.iter().any(|(k, _)| *k == kind && *k != "note");
    let notes: String = sections
        .iter()
        .filter_map(|(kind, title)| {
            let rows: String = view
                .notes
                .iter()
                .filter(|note| {
                    if *kind == "note" {
                        !known(&note.kind)
                    } else {
                        note.kind == *kind
                    }
                })
                .map(|note| {
                    let addressed = note
                        .to_principal
                        .as_deref()
                        .map(|to| {
                            format!(
                                " · <span class=\"note-for\">{}</span>",
                                crate::site::Ctx::new(&site)
                                    .with("name", to.to_string())
                                    .inline(&me_copy.note_for)
                            )
                        })
                        .unwrap_or_default();
                    format!(
                        "<li><span class=\"meta\">{} · {}{addressed}</span><p class=\"body\">{}</p></li>",
                        who(Some(&note.author)),
                        pages::ago((note.created_at / 1000).max(0) as u64),
                        escape(&note.body)
                    )
                })
                .collect();
            (!rows.is_empty()).then(|| {
                format!(
                    "<section class=\"note-section\" id=\"notes-{kind}\"><h3>{}</h3><ul class=\"rows\">{rows}</ul></section>",
                    escape(title)
                )
            })
        })
        .collect();
    let notes = if notes.is_empty() {
        format!("<p class=\"empty\">{}</p>", escape(&copy.notes_empty))
    } else {
        format!("<div id=\"notes\">{notes}</div>")
    };
    let history: String = events
        .iter()
        .rev()
        .take(50)
        .map(|e| {
            format!(
                "<li><span class=\"meta\">{} · {}</span> {}</li>",
                pages::utc((e.at / 1000).max(0) as u64),
                who(Some(&e.actor)),
                escape(&e.action)
            )
        })
        .collect();
    let actions = format!(
        "<div class=\"ticket-actions\">{}{}{}</div>",
        form(
            "assign",
            &format!(
                "<button type=\"submit\" id=\"assign\">{}</button>",
                escape(&copy.assign_me)
            )
        ),
        form(
            "close",
            &format!(
                "<button class=\"deny\" type=\"submit\" id=\"close\">{}</button>",
                escape(&copy.close)
            )
        ),
        form(
            "hand",
            &format!(
                "<button type=\"submit\" id=\"hand\">{}</button>",
                escape(&copy.hand_to_ai)
            )
        ),
    );
    let comment = form(
        "comment",
        &format!(
            "<label>{}<textarea name=\"body\" id=\"comment\" required maxlength=\"20000\" rows=\"3\"></textarea>\
</label><div class=\"note-fields\"><label>{}<select name=\"kind\" id=\"note-kind\">{}</select></label>\
<label>{}<input type=\"text\" name=\"to\" id=\"note-to\" maxlength=\"200\" autocapitalize=\"none\" autocorrect=\"off\" \
spellcheck=\"false\" placeholder=\"{}\"></label></div>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">{}</button></p>",
            escape(&copy.comment_label),
            escape(&site.messages.me.note_kind),
            [
                ("note", &site.messages.me.notes_general),
                ("question", &site.messages.me.notes_questions),
                ("handoff", &site.messages.me.notes_handoffs),
                ("decision", &site.messages.me.notes_decisions),
                ("review", &site.messages.me.notes_reviews),
            ]
            .iter()
            .map(|(value, label)| format!("<option value=\"{value}\">{}</option>", escape(label)))
            .collect::<String>(),
            escape(&site.messages.me.note_to),
            escape(&site.messages.me.note_to_hint),
            escape(&copy.comment_button)
        ),
    );
    // Where it belongs, as one line (machine · environment · repo · agent and session).
    let anchors = ticket
        .anchor_line()
        .map(|line| {
            format!(
                "<dt>{}</dt><dd id=\"anchors\">{}</dd>",
                escape(&copy.ticket_anchors),
                escape(&line)
            )
        })
        .unwrap_or_default();
    let source = source
        .map(|source| {
            let finalization = match source.finalization() {
                "pending" => &copy.issue_finalization_pending,
                "needs_reconciliation" => &copy.issue_finalization_reconcile,
                _ => &copy.issue_finalization_none,
            };
            format!(
                "<section class=\"card\" id=\"issue-source\"><h2>{}</h2>\
<p><a href=\"{}\" rel=\"noreferrer\">{} #{}</a></p>\
<dl class=\"facts\"><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{} / {}</dd>\
<dt>{}</dt><dd id=\"issue-finalization\">{}</dd></dl></section>",
                escape(&copy.issue_source),
                escape(&source.issue_url),
                escape(&source.repository),
                source.issue_number,
                escape(&copy.issue_source_state),
                escape(&source.source_state),
                escape(&copy.issue_source_revision),
                source.source_revision,
                source.epoch,
                escape(&copy.issue_finalization),
                escape(finalization)
            )
        })
        .unwrap_or_default();
    let body = format!(
        "<p class=\"meta\"><a href=\"/app/boards/{}\">{}</a> · {}</p><h1>{}</h1>{flash}{undo}\
<section class=\"card\" id=\"ticket\"><dl class=\"facts\"><dt>{}</dt><dd id=\"state\">{}</dd>\
<dt>{}</dt><dd>P{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd id=\"assignee\">{}</dd><dt>{}</dt><dd>{}</dd>{anchors}</dl>\
<pre class=\"what\">{}</pre>{actions}</section>{source}<h2>{}</h2>{notes}{comment}<h2>{}</h2>\
<ul class=\"rows\" id=\"history\">{history}</ul>",
        encode(&project),
        escape(&project),
        escape(&ticket.id),
        escape(&ticket.title),
        escape(&copy.ticket_state),
        escape(&ticket.state),
        escape(&copy.ticket_priority),
        ticket.priority,
        escape(&copy.ticket_labels),
        escape(&ticket.labels.join(", ")),
        escape(&copy.ticket_assignee),
        who(ticket.assignee.as_deref()),
        escape(&copy.ticket_holder),
        who(ticket.holder.as_deref()),
        escape(&ticket.description),
        escape(&copy.notes_title),
        escape(&copy.events_title),
    );
    let mut response = pages::app_page(StatusCode::OK, &ticket.title, &body, Tab::Boards);
    // Its forms post here: the browser sends a POST's true Origin only under `same-origin`.
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}
