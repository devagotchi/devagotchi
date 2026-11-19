//! Flat, closed machine operation inputs; fields mirror their broker contracts.
use devagotchi_contracts::*;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// A caller-created retry key. The same key and inputs return the same operation.
    pub idempotency_key: String,
    /// The program to run, with `args`: a name on PATH (`python3`, `cargo`), or a path,
    /// absolute or relative to `cwd`. A script runs with its interpreter (its `#!` line, or
    /// its extension: `.py` with Python, `.ps1` with PowerShell, `.sh` with bash, `.js` with
    /// node), never through a file association; the operation's detail says what ran and why.
    /// On a broker that runs only operator-granted programs the program must be granted.
    /// Leave empty when `command` is given.
    #[serde(default)]
    pub executable: String,
    /// Exact argument vector. No shell parsing, interpolation, or added command text.
    #[serde(default)]
    pub args: Vec<String>,
    /// A command line for the machine's own shell (bash or sh on Linux, Windows PowerShell
    /// on Windows), accepted by a broker that runs commands as its account. Give either
    /// this or `executable` with `args`, not both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Working directory (absolute, `~/...`, or relative to the workspace); defaults to the
    /// workspace. A broker that runs only
    /// granted programs keeps it within the workspace.
    pub cwd: Option<String>,
    /// Execution deadline, not a client wait timeout. 1..=[`MAX_DEADLINE_MS`] milliseconds.
    pub deadline_ms: u64,
    /// Keep the program's input open for `operation_input`: on a pseudo-terminal (`terminal`,
    /// the default: prompts, REPLs, sign-ins, Ctrl-C) or on a plain pipe (`pipe`). Absent:
    /// the program reads nothing (its stdin is the null device), as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveInput>,
    /// One pipe for stdout and stderr, so the output keeps the order the program wrote it
    /// (as a terminal shows it); every frame is then `stdout`. `run` sets it. Ignored for an
    /// interactive operation, whose terminal already merges them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub combined_output: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// Return when output beyond this sequence exists (the cursor you have read up to), as
    /// well as when the operation settles. Without it, return only when it settles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_after: Option<u64>,
    /// Longest to wait, 1..=50000 milliseconds. The answer comes as soon as the operation
    /// changes as asked, without polling; at the limit it comes back unchanged.
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// Last sequence consumed, starting at zero. Reading never advances another reader.
    #[serde(default)]
    pub cursor: u64,
    /// Maximum raw bytes to return, 1024..=16384. Frames are at most 1024 bytes.
    #[serde(default = "devagotchi_contracts::default_page_bytes")]
    pub max_bytes: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Number of operations to skip (default 0).
    #[serde(default)]
    pub offset: u64,
    /// Most operations to return, 1..=64 (default 32).
    #[serde(default = "devagotchi_contracts::default_list_limit")]
    pub limit: u32,
    /// With a caller: `account` (the default) lists what any of its account's conversations
    /// started, also before a restart (for this machine's own account, everything here);
    /// `mine` only what this conversation or client started. Without a caller: everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ListScope>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// Text to type, exactly as given (UTF-8, at most 16 KiB). Nothing is added: end a line
    /// with the `enter` key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Keys pressed after the text, in order (at most 64).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<Key>,
    /// Drive an operation another conversation or client of the same account started, or
    /// take the keyboard from the one holding it. Never reaches another account's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub take_over: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResizeRouted {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// New width, 2..=500 columns.
    pub cols: u16,
    /// New height, 1..=200 rows.
    pub rows: u16,
    /// As for `operation_input`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub take_over: bool,
}
