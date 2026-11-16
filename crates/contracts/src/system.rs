//! The machine itself, beside its files and jobs: the broker's settings (`get_config`,
//! `set_config_value`) and the machine's processes (`list_processes`, `kill_process`).
//!
//! Settings that bound an agent (the workspace, the exec policy and the executable grants)
//! are the machine operator's and change only in broker.json; an agent reads them and may
//! change only the limits in [`CHANGEABLE_SETTINGS`]. Processes are the account's, so only a
//! broker that runs what its account can (exec policy `account`) lists or stops them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The settings `set_config_value` changes, live and in broker.json.
pub const CHANGEABLE_SETTINGS: [&str; 3] =
    ["output_limit_bytes", "max_operations", "max_concurrent"];
/// Most processes one `list_processes` returns.
pub const MAX_PROCESS_RESULTS: u32 = 1000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigGetInput {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigSetInput {
    /// One of `output_limit_bytes`, `max_operations`, `max_concurrent`.
    pub key: String,
    /// The new value, a whole number.
    pub value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigView {
    /// The broker.json this broker was started from, when it was started from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_file: Option<String>,
    /// Where a relative path and a job without `cwd` start (canonical).
    pub workspace: String,
    /// `account`: anything this OS account can run. `granted`: only `allowed_executables`.
    pub exec_policy: String,
    /// The programs a `granted` broker runs (canonical paths).
    pub allowed_executables: Vec<String>,
    /// Output kept per job, in bytes.
    pub output_limit_bytes: u64,
    /// Jobs kept, running and finished; the oldest finished one is retired past this.
    pub max_operations: u64,
    /// Jobs running at once.
    pub max_concurrent: u64,
    /// The settings `set_config_value` changes.
    pub changeable: Vec<String>,
    /// Jobs held now, and how many of them are running.
    pub operations_held: u64,
    pub operations_running: u64,
    /// `linux` or `windows`, and the CPU architecture.
    pub os: String,
    pub arch: String,
    /// The OS account the broker runs as, and its home directory (what `~` means).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// The broker's build.
    pub version: String,
    /// What the last change did, for `set_config_value`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessListInput {
    /// Only processes whose name or command line contains this (case ignored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Most processes to return, 1..=1000 (default 200), busiest first.
    #[serde(default = "default_process_results")]
    pub max_results: u32,
}

fn default_process_results() -> u32 {
    200
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessInfo {
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<u32>,
    pub name: String,
    /// The command line, cut when very long.
    pub command: String,
    /// Share of one CPU over a short sample (100 is one whole core).
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    /// When it started, ms since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    /// The OS account it runs as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// The Devagotchi job this process belongs to, when it is one of this broker's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// This broker, a process it runs under, or another Devagotchi process (its link, an
    /// adapter): stopping it would cut this connection, so kill_process refuses it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub broker: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessPage {
    pub processes: Vec<ProcessInfo>,
    /// Processes that matched, before `max_results`.
    pub matched: u64,
    /// Processes on the machine.
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessKillInput {
    pub pid: u32,
    /// Kill at once (SIGKILL) instead of asking it to stop (SIGTERM). On Windows a process is
    /// always ended at once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessKilled {
    pub pid: u32,
    pub name: String,
    /// `terminate` or `kill`.
    pub signal: String,
    /// Whether it was gone within a few seconds.
    pub exited: bool,
}

/// What each tool says, the same through `devagotchi mcp` and through a hub.
pub mod describe {
    pub const GET_CONFIG: &str = "The machine's broker settings and facts: its workspace (where relative paths and jobs start), exec policy (`account` runs what its OS account can; `granted` only allowed_executables), the limits on output kept per job, jobs kept and jobs running at once, which of them set_config_value changes, the OS and account it runs as, its home (what `~` means) and its build.";
    pub const SET_CONFIG_VALUE: &str = "Change one of the broker's limits while it runs: output_limit_bytes (output kept per job, 1024 to 16777216), max_operations (jobs kept, 1 to 4096) or max_concurrent (jobs running at once, 1 to 64, no more than max_operations). It applies at once and is saved to the broker's broker.json. The workspace, exec policy and executable grants are the machine operator's and change only there.";
    pub const LIST_PROCESSES: &str = "List the machine's processes, busiest first: PID, parent, name, command line, CPU (100 is one core), memory, start time, account, and the Devagotchi job a process belongs to (`operation_id`). `broker: true` marks Devagotchi's own processes and what the broker runs under. filter narrows by name or command line. Only on a broker that runs what its account can.";
    pub const KILL_PROCESS: &str = "Stop a process by PID: it is asked to stop (SIGTERM) unless force (SIGKILL); on Windows it is ended at once. Says whether it was gone within 3 seconds. A job's own process stops with operation_cancel instead, and Devagotchi's own processes (the broker, its link, an adapter, and what they run under) are never stopped. Only on a broker that runs what its account can.";
}
