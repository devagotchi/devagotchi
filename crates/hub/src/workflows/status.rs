//! What a workflow and each of its steps look like to the AI that follows them
//! (`workflow_status`, `workflow_wait`).

use schemars::JsonSchema;
use serde::Serialize;

/// One step as the hub sees it now.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct StepStatus {
    /// `secure_account`, `machine`, `phone` or `done`.
    pub kind: String,
    pub title: String,
    /// `done`, `current` (the one to do now), `pending` or `unavailable` (see `why`).
    pub state: String,
    /// What the hub sees, in words for the user: "nas is online".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact: Option<String>,
    /// A machine step: the machine's name, once its line was handed out (or the name it
    /// will get).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// A machine step: `linux`, `windows`, `docker` or `host`, fixed or as the user picked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Why an unavailable step cannot be done: tell the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The step is offered but never holds the workflow up (keeping the account).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// An `ai_client` step: which AI client (`claude`, `chatgpt`, `grok`, `cursor`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

/// Where a workflow stands.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct WorkflowStatus {
    pub id: String,
    /// The workflow's id from `workflow_list`.
    pub workflow: String,
    pub title: String,
    /// `open`, `completed`, `cancelled` or `expired` (the link stopped working: start again).
    pub state: String,
    /// The link to give the user, while it works.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// When the link stops working, in Unix seconds.
    pub expires_at: u64,
    pub steps: Vec<StepStatus>,
    /// What to do next.
    pub next: String,
}
