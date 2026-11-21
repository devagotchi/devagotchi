//! Canonical wire types. Serde rejects unknown fields; broker validation enforces bounds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod ai;
pub mod describe;
mod files;
mod grants;
mod history;
mod interactive;
mod search;
mod system;
mod web;
pub use ai::*;
pub use files::*;
pub use grants::*;
pub use history::*;
pub use interactive::*;
pub use search::*;
pub use system::*;
pub use web::*;

pub const API_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_OUTPUT_PAGE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartInput {
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
    /// Who is starting it, set by the transport that carried the request (this machine's
    /// adapter, a hub), never by an AI: whatever arrives here from a tool call is replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetInput {
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

/// Longest an operation may run: a day. Long builds and test runs fit; a job that should
/// outlive a day is a service, not an operation.
pub const MAX_DEADLINE_MS: u64 = 24 * 60 * 60 * 1000;

/// Longest a broker holds one wait open. MCP clients give up on a call after about a
/// minute, so a wait ends well before that and the caller waits again.
pub const MAX_WAIT_MS: u64 = 50_000;

/// Wait until the broker's operations change as a list shows them: one admitted, retired,
/// or moved to another state (output alone is not a change here). For a view that
/// re-lists only when something changed, instead of on a timer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangesInput {
    /// The revision your list reflects (from the last answer). Absent: answer at once with
    /// the current revision. A broker that restarted answers at once, too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_revision: Option<u64>,
    /// Longest to wait, 1..=50000 milliseconds; at the limit the same revision comes back.
    pub timeout_ms: u64,
}

