//! Canonical wire types. Serde rejects unknown fields; broker validation enforces bounds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod files;
mod interactive;
pub use files::*;
pub use interactive::*;

pub const API_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_OUTPUT_PAGE_BYTES: usize = 16 * 1024;

/// DG-CAPTURE-1. A harness identifies the adapter, not the model vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AiHarness {
    ClaudeCode,
    Codex,
    Copilot,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AiEventKind {
    SessionStart,
    SessionEnd,
    TurnEnd,
    UserPrompt,
    ToolBefore,
    ToolAfter,
    Error,
    Checkpoint,
    TranscriptRecord,
}

/// Private local archive input, not a hosted-board mutation or a training grant.
/// Runtime validation additionally enforces bounds and policy consent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AiCaptureEvent {
    pub schema_version: u32,
    /// Stable caller retry key within (harness, session). Changed replay is rejected.
    pub event_id: String,
    pub session_id: String,
    pub harness: AiHarness,
    pub kind: AiEventKind,
    pub model: Option<String>,
    pub issue: Option<String>,
    /// Native opaque hook/transcript payload. Requires separate content consent.
    pub content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartInput {
    /// A caller-created retry key. The same key and inputs return the same operation.
    pub idempotency_key: String,
    /// Absolute path of the program to run, with `args`. On a broker that runs only
    /// operator-granted programs it must be granted. Leave empty when `command` is given.
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
    /// Absolute working directory; defaults to the workspace. A broker that runs only
    /// granted programs keeps it within the workspace.
    pub cwd: Option<String>,
    /// Execution deadline, not a client wait timeout. 1..=300000 milliseconds.
    pub deadline_ms: u64,
    /// Keep the program's input open for `operation_input`: on a pseudo-terminal (`terminal`,
    /// the default: prompts, REPLs, sign-ins, Ctrl-C) or on a plain pipe (`pipe`). Absent:
    /// the program reads nothing (its stdin is the null device), as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveInput>,
    /// Who is starting it, set by the transport that carried the request (this machine's
    /// adapter, a hub), never by an AI: whatever arrives here from a tool call is replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetInput {
    pub operation_id: String,
}

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
    pub operation_id: String,
    /// Return when output beyond this sequence exists (the cursor you have read up to), as
    /// well as when the operation settles. Without it, return only when it settles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_after: Option<u64>,
    /// Longest to wait, 1..=50000 milliseconds. The answer comes as soon as the operation
    /// changes as asked, without polling; at the limit it comes back unchanged.
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListInput {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_list_limit")]
    pub limit: u32,
    /// With a caller: `mine` (the default) lists what this conversation or client started;
    /// `account` lists what any of its account's conversations started (and, for this
    /// machine's own account, everything here). Without a caller (the inspector): everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ListScope>,
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}
fn default_list_limit() -> u32 {
    32
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
    pub operation_id: String,
    /// Last sequence consumed, starting at zero. Reading never advances another reader.
    #[serde(default)]
    pub cursor: u64,
    /// Maximum raw bytes to return, 1024..=16384. Frames are at most 1024 bytes.
    #[serde(default = "default_page_bytes")]
    pub max_bytes: usize,
}

fn default_page_bytes() -> usize {
    MAX_OUTPUT_PAGE_BYTES
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
    OperationInput(SendInput),
    OperationResize(ResizeInput),
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
            _ => std::time::Duration::ZERO,
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

/// The machine link (plan N20): a broker on another machine is reached through one
/// outbound WebSocket that its `devagotchi-link` holds open to the hub. These frames
/// are the whole protocol. A `call` carries exactly one broker request, so the
/// broker's own validation and executable grants still decide what runs.
pub const LINK_PROTOCOL_VERSION: u32 = 1;
/// Largest WebSocket message either side accepts.
pub const MAX_LINK_MESSAGE_BYTES: usize = 1024 * 1024;

/// Hub to device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HubFrame {
    /// The first frame on every connection. The device answers with a signed `hello`.
    Challenge {
        nonce: String,
        hub: String,
    },
    Welcome {
        device: String,
    },
    Refused {
        code: String,
        message: String,
    },
    Call {
        id: u64,
        request: WireRequest,
    },
    /// Update this machine to the hub's build `version`: wait for its running jobs unless
    /// `force`, then download, check and install it. The hub sends it only to a device
    /// whose hello carried `build`; an older device would refuse an unknown frame.
    Update {
        version: String,
        force: bool,
    },
}

