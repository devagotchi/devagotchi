//! What the board tools take and give: each tool's input and output, as the MCP schema
//! clients see them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::agenthub::{Lease, approval::ApprovalStatus};

/// `board_wait`'s wait when `timeout_ms` is not given, and the longest it holds (ms).
pub const BOARD_WAIT_DEFAULT_MS: u64 = 20_000;
pub const BOARD_WAIT_MAX_MS: u64 = 25_000;
/// `approval_wait`'s wait when `timeout_ms` is not given, and the longest it holds (ms).
pub const APPROVAL_WAIT_DEFAULT_MS: u64 = 25_000;
pub const APPROVAL_WAIT_MAX_MS: u64 = 50_000;

// ----- inputs ------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrimeIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// How many ready tickets to list (1..=10, default 10).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadyIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// At most this many (default 20).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// Only tickets in this workflow state (board_prime names them).
    #[serde(default)]
    pub state: Option<String>,
    /// A holder's name, or `@me` for yourself as this board knows you.
    #[serde(default)]
    pub holder: Option<String>,
    /// An assignee's name, or `@me`.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Only tickets carrying this label.
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
#[serde(deny_unknown_fields)]
pub struct TicketIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
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

/// Tickets: as filtered (`board_list`), or ready for you, best first (`board_ready`). An
/// answer is an object, so the list is inside one (MCP `structuredContent`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct TicketsOut {
    pub tickets: Vec<crate::agenthub::Ticket>,
}

/// Events on the board's chain, oldest first (`board_events`, `board_wait`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct EventsOut {
    pub events: Vec<crate::agenthub::Event>,
}

/// A ticket's dependencies as they are now (`board_dep`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsOut {
    pub deps: Vec<crate::agenthub::Dep>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ClaimOut {
    /// False when nothing was ready for you; `lease` is then absent.
    pub claimed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease: Option<Lease>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
    /// The fence from your claim. Omit it to use the one this hub last gave you for this ticket.
    #[serde(default)]
    pub fence: Option<i64>,
    /// Anything watchers should see about where you are; replaces the previous progress.
    #[serde(default)]
    pub progress: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
    /// The ticket's current version (optimistic concurrency); from board_get.
    pub version: i64,
    /// A new title, 1 to 300 characters.
    #[serde(default)]
    pub title: Option<String>,
    /// A new description; replaces the old one.
    #[serde(default)]
    pub description: Option<String>,
    /// A new priority; 0 is most urgent.
    #[serde(default)]
    pub priority: Option<i64>,
    /// Replaces the label set.
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// A new assignee.
    #[serde(default)]
    pub assignee: Option<String>,
    /// True to leave the ticket unassigned.
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
#[serde(deny_unknown_fields)]
pub struct NoteIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
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
#[serde(deny_unknown_fields)]
pub struct HandoffIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
    /// The fence from your claim. Omit it to use the one this hub last gave you for this ticket.
    #[serde(default)]
    pub fence: Option<i64>,
    /// What the next worker must know. Required.
    pub note: String,
    /// Who should pick it up; they become the assignee.
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
    /// The fence from your claim. Omit it to use the one this hub last gave you for this ticket.
    #[serde(default)]
    pub fence: Option<i64>,
    /// Why you are letting it go without finishing.
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
    /// The fence from your claim. Omit it to use the one this hub last gave you for this ticket.
    #[serde(default)]
    pub fence: Option<i64>,
    /// What was done, for the reviewer. Required.
    pub summary: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DepIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket that depends.
    pub id: String,
    /// The ticket it depends on.
    pub depends_on: String,
    /// blocks (default; the only kind readiness uses), parent, related, discovered-from,
    /// duplicates or supersedes.
    #[serde(default)]
    pub kind: Option<String>,
    /// True to remove the dependency instead of recording it.
    #[serde(default)]
    pub remove: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForceIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket to take away from whoever holds it.
    pub id: String,
    /// At least three characters; recorded on the ticket.
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventsIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// Events with a higher sequence number than this (0 for all).
    #[serde(default)]
    pub after_seq: i64,
    /// At most this many (default 100, max 1000).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// Return as soon as an event newer than this exists.
    pub after_seq: i64,
    /// How long to wait before returning an empty list (default 20000, max 25000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InboxIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
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
#[serde(deny_unknown_fields)]
pub struct WakeIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
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
#[serde(deny_unknown_fields)]
pub struct ApprovalRequestIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// One line a person reads on their phone, e.g. "Restart the web server".
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
    /// What it is aimed at, when that matters: the machine or the service (`lab`, `api`).
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
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
#[serde(deny_unknown_fields)]
pub struct ApprovalWaitIn {
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The approval's id, from approval_request.
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
