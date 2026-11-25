//! What the ticket page's forms do (`ticket.rs`): `POST /app/tickets/<project>/<id>/<action>`
//! with `comment`, `assign`, `delegate`, `close`, `undo` or `hand`, each the board calls the
//! MCP tools make, as the person (`Actor::human`), audited as the board tools audit theirs.
//! `delegate` assigns the ticket to an agent by name and leaves that agent a note, so it hears
//! of it on its next call (the inbox banner) and at once when it has a webhook (`wake`).

use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use devagotchi_contracts::BridgeResult;
use serde::Deserialize;

use super::ticket::unknown;
use crate::agenthub::web::approvals::same_origin;
use crate::web::app::{person, words};
use crate::{
    AppState,
    agenthub::{Actor, Board, Patch, workflow::StateKind},
    web::{encode, pages, routes::see_other},
};

#[derive(Deserialize, serde::Serialize)]
pub struct Act {
    #[serde(skip_serializing)]
    request_id: Option<String>,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    body: String,
    /// A comment's kind (`note`, `question`, `handoff`, `decision`, `review`).
    #[serde(default)]
    kind: String,
    /// Who a comment is for: a seat, or an AI by name.
    #[serde(default)]
    to: String,
    /// Undo: the state the ticket was closed from.
    #[serde(default)]
    from: String,
}

/// `POST /app/tickets/<project>/<id>/<action>`: comment, assign, close, undo or hand.
pub async fn act(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((project, id, action)): Path<(String, String, String)>,
    Form(act): Form<Act>,
) -> Response {
    let here = format!("/app/tickets/{}/{}", encode(&project), encode(&id));
    let site = words();
    let copy = &site.messages.app;
    if !same_origin(&headers, &state.hub.config.public_url) {
        return pages::message(
            StatusCode::FORBIDDEN,
            &copy.ticket_unknown,
            &copy.not_from_here,
            None,
        );
    }
    let subject = match person(&state, &headers, &here).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if let Some(refused) = state.hub.waiting_refusal(&subject) {
        return see_other(
            &format!("{here}?done=no&why={}", encode(&refused.message)),
            &[],
        );
    }
    // What the person is on this board decides what they may do from here: every action
    // on this page is a person's (`Actor::human`), so a viewer or an agent's role changes
    // nothing, as the board tools refuse them (`Hub::board_access`).
    let board = match state.hub.board_access(&subject, &project) {
        Ok((board, _, role)) if role.is_person() => board,
        Ok((_, _, role)) => {
            let why = format!(
                "{} is {} on this board; a member or above acts on its tickets",
                subject.id,
                role.as_str()
            );
            return see_other(&format!("{here}?done=no&why={}", encode(&why)), &[]);
        }
        Err(error) if matches!(error.code.as_str(), "unknown_board" | "not_granted") => {
            return unknown();
        }
        Err(error) => {
            eprintln!("devagotchi hub: open ticket action board: {error}");
            return crate::web::routes::unavailable();
        }
    };
    let tool = match action.as_str() {
        "comment" => "board_note",
        "assign" | "delegate" | "close" | "undo" | "hand" => "board_update",
        _ => return unknown(),
    };
    let me = subject.id.clone();
    let ticket = id.clone();
    let addressed = (action == "comment" || action == "delegate")
        .then(|| act.to.trim().to_string())
        .filter(|to| !to.is_empty());
    let wakes = board.clone();
    let done = tokio::task::spawn_blocking(move || {
        let operation = match action.as_str() {
            "comment" => "add_note",
            "delegate" => "delegate",
            "undo" => "undo_close",
            _ => "update_task",
        };
        let requested = act
            .request_id
            .as_deref()
            .map(|key| board.requested(&me, key, operation, &(&ticket, &action, &act)))
            .transpose()?;
        let board = requested.as_ref().unwrap_or(&board);
        let next = apply(board, &Actor::human(&me), &ticket, &action, act)?;
        Ok((next, board.hosted().replayed()))
    })
    .await
    .unwrap_or_else(|_| {
        Err(crate::agenthub::fail(
            "board_store",
            "the board did not answer",
        ))
    });
    if let (Ok((_, false)), Some(to)) = (&done, &addressed) {
        state.hub.wake_recipient(&project, wakes, to);
    }
    let outcome = match &done {
        Ok(_) => "ok".to_string(),
        Err(error) => error.code.clone(),
    };
    // After the store committed: a failed append raises the operator alert and the page
    // still says what happened (G7, D1).
    state
        .hub
        .record_completion(&subject, tool, &project, &id, &outcome)
        .await;
    match done {
        Ok((next, replayed)) => {
            crate::web::app::requests::redirect(&format!("{here}?{next}"), replayed)
        }
        Err(error) => see_other(
            &format!("{here}?done=no&why={}", encode(&error.message)),
            &[],
        ),
    }
}

/// One action on the board as the person; the query the ticket's page is shown with next.
fn apply(
    board: &Board,
    actor: &Actor<'_>,
    id: &str,
    action: &str,
    act: Act,
) -> BridgeResult<String> {
    let workflow = board.workflow()?;
    let update = |patch: Patch| board.update(actor, id, act.version, patch);
    match action {
        "comment" => board
            .add_note(
                actor,
                id,
                &act.kind,
                &act.body,
                Some(act.to.trim()).filter(|to| !to.is_empty()),
                None,
            )
            .map(|_| "done=comment".into()),
        "assign" => update(Patch {
            assignee: Some(Some(actor.id.to_string())),
            ..Patch::default()
        })
        .map(|_| "done=assign".into()),
        "delegate" => {
            let copy = &words().messages.app.delegate;
            let to = act.to.trim();
            if !crate::agenthub::valid_name(to) && !(to.starts_with("@agent:") && to.len() <= 256) {
                return Err(crate::agenthub::fail("invalid", copy.invalid.clone()));
            }
            // The assignee and the note saying so, in one transaction.
            board
                .delegate(actor, id, act.version, to, &copy.note)
                .map(|_| "done=delegate".into())
        }
        "close" => {
            let from = if act.from.is_empty() {
                if act.request_id.is_some() {
                    return Err(crate::agenthub::fail(
                        "invalid",
                        "reload the close form before submitting",
                    ));
                }
                board.get(id)?.ticket.state
            } else {
                act.from.clone()
            };
            // Done when the workflow goes there from here, else its first closed state that it does.
            let to = ["done".to_string()]
                .into_iter()
                .chain(
                    workflow
                        .states
                        .iter()
                        .filter(|s| s.kind == StateKind::Done)
                        .map(|s| s.key.clone()),
                )
                .find(|to| workflow.is_done(to) && workflow.allows(&from, to))
                .unwrap_or_else(|| "done".into());
            update(Patch {
                state: Some(to),
                ..Patch::default()
            })
            .map(|_| format!("done=close&from={}", encode(&from)))
        }
        // Back where it was, or to the queue when the workflow reopens only there, while the
        // window is open by the database's clock (`HostedBoard::undo_close`).
        "undo" => match board.undo_close(actor, id, act.version, &act.from) {
            Ok(_) => Ok("done=undo".into()),
            Err(error) if error.code == "undo_too_late" => Ok("done=late".into()),
            Err(error) => Err(error),
        },
        _ => {
            let queue = workflow
                .lanes
                .first()
                .map(|l| l.queue.clone())
                .unwrap_or(workflow.initial_state.clone());
            update(Patch {
                state: Some(queue),
                assignee: Some(None),
                ..Patch::default()
            })
            .map(|_| "done=hand".into())
        }
    }
}