/// Device to hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkFrame {
    /// Proves the device key over the challenge. `enroll_code` is present only on the
    /// first connection, when the hub binds this key to the device name.
    Hello {
        version: u32,
        device: String,
        /// Ed25519 public key, 32 bytes, hex.
        public_key: String,
        /// Ed25519 signature over `link_signing_message`, 64 bytes, hex.
        signature: String,
        #[serde(default)]
        enroll_code: Option<String>,
        /// The build this device runs (`devagotchi --version`'s name: `main-<sha7>`, a
        /// release version, or `dev`). An older device leaves it out.
        #[serde(default)]
        build: Option<String>,
    },
    Result {
        id: u64,
        response: WireResponse,
    },
    Heartbeat,
    /// Where an update the hub asked for stands: `waiting_for_jobs`, `downloading`,
    /// `installing`, `done` or `failed`, with `detail` saying why when it failed.
    UpdateStatus {
        state: String,
        #[serde(default)]
        detail: Option<String>,
    },
}

/// The states a [`LinkFrame::UpdateStatus`] reports, in the order an update passes them.
pub const UPDATE_STATES: [&str; 5] = [
    "waiting_for_jobs",
    "downloading",
    "installing",
    "done",
    "failed",
];

/// The exact bytes a device signs: protocol, the hub's public URL, the device name and
/// the hub's nonce, so a signature cannot be replayed to another hub, as another
/// device, or on another connection.
pub fn link_signing_message(hub: &str, device: &str, nonce: &str) -> Vec<u8> {
    format!("devagotchi-link/{LINK_PROTOCOL_VERSION}\n{hub}\n{device}\n{nonce}").into_bytes()
}

#[cfg(test)]
mod link_frame_tests {
    use super::*;

    #[test]
    fn hello_carries_its_build_and_an_old_hello_without_one_still_parses() {
        let hello = LinkFrame::Hello {
            version: LINK_PROTOCOL_VERSION,
            device: "dabo".into(),
            public_key: "ab".into(),
            signature: "cd".into(),
            enroll_code: None,
            build: Some("main-abc1234".into()),
        };
        let text = serde_json::to_string(&hello).unwrap();
        assert!(text.contains(r#""build":"main-abc1234""#), "{text}");
        let back: LinkFrame = serde_json::from_str(&text).unwrap();
        assert!(matches!(back, LinkFrame::Hello { build: Some(ref b), .. } if b == "main-abc1234"));
        // What a device from before builds had a name sends.
        let old =
            r#"{"type":"hello","version":1,"device":"dabo","public_key":"ab","signature":"cd"}"#;
        let parsed: LinkFrame = serde_json::from_str(old).unwrap();
        assert!(matches!(
            parsed,
            LinkFrame::Hello {
                build: None,
                enroll_code: None,
                ..
            }
        ));
    }

    #[test]
    fn update_and_update_status_round_trip() {
        let update = HubFrame::Update {
            version: "main-abc1234".into(),
            force: true,
        };
        let text = serde_json::to_string(&update).unwrap();
        assert_eq!(
            text,
            r#"{"type":"update","version":"main-abc1234","force":true}"#
        );
        let back: HubFrame = serde_json::from_str(&text).unwrap();
        assert!(
            matches!(back, HubFrame::Update { ref version, force: true } if version == "main-abc1234")
        );
        for state in UPDATE_STATES {
            let detail = (state == "failed").then(|| "busy".to_string());
            let status = LinkFrame::UpdateStatus {
                state: state.into(),
                detail: detail.clone(),
            };
            let text = serde_json::to_string(&status).unwrap();
            let back: LinkFrame = serde_json::from_str(&text).unwrap();
            assert!(
                matches!(back, LinkFrame::UpdateStatus { state: ref s, detail: ref d } if s == state && *d == detail),
                "{text}"
            );
        }
        let bare: LinkFrame =
            serde_json::from_str(r#"{"type":"update_status","state":"downloading"}"#).unwrap();
        assert!(matches!(bare, LinkFrame::UpdateStatus { detail: None, .. }));
    }
}