/// The broker's current operations revision.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitInput {
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// Return when output beyond this sequence exists (the cursor you have read up to), as
    /// well as when the operation settles. Without it, return only when it settles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_after: Option<u64>,
    /// Longest to wait, 1..=50000 milliseconds. The answer comes as soon as the operation
    /// changes as asked, without polling; at the limit it comes back unchanged.
    pub timeout_ms: u64,
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListInput {
    /// Number of operations to skip (default 0).
    #[serde(default)]
    pub offset: u64,
    /// Most operations to return, 1..=64 (default 32).
    #[serde(default = "default_list_limit")]
    pub limit: u32,
    /// With a caller: `account` (the default) lists what any of its account's conversations
    /// started, also before a restart (for this machine's own account, everything here);
    /// `mine` only what this conversation or client started. Without a caller: everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ListScope>,
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}
pub const MAX_LIST_LIMIT: u32 = 64;
pub const DEFAULT_LIST_LIMIT: u32 = 32;
pub fn default_list_limit() -> u32 {
    DEFAULT_LIST_LIMIT
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationSummary {
    pub operation_id: String,
    pub state: OperationState,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    /// Bounded display preview only. Execute only the exact argv from the canonical request.
    pub display_command: String,
    /// How an interactive operation takes input; absent for one that takes none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InputMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationsPage {
    pub operations: Vec<OperationSummary>,
    pub next_offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadInput {
    /// The operation ID returned by operation_start or run.
    pub operation_id: String,
    /// Last sequence consumed, starting at zero. Reading never advances another reader.
    #[serde(default)]
    pub cursor: u64,
    /// Maximum raw bytes to return, 1024..=16384. Frames are at most 1024 bytes.
    #[serde(default = "default_page_bytes")]
    pub max_bytes: usize,
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

pub const DEFAULT_PAGE_BYTES: usize = MAX_OUTPUT_PAGE_BYTES;
pub fn default_page_bytes() -> usize {
    DEFAULT_PAGE_BYTES
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
    Unknown,
}

impl OperationState {
    pub fn terminal(&self) -> bool {
        !matches!(self, Self::Accepted | Self::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationView {
    pub operation_id: String,
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub deadline_ms: u64,
    pub state: OperationState,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub exit_code: Option<i32>,
    pub detail: Option<String>,
    /// Root process and its inherited process group only; this is not OS containment.
    pub ownership_scope: String,
    pub cancellation_requested: bool,
    pub output_complete: bool,
    /// Present for an interactive operation: how it takes input and whether it still can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartOutput {
    pub operation: OperationView,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputFrame {
    pub sequence: u64,
    pub stream: String,
    /// Raw process bytes, losslessly encoded. Text below is a UTF-8 convenience view.
    pub bytes_base64: String,
    /// Incrementally decoded per stream; split UTF-8 sequences are retained between chunks.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputPage {
    pub operation_id: String,
    pub frames: Vec<OutputFrame>,
    pub next_cursor: u64,
    /// True when requested output was evicted from this operation's bounded retention.
    pub gap: bool,
    pub earliest_retained_sequence: u64,
    pub latest_sequence: u64,
    pub has_more: bool,
    pub operation: OperationView,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

impl BridgeError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for BridgeError {}

pub type BridgeResult<T> = Result<T, BridgeError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RequestBody {
    OperationStart(StartInput),
    OperationGet(GetInput),
    OperationRead(ReadInput),
    OperationCancel(GetInput),
    OperationList(ListInput),
    OperationWait(WaitInput),
    OperationsChanged(ChangesInput),
    FileRead(FileReadInput),
    FileList(FileListInput),
    FileWrite(FileWriteInput),
    FileEdit(FileEditInput),
    FileInfo(FileInfoInput),
    FileMkdir(DirectoryInput),
    FileMove(FileMoveInput),
    FileSearch(FileSearchInput),
    ConfigGet(ConfigGetInput),
    ConfigSet(ConfigSetInput),
    ProcessList(ProcessListInput),
    ProcessKill(ProcessKillInput),
    UrlRead(UrlReadInput),
    OperationInput(SendInput),
    OperationResize(ResizeInput),
    BrokerCapabilities(CapabilitiesInput),
}

impl RequestBody {
    /// How long the broker may hold this request before answering, beyond ordinary work:
    /// transports (local IPC, the machine link) wait this much longer for the response.
    pub fn held_for(&self) -> std::time::Duration {
        match self {
            Self::OperationWait(WaitInput { timeout_ms, .. })
            | Self::OperationsChanged(ChangesInput { timeout_ms, .. }) => {
                std::time::Duration::from_millis((*timeout_ms).min(MAX_WAIT_MS))
            }
            Self::OperationStart(_)
            | Self::OperationGet(_)
            | Self::OperationRead(_)
            | Self::OperationCancel(_)
            | Self::OperationList(_)
            | Self::FileRead(_)
            | Self::FileList(_)
            | Self::FileWrite(_)
            | Self::FileEdit(_)
            | Self::FileInfo(_)
            | Self::FileMkdir(_)
            | Self::FileMove(_)
            | Self::FileSearch(_)
            | Self::ConfigGet(_)
            | Self::ConfigSet(_)
            | Self::ProcessList(_)
            | Self::ProcessKill(_)
            | Self::UrlRead(_)
            | Self::OperationInput(_)
            | Self::OperationResize(_)
            | Self::BrokerCapabilities(_) => std::time::Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireRequest {
    pub api_version: u32,
    pub request: RequestBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ResponseBody {
    Start(StartOutput),
    Operation(OperationView),
    Output(OutputPage),
    Operations(OperationsPage),
    File(FileContent),
    Listing(FileListing),
    Written(FileWritten),
    Revision(Revision),
    Info(FileInfo),
    InputSent(InputSent),
    Search(SearchPage),
    Config(ConfigView),
    Processes(ProcessPage),
    Killed(ProcessKilled),
    Url(UrlContent),
    Capabilities(BrokerCapabilities),
}

impl ResponseBody {
    /// What kind of answer this is, by name: for an error that says a broker answered with
    /// another kind than the request calls for. Every transport and the desktop name it here.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Start(_) => "start",
            Self::Operation(_) => "operation",
            Self::Output(_) => "output",
            Self::Operations(_) => "operations",
            Self::File(_) => "file",
            Self::Listing(_) => "listing",
            Self::Written(_) => "written",
            Self::Revision(_) => "revision",
            Self::Info(_) => "info",
            Self::InputSent(_) => "input_sent",
            Self::Search(_) => "search",
            Self::Config(_) => "config",
            Self::Processes(_) => "processes",
            Self::Killed(_) => "killed",
            Self::Url(_) => "url",
            Self::Capabilities(_) => "capabilities",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WireResponse {
    Ok(ResponseBody),
    Error(BridgeError),
}

mod link;
pub use link::*;
