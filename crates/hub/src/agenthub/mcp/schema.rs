//! What the board tools take and give: each tool's input and output, as the MCP schema
//! clients see them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::agenthub::{Lease, approval::ApprovalStatus};

// ----- inputs ------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PrimeIn {
    pub project: String,
    /// How many ready tickets to list (1..=10, default 10).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadyIn {
    pub project: String,
    /// At most this many (default 20).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListIn {
    pub project: String,
    #[serde(default)]
    pub state: Option<String>,
    /// A holder's name, or `@me` for yourself as this board knows you.
    #[serde(default)]
    pub holder: Option<String>,
    /// An assignee's name, or `@me`.
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// Children of this ticket only.
    #[serde(default)]
    pub parent: Option<String>,
    /// task, epic or message.
    #[serde(default)]
    pub kind: Option<String>,
    /// Leave out done and cancelled tickets.
    #[serde(default)]
    pub open_only: bool,
    /// Substring of the title or description.
    #[serde(default)]
    pub text: Option<String>,
    /// Only tickets on this machine.
    #[serde(default)]
    pub machine: Option<String>,
    /// Only tickets for this repository URL.
    #[serde(default)]
    pub repo: Option<String>,
    /// Only tickets in this environment.
    #[serde(default)]
    pub environment: Option<String>,
    /// Only tickets of this agent.
    #[serde(default)]
    pub agent: Option<String>,
    /// Only tickets of this agent session.
    #[serde(default)]
    pub session: Option<String>,
    /// Only tickets with this fingerprint.
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// At most this many (default 50).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TicketIn {
    pub project: String,
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateIn {
    pub project: String,
    /// 1 to 300 characters.
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// task (default), epic or message.
    #[serde(default)]
    pub kind: Option<String>,
    /// 0 is most urgent; the board default is 2.
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Makes this a child (`parent.n` id) and records a `parent` dependency.
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    /// The ticket you were working when you found this one; recorded as `discovered-from`.
    #[serde(default)]
    pub discovered_from: Option<String>,
    /// The machine this ticket is about, as `machines` names it. Optional; the hub fills it
    /// when it knows it.
    #[serde(default)]
    pub machine: Option<String>,
    /// The repository URL. Optional; the hub fills it from the board's settings when set.
    #[serde(default)]
    pub repo: Option<String>,
    /// One of the board's environments (default dev, staging, prod). Optional.
    #[serde(default)]
    pub environment: Option<String>,
    /// The agent that made or works it. Optional; the hub fills it with your seat.
    #[serde(default)]
    pub agent: Option<String>,
    /// The agent's session id. Optional; the hub fills it when the connection has one.
    #[serde(default)]
    pub session: Option<String>,
    /// Folds repeats of one failure into one ticket. Optional.
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// The document item this ticket is imported from: `path` (repository-relative), `item`
    /// (its stable id there), optional `revision` and `content_sha256`. Once per board: the
    /// same create again returns the ticket as it is now; changed inputs are `source_conflict`.
    #[serde(default)]
    pub origin: Option<crate::agenthub::TicketSource>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClaimIn {
    pub project: String,
    /// A ticket to claim. Omit it to claim the best ready ticket for you.
    #[serde(default)]
    pub id: Option<String>,
    /// Lease length; the board clamps it to its min and max.
    #[serde(default)]
    pub ttl_ms: Option<i64>,
    /// Without an id: only tickets carrying this label.
    #[serde(default)]
    pub label: Option<String>,
    /// Without an id: only tickets in these queue states.
    #[serde(default)]
    pub states: Vec<String>,
    /// Without an id: only children of this ticket.
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ClaimOut {
    /// False when nothing was ready for you; `lease` is then absent.
    pub claimed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease: Option<Lease>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HeartbeatIn {
    pub project: String,
    pub id: String,
    /// The fence from your claim. Omit it to use the one this hub last gave you for this ticket.
    #[serde(default)]
    pub fence: Option<i64>,
    /// Anything watchers should see about where you are; replaces the previous progress.
    #[serde(default)]
    pub progress: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateIn {
    pub project: String,
    pub id: String,
    /// The ticket's current version (optimistic concurrency); from board_get.
    pub version: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    /// Replaces the label set.
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub clear_assignee: bool,
    /// A declared workflow transition for an unheld ticket (blocked, cancelled, back to
    /// ready). Active states and held tickets move only through claim / complete /
    /// handoff / release.
    #[serde(default)]
    pub state: Option<String>,
    /// The machine this ticket is about. Optional; an empty string clears it.
    #[serde(default)]
    pub machine: Option<String>,
    /// The repository URL. Optional; an empty string clears it.
    #[serde(default)]
    pub repo: Option<String>,
    /// One of the board's environments. Optional; an empty string clears it.
    #[serde(default)]
    pub environment: Option<String>,
    /// The agent. Optional; an empty string clears it.
    #[serde(default)]
    pub agent: Option<String>,
    /// The agent's session id. Optional; an empty string clears it.
    #[serde(default)]
    pub session: Option<String>,
    /// Optional; an empty string clears it.
    #[serde(default)]
    pub fingerprint: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteIn {
    pub project: String,
    pub id: String,
    /// 1 to 20000 characters; notes are append-only and travel with the ticket.
    pub body: String,
    /// note (default), handoff, review, decision, question …
    #[serde(default)]
    pub kind: Option<String>,
    /// Who it is for: a seat, or an AI by name (claude, chatgpt, codex, grok, hermes …). They
    /// get it, complete, on their next tool call, or at their webhook.
    #[serde(default)]
    pub to: Option<String>,
    /// Required while you hold the ticket; omit to use the one this hub last gave you.
    #[serde(default)]
    pub fence: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HandoffIn {
    pub project: String,
    pub id: String,
    #[serde(default)]
    pub fence: Option<i64>,
    /// What the next worker must know. Required.
    pub note: String,
    /// Who should pick it up; they become the assignee.
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReleaseIn {
    pub project: String,
    pub id: String,
    #[serde(default)]
    pub fence: Option<i64>,
    /// Why you are letting it go without finishing.
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompleteIn {
    pub project: String,
    pub id: String,
    #[serde(default)]
    pub fence: Option<i64>,
    /// What was done, for the reviewer. Required.
    pub summary: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepIn {
    pub project: String,
    pub id: String,
    pub depends_on: String,
    /// blocks (default; the only kind readiness uses), parent, related, discovered-from,
    /// duplicates or supersedes.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForceIn {
    pub project: String,
    pub id: String,
    /// At least three characters; recorded on the ticket.
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EventsIn {
    pub project: String,
    /// Events with a higher sequence number than this (0 for all).
    #[serde(default)]
    pub after_seq: i64,
    /// At most this many (default 100, max 1000).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WaitIn {
    pub project: String,
    /// Return as soon as an event newer than this exists.
    pub after_seq: i64,
    /// How long to wait before returning an empty list (default 20000, max 25000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InboxIn {
    pub project: String,
    /// Mark everything through this note's seq as read.
    #[serde(default)]
    pub ack_through: Option<i64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct InboxOut {
    pub notes: Vec<crate::agenthub::inbox::InboxNote>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WakeIn {
    pub project: String,
    /// The seat or AI name notes are addressed to.
    pub recipient: String,
    /// Where to POST its notes; omit to remove the webhook.
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WakeOut {
    pub recipient: String,
    /// Signs each POST (devagotchi-signature: sha256=HMAC of the body). Shown once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApprovalRequestIn {
    pub project: String,
    /// One line a person reads on their phone, e.g. "Restart the hub on Texas".
    pub title: String,
    /// Exactly what will run or change.
    pub what: String,
    /// Why it is needed now.
    #[serde(default)]
    pub why: Option<String>,
    /// How to undo it if it goes wrong.
    #[serde(default)]
    pub undo: Option<String>,
    /// The kind of step, in a word or two with dashes: `restart`, `deploy`, `edit-file`,
    /// `install-package`. Name it every time: a person can approve one exact step ahead of
    /// time, and then the same kind, target and `what` is approved at once.
    #[serde(default)]
    pub kind: Option<String>,
    /// What it is aimed at, when that matters: the machine or the service (`dabo`, `api`).
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SettingsIn {
    pub project: String,
    /// The project's repository, an https URL (https://github.com/you/project). New tickets
    /// carry it when they name none.
    #[serde(default)]
    pub repo: Option<String>,
    /// The environments a ticket may name: 1 to 20 lowercase names (dev, staging, prod).
    #[serde(default)]
    pub environments: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApprovalWaitIn {
    pub project: String,
    pub id: String,
    /// How long to wait for the decision (default 25000, max 50000). 0 reads the status now.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApprovalOut {
    #[serde(flatten)]
    pub approval: ApprovalStatus,
    /// Where the person approves or denies it.
    pub url: String,
    /// On a request: how many phones were sent a push. 0 means nobody who decides on this
    /// board has turned on notifications: give the user the url yourself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phones: Option<usize>,
    /// On a request a person approved ahead of time: what approved it. The status is then
    /// already `approved`; take the step without waiting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing: Option<crate::agenthub::approval::Standing>,
}
