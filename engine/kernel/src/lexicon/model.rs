//! The lexicon's records: an entry, the change that makes or moves one, and the event each
//! change leaves.
//!
//! Field for field this is Clyffy's lexicon entry (clyffy_alpha `4e578c9`,
//! `interfaces/clyffyd/src/app/store/lexicon.rs`) so a handoff round-trips, plus `concepts`
//! (OntoLex-Lemon: a sense references one concept, and a phrase with two is ambiguous until
//! context picks) and two states Clyffy lacks: `inconclusive` (seen, not yet understood) and
//! `retired` (no longer used; old text still resolves).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most entries one list returns.
pub const MAX_ENTRIES: usize = 2000;
/// Most examples or counterexamples on an entry, and the longest one.
pub const MAX_ITEMS: usize = 12;
pub const MAX_ITEM_CHARS: usize = 500;
/// Most concepts one entry points at.
pub const MAX_CONCEPTS: usize = 8;
pub const MAX_PHRASE_CHARS: usize = 120;
pub const MAX_MEANING_CHARS: usize = 1000;

/// Whose words these are: the person's own, or one project's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Personal,
    Project { project_id: String },
}

/// Where an entry stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Suggested (by the AI, from a conversation); not the person's word yet.
    Proposed,
    /// The person's own meaning. Only this resolves ahead of the dictionary.
    Confirmed,
    /// Seen, and not understood yet: the pile new aliases come from.
    Inconclusive,
    /// Turned down.
    Dismissed,
    /// No longer used; it still resolves, marked retired, so older text keeps its meaning.
    Retired,
    /// Gone: its words are erased and only the id and revision are kept.
    Deleted,
}

impl State {
    /// Whether it holds its phrase in its scope: one live entry per phrase.
    pub fn live(self) -> bool {
        matches!(self, Self::Proposed | Self::Confirmed | Self::Inconclusive)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Confirmed => "confirmed",
            Self::Inconclusive => "inconclusive",
            Self::Dismissed => "dismissed",
            Self::Retired => "retired",
            Self::Deleted => "deleted",
        }
    }
}

/// Where a change came from. Never the conversation's text, only ids pointing at it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// `user` (the person), `conversation` (proposed from one), `seed`, or `clyffy` (a
    /// handoff).
    pub source_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

impl Provenance {
    /// The person, directly.
    pub fn user() -> Self {
        Self {
            source_kind: "user".into(),
            project_id: None,
            workflow_id: None,
            turn_id: None,
        }
    }
}

/// What an entry says.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wording {
    pub phrase: String,
    /// What the phrase means, in the person's words.
    pub meaning: String,
    /// The dictionary concepts it means (`boundary/iam`); none when it is the person's own.
    #[serde(default)]
    pub concepts: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub counterexamples: Vec<String>,
}

/// One phrase in one scope, versioned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub owner_id: String,
    pub scope: Scope,
    pub wording: Wording,
    pub state: State,
    /// 1 when made; every change adds one. A change names the revision it read.
    pub revision: u64,
    /// Seconds since the epoch, as Clyffy keeps them.
    pub created_at: f64,
    pub updated_at: f64,
    pub provenance: Provenance,
}

/// What a change does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// A new entry, suggested: `proposed`.
    Propose { wording: Wording },
    /// A new entry in the person's own words: `confirmed`.
    Define { wording: Wording },
    /// A phrase seen and not understood: `inconclusive`.
    Park { wording: Wording },
    /// New words for a live entry; its state stays.
    Revise { wording: Wording },
    /// `proposed` or `inconclusive` becomes `confirmed`.
    Confirm,
    /// A live entry is turned down.
    Dismiss,
    /// A `confirmed` entry is no longer used.
    Retire,
    /// Erase it: any state but `deleted`.
    Delete,
}

impl Operation {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Propose { .. } => "propose",
            Self::Define { .. } => "define",
            Self::Park { .. } => "park",
            Self::Revise { .. } => "revise",
            Self::Confirm => "confirm",
            Self::Dismiss => "dismiss",
            Self::Retire => "retire",
            Self::Delete => "delete",
        }
    }

    /// Whether it makes a new entry.
    pub fn creates(&self) -> bool {
        matches!(
            self,
            Self::Propose { .. } | Self::Define { .. } | Self::Park { .. }
        )
    }
}

/// One change, as a caller asks for it. `owner_id` is set by the host from who is signed
/// in, never from what the caller sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// The caller's id for this change: the same id with the same change returns the first
    /// answer again; with a different change it is refused.
    pub request_id: String,
    pub owner_id: String,
    pub scope: Scope,
    /// The entry it changes; none when it makes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
    /// The revision the caller read; none when it makes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    pub provenance: Provenance,
    pub operation: Operation,
}

/// What one change left: the entry as it became.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// Order among the owner's events, from the store.
    pub seq: u64,
    pub entry_id: String,
    pub revision: u64,
    pub request_id: String,
    /// The [`Operation::name`].
    pub operation: String,
    pub at: f64,
    pub entry: Entry,
}

/// Why a change or read was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexiconError {
    /// The request is malformed: the field and what is wrong.
    Invalid(String),
    /// The entry moved since it was read, the phrase is taken, the move is not allowed from
    /// its state, or the request id was used for another change.
    Conflict(String),
    NotFound,
    /// The host's storage failed.
    Store(String),
}

impl LexiconError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid_request",
            Self::Conflict(_) => "conflict",
            Self::NotFound => "not_found",
            Self::Store(_) => "store_failed",
        }
    }
}

impl std::fmt::Display for LexiconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) | Self::Conflict(m) | Self::Store(m) => {
                write!(f, "{}: {m}", self.code())
            }
            Self::NotFound => f.write_str("not_found: no such entry"),
        }
    }
}

impl std::error::Error for LexiconError {}

pub type Result<T> = std::result::Result<T, LexiconError>;
