//! `get_recent_tool_calls` and `get_usage_stats` through a hub: the caller's own calls, read
//! from the hub's audit of every routed call (`ops::audit`), so they reach across sessions
//! and AI clients signed in as the same subject. Only the caller's own: another subject's
//! calls are never shown, and a name the caller gave that is none of its machines (another's
//! machine, a board) is never counted as a machine.

use devagotchi_contracts::{
    BridgeError, MAX_RECENT_CALLS, RecentCalls, ToolCall, UsageInput, UsageStats, UsageTally,
    describe,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::Hub;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecentCallsInput {
    /// Only calls of this tool, such as `run` or `read_file`.
    #[serde(default)]
    pub tool: Option<String>,
    /// Only calls to this machine, by its name from `machines`.
    #[serde(default)]
    pub machine: Option<String>,
    /// Most calls to return, newest first, 1 to 500 (default 50).
    #[serde(default)]
    pub max_results: Option<u32>,
}

/// An audit row's `broker` is the name the call gave: one of the caller's machines, or a
/// board's project, or a name that reached nothing. Only the caller's own machines are shown
/// as machines; any other name is part of what the call acted on.
fn place(broker: String, target: String, machines: &[String]) -> (Option<String>, Option<String>) {
    if machines.contains(&broker) {
        return (
            Some(broker),
            Some(target).filter(|target| !target.is_empty()),
        );
    }
    let named: Vec<String> = [broker, target]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    (
        None,
        Some(named.join(" ")).filter(|target| !target.is_empty()),
    )
}

#[tool_router(router = history_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "get_recent_tool_calls",
        description = describe::GET_RECENT_TOOL_CALLS,
        title = "Your recent calls",
        annotations(title = "Your recent calls", read_only_hint = true, open_world_hint = false)
    )]
    async fn get_recent_tool_calls(
        &self,
        Parameters(input): Parameters<RecentCallsInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<RecentCalls>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let most = input.max_results.unwrap_or(50);
        if !(1..=MAX_RECENT_CALLS).contains(&most) {
            return Err(Json(BridgeError::new(
                "invalid_request",
                format!("max_results must be 1..={MAX_RECENT_CALLS}"),
            )));
        }
        let (rows, matched) = self
            .audit
            .calls_of(
                &subject.id,
                input.tool.as_deref(),
                input.machine.as_deref(),
                most,
            )
            .await
            .map_err(Json)?;
        let machines = self.machine_names(&subject).await;
        let calls = rows
            .into_iter()
            .map(|row| {
                let (machine, target) = place(row.broker, row.operation_id, &machines);
                ToolCall {
                    at: row.at,
                    tool: row.tool,
                    machine,
                    target,
                    outcome: row.outcome,
                }
            })
            .collect();
        Ok(Json(RecentCalls { calls, matched }))
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
        context: RequestContext<RoleServer>,
    ) -> Result<Json<UsageStats>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let rows = self.audit.usage_of(&subject.id).await.map_err(Json)?;
        let machines = self.machine_names(&subject).await;
        let mut tally = UsageTally::default();
        for row in &rows {
            let machine =
                Some(row.broker.as_str()).filter(|name| machines.iter().any(|own| own == name));
            tally.add(
                &row.tool,
                machine,
                &row.outcome,
                row.calls,
                &row.first_at,
                &row.last_at,
            );
        }
        Ok(Json(tally.finish(
            "every call you made through this hub, from every session and AI client signed in as you",
        )))
    }
}
