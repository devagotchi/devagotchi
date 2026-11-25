//! `get_recent_tool_calls` and `get_usage_stats` through `devagotchi mcp`: the calls of this
//! connection, kept in memory as they are made (the newest 1000), so an AI whose context was
//! cut can see its last steps. Arguments and output are not kept: only the tool, what it
//! acted on, and how it went. Through a hub the same tools read the hub's audit instead.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use devagotchi_contracts::{
    BridgeError, MAX_RECENT_CALLS, RecentCalls, RecentCallsInput, ToolCall, UsageInput, UsageStats,
    UsageTally, describe, utc_rfc3339,
};
use rmcp::model::{CallToolResponse, CallToolResult, JsonObject};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};

use super::BridgeAdapter;

/// Calls kept: the oldest is dropped past this.
const KEPT: usize = 1000;
/// Longest target kept (a long command line is cut).
const TARGET_CHARS: usize = 120;

#[derive(Clone, Default)]
pub(super) struct History(Arc<Mutex<VecDeque<ToolCall>>>);

impl History {
    fn calls(&self) -> std::sync::MutexGuard<'_, VecDeque<ToolCall>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Keep one finished call of `tool` with `arguments`.
    pub(super) fn record(
        &self,
        tool: &str,
        arguments: Option<&JsonObject>,
        answer: &Result<CallToolResponse, rmcp::ErrorData>,
    ) {
        if matches!(tool, "get_recent_tool_calls" | "get_usage_stats") {
            return;
        }
        let call = ToolCall {
            at: utc_rfc3339(std::time::SystemTime::now()),
            tool: tool.into(),
            machine: None,
            target: arguments.and_then(target),
            outcome: outcome(answer),
        };
        let mut calls = self.calls();
        if calls.len() == KEPT {
            calls.pop_front();
        }
        calls.push_back(call);
    }
}

/// What a call acted on, from its arguments: the first of a path, an address, a job, a PID,
/// a setting, a command line.
fn target(arguments: &JsonObject) -> Option<String> {
    let text = |key: &str| match arguments.get(key)? {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    };
    let found = [
        "path",
        "source",
        "url",
        "operation_id",
        "pid",
        "key",
        "command",
    ]
    .into_iter()
    .find_map(text)
    .or_else(|| {
        let paths = arguments.get("paths")?.as_array()?;
        let first = paths.first()?.as_str()?;
        Some(match paths.len() {
            1 => first.to_string(),
            more => format!("{first} and {} more", more - 1),
        })
    })?;
    Some(match found.char_indices().nth(TARGET_CHARS) {
        Some((cut, _)) => format!("{}…", &found[..cut]),
        None => found,
    })
}

/// `ok`, `running` for a command still going, or the error's code.
fn outcome(answer: &Result<CallToolResponse, rmcp::ErrorData>) -> String {
    let result: &CallToolResult = match answer {
        Ok(CallToolResponse::Complete(result)) => result,
        Ok(_) => return "ok".into(),
        Err(_) => return "invalid_request".into(),
    };
    let structured = result.structured_content.as_ref();
    let field = |key: &str| structured.and_then(|value| value.get(key));
    if result.is_error == Some(true) {
        return field("code")
            .and_then(|code| code.as_str())
            .unwrap_or("error")
            .into();
    }
    if field("next").is_some_and(|next| !next.is_null()) {
        return "running".into();
    }
    "ok".into()
}

#[tool_router(router = history_router, vis = "pub(super)")]
impl BridgeAdapter {
    #[tool(
        name = "get_recent_tool_calls",
        description = describe::GET_RECENT_TOOL_CALLS,
        title = "Your recent calls",
        annotations(title = "Your recent calls", read_only_hint = true, open_world_hint = false)
    )]
    async fn get_recent_tool_calls(
        &self,
        Parameters(input): Parameters<RecentCallsInput>,
    ) -> Result<Json<RecentCalls>, Json<BridgeError>> {
        if !(1..=MAX_RECENT_CALLS).contains(&input.max_results) {
            return Err(Json(BridgeError::new(
                "invalid_request",
                format!("max_results must be 1..={MAX_RECENT_CALLS}"),
            )));
        }
        let calls = self.history.calls();
        let matching: Vec<&ToolCall> = calls
            .iter()
            .rev()
            .filter(|call| input.tool.as_ref().is_none_or(|tool| call.tool == *tool))
            .collect();
        Ok(Json(RecentCalls {
            matched: matching.len() as u64,
            calls: matching
                .into_iter()
                .take(input.max_results as usize)
                .cloned()
                .collect(),
        }))
    }

    #[tool(
        name = "get_usage_stats",
        description = describe::GET_USAGE_STATS,
        title = "Your usage",
        annotations(title = "Your usage", read_only_hint = true, open_world_hint = false)
    )]
    async fn get_usage_stats(
        &self,
        Parameters(_): Parameters<UsageInput>,
    ) -> Result<Json<UsageStats>, Json<BridgeError>> {
        let mut tally = UsageTally::default();
        self.history
            .calls()
            .iter()
            .for_each(|call| tally.call(call));
        Ok(Json(tally.finish(
            "this connection's calls, since it started (the newest 1000)",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_the_first_thing_a_call_acted_on() {
        let args = |value: serde_json::Value| value.as_object().unwrap().clone();
        assert_eq!(
            target(&args(serde_json::json!({"path": "/a", "url": "x"}))).as_deref(),
            Some("/a")
        );
        assert_eq!(
            target(&args(serde_json::json!({"pid": 42}))).as_deref(),
            Some("42")
        );
        assert_eq!(
            target(&args(serde_json::json!({"paths": ["/a", "/b", "/c"]}))).as_deref(),
            Some("/a and 2 more")
        );
        let long = "x".repeat(200);
        assert_eq!(
            target(&args(serde_json::json!({"command": long})))
                .unwrap()
                .chars()
                .count(),
            121
        );
        assert_eq!(target(&args(serde_json::json!({"max_results": 5}))), None);
    }
}
