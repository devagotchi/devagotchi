//! DG-CAPTURE-1: an AI session's events, as an adapter hands them to the machine's private
//! AI evidence (`devagotchi ai`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
