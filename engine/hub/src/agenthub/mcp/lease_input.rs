//! Typed lease mutation inputs, including explicit retry identity.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
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
    /// Without an id: require every label, combined with the singular label if supplied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Without an id: only tickets in these queue states.
    #[serde(default)]
    pub states: Vec<String>,
    /// Without an id: only children of this ticket.
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket's id (from board_list, board_ready or board_get).
    pub id: String,
    /// The fence from your claim. Omit it to use the one this hub last gave you for this ticket.
    #[serde(default)]
    pub fence: Option<i64>,
    /// Anything watchers should see about where you are; replaces the previous progress.
    #[serde(
        default,
        deserialize_with = "super::schema::present_progress",
        skip_serializing_if = "Option::is_none"
    )]
    pub progress: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForceIn {
    /// Reuse this key and identical input after an uncertain result (1..=200 bytes).
    #[serde(default)]
    pub request_id: Option<String>,
    /// The project whose board to use, as the hub names it (whoami lists yours).
    pub project: String,
    /// The ticket to take away from whoever holds it.
    pub id: String,
    /// At least three characters; recorded on the ticket.
    pub reason: String,
}
