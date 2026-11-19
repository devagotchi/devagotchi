//! The machine itself, beside its files and jobs: the broker's settings (`get_config`,
//! `set_config_value`) and the machine's processes (`list_processes`, `kill_process`).
//!
//! Settings that bound an agent (the workspace, the exec policy and the executable grants)
//! are the machine operator's and change only in broker.json; an agent reads them and may
//! change only the limits in [`CHANGEABLE_SETTINGS`]. Processes are the account's, so only a
//! broker that runs what its account can (exec policy `account`) lists or stops them.
//!
//! `broker_capabilities` asks a live broker what it is and what it serves, without paths,
//! grants, environment or secrets: a link that is connected is not yet a broker that answers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The settings `set_config_value` changes, live and in broker.json.
pub const CHANGEABLE_SETTINGS: [&str; 3] =
    ["output_limit_bytes", "max_operations", "max_concurrent"];
/// Most processes one `list_processes` returns.
pub const MAX_PROCESS_RESULTS: u32 = 1000;
/// What `output_limit_bytes` may be set to: bytes of output kept per operation.
pub const OUTPUT_LIMIT_BYTES_RANGE: std::ops::RangeInclusive<usize> = 1024..=16 * 1024 * 1024;
/// What `max_operations` may be set to: operations the broker keeps, running or finished.
pub const MAX_OPERATIONS_RANGE: std::ops::RangeInclusive<usize> = 1..=4096;
/// What `max_concurrent` may be set to: operations running at once.
pub const MAX_CONCURRENT_RANGE: std::ops::RangeInclusive<usize> = 1..=64;

/// Every request method this build's broker serves, in `RequestBody`'s order: what
/// `broker_capabilities` starts from before a broker's policy removes what it always refuses.
pub const REQUEST_METHODS: [&str; 23] = [
    "operation_start",
    "operation_get",
    "operation_read",
    "operation_cancel",
    "operation_list",
    "operation_wait",
    "operations_changed",
    "file_read",
    "file_list",
    "file_write",
    "file_edit",
    "file_info",
    "file_mkdir",
    "file_move",
    "file_search",
    "config_get",
    "config_set",
    "process_list",
    "process_kill",
    "url_read",
    "operation_input",
    "operation_resize",
    "broker_capabilities",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigGetInput {}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesInput {}

/// What a live broker is and what it serves, read from its policy alone. Never a local path,
/// a grant, an environment value or a credential, and no permission to run anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrokerCapabilities {
    /// The wire API version the broker speaks.
    pub api_version: u32,
    /// The broker's build.
    pub version: String,
    /// `linux` or `windows`, and the CPU architecture.
    pub os: String,
    pub arch: String,
    /// `account`: anything this OS account can run. `granted`: only the operator's grants.
    pub exec_policy: String,
    /// Whether a `command` line runs on the machine's shell (an `account` broker only).
    pub shell_commands: bool,
    /// `account`: files and jobs anywhere the account reaches. `workspace`: only inside the
    /// broker's workspace.
    pub workspace_scope: String,
    /// The request methods this broker answers under its policy; one it always refuses (a
    /// `granted` broker's processes, settings changes and web reads) is left out. Listed is
    /// not granted: the broker still checks each request.
    pub supported_requests: Vec<String>,
}

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

/// Default for omitted process_results inputs on either tool surface.
pub const DEFAULT_PROCESS_RESULTS: u32 = 200;
pub fn default_process_results() -> u32 {
    DEFAULT_PROCESS_RESULTS
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
    /// The process ID, from list_processes.
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

#[cfg(test)]
mod tests;
