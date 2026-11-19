//! `get_recent_tool_calls` and `get_usage_stats`: what an AI has done through Devagotchi, so
//! it can see its own last steps after its context was cut, and how its calls went. Through
//! a hub they come from the hub's audit of every routed call (the caller's own, across
//! sessions); through `devagotchi mcp` from the calls of that connection.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most calls one `get_recent_tool_calls` returns.
pub const MAX_RECENT_CALLS: u32 = 500;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecentCallsInput {
    /// Only calls of this tool, such as `run` or `read_file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Most calls to return, newest first, 1..=500 (default 50).
    #[serde(default = "default_recent_calls")]
    pub max_results: u32,
}

/// Default for omitted recent_calls inputs on either tool surface.
pub const DEFAULT_RECENT_CALLS: u32 = 50;
pub fn default_recent_calls() -> u32 {
    DEFAULT_RECENT_CALLS
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    /// When it was called, RFC 3339 UTC.
    pub at: String,
    pub tool: String,
    /// The machine it went to, through a hub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// What it acted on: a path, a job, a PID, an address, a setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// `ok`, `running` for a job still going, or the error's code.
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecentCalls {
    /// Newest first.
    pub calls: Vec<ToolCall>,
    /// Calls that matched, before `max_results`.
    pub matched: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UsageInput {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tally {
    /// A tool, a machine, or an error code.
    pub name: String,
    pub calls: u64,
    /// Of them, the ones that failed (for tools and machines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UsageStats {
    pub calls: u64,
    pub failed: u64,
    /// The first and the latest call counted, RFC 3339 UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_at: Option<String>,
    /// Per tool, most called first.
    pub tools: Vec<Tally>,
    /// Per machine, through a hub.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub machines: Vec<Tally>,
    /// Why calls failed, by error code, most frequent first.
    pub failures: Vec<Tally>,
    /// What these numbers cover.
    pub scope: String,
}

impl ToolCall {
    pub fn failed(&self) -> bool {
        failed(&self.outcome)
    }
}

/// Whether a call with this outcome failed: anything but `ok`, or `running` for a job.
pub fn failed(outcome: &str) -> bool {
    !matches!(outcome, "ok" | "running")
}

/// Calls counted into [`UsageStats`], one at a time or a group at a time.
#[derive(Debug, Default)]
pub struct UsageTally {
    calls: u64,
    failed: u64,
    first_at: Option<String>,
    last_at: Option<String>,
    tools: BTreeMap<String, (u64, u64)>,
    machines: BTreeMap<String, (u64, u64)>,
    failures: BTreeMap<String, u64>,
}

impl UsageTally {
    /// `count` calls of `tool` to `machine` with `outcome`, made from `first_at` to `last_at`.
    pub fn add(
        &mut self,
        tool: &str,
        machine: Option<&str>,
        outcome: &str,
        count: u64,
        first_at: &str,
        last_at: &str,
    ) {
        let lost = if failed(outcome) { count } else { 0 };
        self.calls += count;
        self.failed += lost;
        if self
            .first_at
            .as_deref()
            .is_none_or(|first| first_at < first)
        {
            self.first_at = Some(first_at.into());
        }
        if self.last_at.as_deref().is_none_or(|last| last_at > last) {
            self.last_at = Some(last_at.into());
        }
        let tool = self.tools.entry(tool.into()).or_default();
        *tool = (tool.0 + count, tool.1 + lost);
        if let Some(machine) = machine {
            let machine = self.machines.entry(machine.into()).or_default();
            *machine = (machine.0 + count, machine.1 + lost);
        }
        if lost > 0 {
            *self.failures.entry(outcome.into()).or_default() += lost;
        }
    }

    pub fn call(&mut self, call: &ToolCall) {
        self.add(
            &call.tool,
            call.machine.as_deref(),
            &call.outcome,
            1,
            &call.at,
            &call.at,
        );
    }

    pub fn finish(self, scope: &str) -> UsageStats {
        let with_failed = |rows: BTreeMap<String, (u64, u64)>| {
            ranked(
                rows.into_iter()
                    .map(|(name, (calls, failed))| (name, calls, Some(failed))),
            )
        };
        UsageStats {
            calls: self.calls,
            failed: self.failed,
            first_at: self.first_at,
            last_at: self.last_at,
            tools: with_failed(self.tools),
            machines: with_failed(self.machines),
            failures: ranked(
                self.failures
                    .into_iter()
                    .map(|(name, calls)| (name, calls, None)),
            ),
            scope: scope.into(),
        }
    }
}

/// Most calls first, then by name.
fn ranked(rows: impl IntoIterator<Item = (String, u64, Option<u64>)>) -> Vec<Tally> {
    let mut rows: Vec<Tally> = rows
        .into_iter()
        .map(|(name, calls, failed)| Tally {
            name,
            calls,
            failed,
        })
        .collect();
    rows.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.name.cmp(&b.name)));
    rows
}

/// `time` as RFC 3339 UTC with milliseconds, such as `2026-11-16T09:30:00.000Z`: the form
/// every call's `at` takes, so they sort as text.
pub fn utc_rfc3339(time: std::time::SystemTime) -> String {
    let now = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (hours, minutes, seconds) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil date from days since epoch (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}.{:03}Z",
        now.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(at: &str, tool: &str, outcome: &str) -> ToolCall {
        ToolCall {
            at: at.into(),
            tool: tool.into(),
            machine: Some("laptop".into()),
            target: None,
            outcome: outcome.into(),
        }
    }

    #[test]
    fn a_time_reads_as_rfc3339_utc() {
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_794_821_400_123);
        assert_eq!(utc_rfc3339(time), "2026-11-16T09:30:00.123Z");
    }

    #[test]
    fn usage_counts_tools_machines_and_failures() {
        let calls = [
            call("2026-11-16T10:00:02.000Z", "run", "ok"),
            call("2026-11-16T10:00:01.000Z", "run", "running"),
            call("2026-11-16T10:00:03.000Z", "read_file", "path_not_found"),
        ];
        let mut tally = UsageTally::default();
        calls.iter().for_each(|call| tally.call(call));
        tally.add(
            "run",
            None,
            "timeout",
            2,
            "2026-11-16T09:00:00.000Z",
            "2026-11-16T09:30:00.000Z",
        );
        let stats = tally.finish("test");
        assert_eq!((stats.calls, stats.failed), (5, 3));
        assert_eq!(stats.first_at.as_deref(), Some("2026-11-16T09:00:00.000Z"));
        assert_eq!(stats.last_at.as_deref(), Some("2026-11-16T10:00:03.000Z"));
        assert_eq!(stats.tools[0].name, "run");
        assert_eq!((stats.tools[0].calls, stats.tools[0].failed), (4, Some(2)));
        assert_eq!(stats.machines[0].failed, Some(1));
        assert_eq!(
            (stats.failures[0].name.as_str(), stats.failures[0].calls),
            ("timeout", 2)
        );
    }
}
