//! Creation inputs shared by the MCP tool schema.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// 1 to 300 characters.
    pub title: String,
    /// What the work is, in as much detail as the next worker needs.
    #[serde(default)]
    pub description: String,
    /// task (default), epic or message.
    #[serde(default)]
    pub kind: Option<String>,
    /// 0 is most urgent; the board default is 2.
    #[serde(default)]
    pub priority: Option<i64>,
    /// Free-form labels; board_list and board_claim can filter by one.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Makes this a child (`parent.n` id) and records a `parent` dependency.
    #[serde(default)]
    pub parent: Option<String>,
    /// Existing tickets that must finish first; committed atomically with this ticket.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Who should take it; it is ready for them first.
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
