//! How a board tool says no: the error an agent reads, with the call to make next.

use devagotchi_contracts::BridgeError;
use rmcp::{Json, RoleServer, service::RequestContext};
use schemars::JsonSchema;
use serde::Serialize;

/// A refused board call: the store's stable code, its message, and what to call next.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BoardError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    pub next: String,
}

fn next_for(code: &str, id: Option<&str>) -> String {
    let id = id.unwrap_or("<id>");
    match code {
        "not_offered" => "whoami lists the boards this hub serves you; ask its operator for a grant".to_string(),
        "name_taken" => "whoami lists that shared board; use it, or board_bind another name".to_string(),
        "plan_limit" => "whoami lists the project you hold; work in it, or ask the operator for more".to_string(),
        "stale_fence" | "lease_expired" | "not_holder" => {
            format!("board_get {id} to see who holds it; board_claim {id} to take it if it is free")
        }
        "held_by_other" => format!("board_get {id}; wait, or board_claim with no id for another ticket"),
        "version_conflict" => format!("board_get {id} and retry board_update with the current version"),
        "wip_limit" => "board_complete, board_handoff or board_release one of the tickets you hold first".into(),
        "blocked" => format!("board_get {id}: finish or hand off the blockers it lists"),
        "distinct_required" => "someone else must take this lane; leave it for them".into(),
        "not_claimable" => format!("board_get {id}: only a lane queue is claimable; board_update moves it along the workflow"),
        "use_lease_ops" => "use board_claim / board_complete / board_handoff / board_release; held tickets do not move by board_update".into(),
        "transition_not_allowed" => "board_prime names the states; the workflow allows only its transitions".into(),
        "humans_only" => "ask the person who owns this board".into(),
        "not_owner" => "only the account that owns this board, a person granted on it, or that account's AI changes its settings; ask them".into(),
        "not_found" => "board_list with open_only to see the ids on this board".into(),
        "not_granted" | "unknown_board" => "whoami lists the boards your identity may work on".into(),
        "no_board" => "the hub's operator must initialise or fix this board; nothing an agent can do".into(),
        "nothing_ready" => "board_create what you found, or wait with board_wait".into(),
        "unknown_environment" => "pass one of the board's environments the message lists, or leave environment out".into(),
        _ => "board_prime to re-orient".into(),
    }
}

/// `given` unless it is absent or blank; then what the hub knows, if anything.
pub(super) fn given_or(
    given: Option<String>,
    known: impl FnOnce() -> Option<String>,
) -> Option<String> {
    match given {
        Some(value) if !value.trim().is_empty() => Some(value),
        _ => known(),
    }
}

/// The MCP session id of this connection, when the transport has one.
pub(super) fn mcp_session(context: &RequestContext<RoleServer>) -> Option<String> {
    context
        .extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.headers.get("mcp-session-id"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn refuse(error: BridgeError, ticket: Option<&str>) -> Json<BoardError> {
    let next = match error.code.as_str() {
        // The store's message ends with the board's list: name it in `next` too.
        // What is wrong and how to give it, as the store said: that is the hint.
        "invalid_settings" => format!("{}, then call board_settings again", error.message),
        "unknown_environment" => match error.message.rsplit_once(": ") {
            Some((_, list)) => {
                format!("pass one of this board's environments ({list}), or leave environment out")
            }
            None => next_for(&error.code, ticket),
        },
        _ => next_for(&error.code, ticket),
    };
    Json(BoardError {
        next,
        code: error.code,
        message: error.message,
        ticket: ticket.map(str::to_string),
    })
}
