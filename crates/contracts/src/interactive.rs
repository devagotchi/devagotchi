//! Interactive operations: a program started with its input kept open, on a
//! pseudo-terminal or a pipe, typed into with `operation_input`, its terminal resized with
//! `operation_resize`; and the caller each transport names, which decides who may drive it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::OperationView;

/// The account of a caller on this machine itself (its adapter over local IPC): the machine's
/// own user, who owns everything its broker runs.
pub const LOCAL_ACCOUNT: &str = "local";

/// Who sent a request, as the transport that carried it knows it. A transport fills it in
/// from what it authenticated and replaces anything an AI put there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Caller {
    /// The account: `local` for this machine's own user over local IPC, `hub:<subject>`
    /// for an account a hub signed in.
    pub account: String,
    /// One AI conversation or client within that account: an adapter process, a hub MCP
    /// session. Opaque; compared, never shown.
    pub client: String,
}

/// How an interactive operation takes input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    /// A pseudo-terminal: the program sees a terminal of `cols` by `rows`, echoes what is
    /// typed, and Ctrl-C interrupts it. Output is one stream, `terminal`.
    #[default]
    Terminal,
    /// A plain pipe on stdin; stdout and stderr stay separate. Ctrl-C signals the program.
    Pipe,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InteractiveInput {
    #[serde(default)]
    pub mode: InputMode,
    /// Terminal width, 2..=500 columns (default 80).
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// Terminal height, 1..=200 rows (default 24).
    #[serde(default = "default_rows")]
    pub rows: u16,
}

impl Default for InteractiveInput {
    fn default() -> Self {
        Self {
            mode: InputMode::Terminal,
            cols: default_cols(),
            rows: default_rows(),
        }
    }
}

/// An interactive operation's input, as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InteractiveView {
    pub mode: InputMode,
    /// The terminal's current size (terminal mode).
    pub cols: u16,
    pub rows: u16,
    /// Whether `operation_input` can still reach the program.
    pub input_open: bool,
    /// Until when (milliseconds since the Unix epoch) one client holds the keyboard; others
    /// wait or take it over. Absent when nobody holds it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_until_ms: Option<u64>,
}

/// A named key for `operation_input`. On a terminal each is the bytes a terminal sends;
/// on a pipe `enter` is a newline, `ctrl_c` and `ctrl_backslash` signal the program (on
/// Linux; Windows refuses them on a pipe), and `ctrl_d` closes its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Space,
    CtrlC,
    CtrlD,
    CtrlZ,
    CtrlL,
    CtrlU,
    CtrlBackslash,
}

/// Most text one `operation_input` carries.
pub const MAX_INPUT_TEXT_BYTES: usize = 16 * 1024;
/// Most keys one `operation_input` carries.
pub const MAX_INPUT_KEYS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendInput {
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
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResizeInput {
    pub operation_id: String,
    /// New width, 2..=500 columns.
    pub cols: u16,
    /// New height, 1..=200 rows.
    pub rows: u16,
    /// As for `operation_input`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub take_over: bool,
    /// Set by the transport, never by an AI (see `StartInput::caller`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub caller: Option<Caller>,
}

/// Input delivered to the program.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputSent {
    pub operation: OperationView,
    /// The output sequence just before this input: what it caused is after it, so read or
    /// wait from here (`operation_read` cursor, `operation_wait` output_after).
    pub cursor: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListScope {
    /// What this conversation or client started.
    Mine,
    /// What any conversation of its account started, also before a restart; for this
    /// machine's own account, everything here.
    #[default]
    Account,
}

/// What a hub puts before the subject it signed in to make a caller's account.
pub const HUB_ACCOUNT_PREFIX: &str = "hub:";

impl Caller {
    /// An account a hub signed in, in one of its MCP sessions.
    pub fn hub(subject: &str, session: &str) -> Self {
        Self {
            account: format!("{HUB_ACCOUNT_PREFIX}{subject}"),
            client: session.into(),
        }
    }
}

impl crate::RequestBody {
    /// Name `caller` as who sent this request, replacing whatever a tool call put there:
    /// what a transport does with the caller it authenticated before passing a request on.
    pub fn set_caller(&mut self, caller: &Caller) {
        if let Some(slot) = self.caller_slot() {
            *slot = Some(caller.clone());
        }
    }

    /// The caller this request names, for a kind of request that carries one.
    /// What the request is about, for an audit line: the operation (its retry key when it
    /// starts one), or the path. Empty for a request about neither.
    pub fn target(&self) -> String {
        match self {
            Self::OperationStart(input) => input.idempotency_key.clone(),
            Self::OperationGet(input) | Self::OperationCancel(input) => input.operation_id.clone(),
            Self::OperationRead(input) => input.operation_id.clone(),
            Self::OperationWait(input) => input.operation_id.clone(),
            Self::OperationInput(input) => input.operation_id.clone(),
            Self::OperationResize(input) => input.operation_id.clone(),
            Self::FileRead(input) => input.path.clone(),
            Self::FileList(input) => input.path.clone(),
            Self::FileWrite(input) => input.path.clone(),
            Self::FileEdit(input) => input.path.clone(),
            Self::FileInfo(input) => input.path.clone(),
            Self::FileMkdir(input) => input.path.clone(),
            Self::FileMove(input) => format!("{} -> {}", input.source, input.destination),
            Self::FileSearch(input) => input.path.clone(),
            Self::ConfigSet(input) => input.key.clone(),
            Self::ProcessKill(input) => format!("pid {}", input.pid),
            Self::UrlRead(input) => input.url.clone(),
            Self::OperationList(_)
            | Self::OperationsChanged(_)
            | Self::ConfigGet(_)
            | Self::ProcessList(_)
            | Self::BrokerCapabilities(_) => String::new(),
        }
    }

    pub fn caller(&self) -> Option<&Caller> {
        match self {
            Self::OperationStart(input) => input.caller.as_ref(),
            Self::OperationList(input) => input.caller.as_ref(),
            Self::OperationInput(input) => input.caller.as_ref(),
            Self::OperationResize(input) => input.caller.as_ref(),
            Self::OperationGet(input) | Self::OperationCancel(input) => input.caller.as_ref(),
            Self::OperationRead(input) => input.caller.as_ref(),
            Self::OperationWait(input) => input.caller.as_ref(),
            _ => None,
        }
    }

    // Every kind is named, so a new one that carries a caller cannot slip past a transport.
    fn caller_slot(&mut self) -> Option<&mut Option<Caller>> {
        match self {
            Self::OperationStart(input) => Some(&mut input.caller),
            Self::OperationList(input) => Some(&mut input.caller),
            Self::OperationInput(input) => Some(&mut input.caller),
            Self::OperationResize(input) => Some(&mut input.caller),
            Self::OperationGet(input) | Self::OperationCancel(input) => Some(&mut input.caller),
            Self::OperationRead(input) => Some(&mut input.caller),
            Self::OperationWait(input) => Some(&mut input.caller),
            Self::OperationsChanged(_)
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
            | Self::BrokerCapabilities(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
