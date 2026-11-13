//! GitHub issues on the board: the records a hub keeps for an issue a ticket follows, and the
//! checks every delivery and context write passes. The signed webhook and its routes are the
//! hub's (`crates/hub/src/agenthub/issue_lifecycle.rs`).

use devagotchi_contracts::{BridgeError, BridgeResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Note, Ticket};

pub const MAX_BODY: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueWebhookEvent {
    pub action: String,
    pub repository: Repository,
    pub issue: Issue,
    pub sender: Sender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: u64,
    pub full_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub html_url: String,
    pub updated_at: String,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SourceIssue {
    pub repository_id: i64,
    pub issue_number: i64,
    pub ticket: String,
    pub repository: String,
    pub issue_title: String,
    pub issue_url: String,
    pub source_state: String,
    pub source_action: String,
    pub source_actor: String,
    pub source_updated_at: String,
    pub source_time: i64,
    pub source_hash: String,
    pub source_revision: i64,
    pub epoch: i64,
    pub needs_reconciliation: i64,
}

impl SourceIssue {
    pub fn finalization(&self) -> &'static str {
        if self.needs_reconciliation != 0 {
            "needs_reconciliation"
        } else if self.source_state == "closed" {
            "pending"
        } else {
            "not_requested"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueContext {
    pub project: String,
    pub issue: SourceIssue,
    pub ticket: Ticket,
    /// Append-only board records, including advice/evidence; subsequent pages use `next_after`.
    pub notes: Vec<Note>,
    pub next_after: Option<i64>,
    /// External closure is not evidence that the board's work or review completed.
    pub finalization: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryReceipt {
    pub status: String,
    pub duplicate: bool,
    pub issue: SourceIssue,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueLookup {
    pub repository: String,
    pub issue_number: u64,
    #[serde(default)]
    pub after: i64,
    #[serde(default = "page_size")]
    pub limit: usize,
}

fn page_size() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Advice {
    /// Stable within the request; identity is always the authenticated board actor.
    pub id: String,
    #[serde(default)]
    pub model_provider: String,
    #[serde(default)]
    pub model_name: String,
    pub recommendation: String,
    pub confidence: f64,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextWrite {
    pub repository: String,
    pub issue_number: u64,
    pub request_id: String,
    pub expected_version: i64,
    #[serde(default)]
    pub fence: Option<i64>,
    pub state_summary: String,
    #[serde(default)]
    pub handoff_note: String,
    #[serde(default)]
    pub decisions: Vec<Advice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ContextReceipt {
    pub request_id: String,
    pub ticket: String,
    pub note_seq: i64,
    pub revision: i64,
}


pub fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}
pub fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn request_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

impl IssueWebhookEvent {
    pub fn validate(&self) -> BridgeResult<()> {
        if self.repository.id == 0
            || self.repository.id > i64::MAX as u64
            || self.issue.number == 0
            || self.issue.number > i64::MAX as u64
        {
            return Err(invalid(
                "repository.id and issue.number must be positive signed 64-bit values",
            ));
        }
        let expected = match self.action.as_str() {
            "opened" | "reopened" => "open",
            "closed" => "closed",
            "edited" => self.issue.state.as_str(),
            _ => {
                return Err(BridgeError::new(
                    "unsupported_action",
                    "issue action is not handled",
                ));
            }
        };
        if !matches!(self.issue.state.as_str(), "open" | "closed") || self.issue.state != expected {
            return Err(invalid("issue state and action disagree"));
        }
        let at = self.issue.updated_at.as_bytes();
        if at.len() != 20
            || at.iter().enumerate().any(|(i, c)| match i {
                4 | 7 => *c != b'-',
                10 => *c != b'T',
                13 | 16 => *c != b':',
                19 => *c != b'Z',
                _ => !c.is_ascii_digit(),
            })
        {
            return Err(invalid("issue.updated_at must be a GitHub UTC timestamp"));
        }
        if self.issue.title.trim().is_empty()
            || self.issue.title.chars().count() > 300
            || self
                .issue
                .body
                .as_ref()
                .is_some_and(|body| body.len() > 128 * 1024)
            || self.sender.login.is_empty()
            || self.sender.login.len() > 100
        {
            return Err(invalid("invalid issue title, body or sender"));
        }
        let expected_url = format!(
            "https://github.com/{}/issues/{}",
            self.repository.full_name, self.issue.number
        );
        if self.issue.html_url != expected_url {
            return Err(invalid(
                "issue URL does not match the repository and number",
            ));
        }
        Ok(())
    }
}

impl ContextWrite {
    pub fn validate(&self) -> BridgeResult<()> {
        if !request_key(&self.request_id)
            || self.issue_number == 0
            || self.issue_number > i64::MAX as u64
            || self.expected_version < 1
            || self.state_summary.trim().is_empty()
            || self.state_summary.len() > 12000
            || self.handoff_note.len() > 4000
            || self.decisions.len() > 25
        {
            return Err(invalid(
                "context needs a request_id, positive expected_version and bounded summary/advice",
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for advice in &self.decisions {
            if !request_key(&advice.id)
                || !ids.insert(&advice.id)
                || !advice.confidence.is_finite()
                || !(0.0..=1.0).contains(&advice.confidence)
                || advice.recommendation.trim().is_empty()
                || advice.recommendation.len() > 4000
                || advice.model_provider.len() > 100
                || advice.model_name.len() > 100
                || advice.evidence_refs.len() > 20
                || advice
                    .evidence_refs
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 1000)
            {
                return Err(invalid(
                    "invalid advice identifier, confidence, recommendation or evidence",
                ));
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|error| invalid(error.to_string()))?;
        if bytes.len() > 20000 {
            return Err(invalid("context record exceeds 20000 bytes"));
        }
        Ok(())
    }
}

