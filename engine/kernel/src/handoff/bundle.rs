//! `LexiconBundleV1`: one person's lexicon as it crosses between Devagotchi and Clyffy.
//!
//! An entry in a bundle is Clyffy's lexicon entry, field for field (clyffy_alpha `4e578c9`,
//! `interfaces/clyffyd/src/app/store/lexicon.rs`: phrase, meaning and examples at the top
//! level, `indexed_revision`), with the two things Devagotchi adds, both optional so an entry
//! Clyffy wrote reads as it is: `concepts`, and the states `inconclusive` and `retired`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::lexicon::{Entry, LexiconError, Provenance, Result, Scope, State, Wording};

/// The format name every bundle carries.
pub const FORMAT: &str = "devagotchi.lexicon.bundle/1";
/// Most entries one bundle holds.
pub const MAX_BUNDLE_ENTRIES: usize = 20_000;

/// Who wrote a bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Devagotchi,
    Clyffy,
}

/// One person's entries, from one side, at one epoch of the authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LexiconBundleV1 {
    /// Always [`FORMAT`].
    pub format: String,
    pub owner_id: String,
    pub from: Side,
    /// The authority's epoch the bundle was written under: a bundle from an older epoch is
    /// refused (`authority.rs`).
    pub epoch: u64,
    /// Seconds since the epoch.
    pub exported_at: f64,
    pub entries: Vec<BundleEntry>,
}

/// An entry as a bundle carries it: Clyffy's shape, and Devagotchi's concepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundleEntry {
    pub id: String,
    /// Clyffy writes it; a bundle's own `owner_id` is what counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub scope: Scope,
    pub phrase: String,
    pub meaning: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub counterexamples: Vec<String>,
    pub state: State,
    pub revision: u64,
    pub created_at: f64,
    pub updated_at: f64,
    pub provenance: Provenance,
    /// Dictionary concepts (Devagotchi's); Clyffy has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concepts: Vec<String>,
    /// The revision Clyffy last indexed into its memory; carried, never used here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_revision: Option<u64>,
}

impl BundleEntry {
    pub fn from_entry(entry: &Entry) -> Self {
        Self {
            id: entry.id.clone(),
            owner_id: Some(entry.owner_id.clone()),
            scope: entry.scope.clone(),
            phrase: entry.wording.phrase.clone(),
            meaning: entry.wording.meaning.clone(),
            examples: entry.wording.examples.clone(),
            counterexamples: entry.wording.counterexamples.clone(),
            state: entry.state,
            revision: entry.revision,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            provenance: entry.provenance.clone(),
            concepts: entry.wording.concepts.clone(),
            indexed_revision: None,
        }
    }

    /// The entry, owned by `owner`.
    pub fn into_entry(self, owner: &str) -> Entry {
        Entry {
            id: self.id,
            owner_id: owner.to_string(),
            scope: self.scope,
            wording: Wording {
                phrase: self.phrase,
                meaning: self.meaning,
                concepts: self.concepts,
                examples: self.examples,
                counterexamples: self.counterexamples,
            },
            state: self.state,
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
            provenance: self.provenance,
        }
    }
}

impl LexiconBundleV1 {
    /// `entries` of `owner`, from `from`, under `epoch`.
    pub fn new(owner: &str, from: Side, epoch: u64, exported_at: f64, entries: &[Entry]) -> Self {
        Self {
            format: FORMAT.to_string(),
            owner_id: owner.to_string(),
            from,
            epoch,
            exported_at,
            entries: entries.iter().map(BundleEntry::from_entry).collect(),
        }
    }

    /// The bundle read from JSON and checked: its format, its size, one owner, ids that are
    /// unique and well formed, revisions from 1, one live entry per phrase in a scope.
    pub fn read(json: &str) -> Result<Self> {
        let bundle: Self = serde_json::from_str(json)
            .map_err(|e| LexiconError::Invalid(format!("the bundle does not read: {e}")))?;
        bundle.check()?;
        Ok(bundle)
    }

    pub fn check(&self) -> Result<()> {
        let bad = |m: String| Err(LexiconError::Invalid(m));
        if self.format != FORMAT {
            return bad(format!("format: {FORMAT}, not {}", self.format));
        }
        crate::lexicon::check_id(&self.owner_id, "owner_id")?;
        if self.entries.len() > MAX_BUNDLE_ENTRIES {
            return bad(format!("at most {MAX_BUNDLE_ENTRIES} entries"));
        }
        let mut ids = std::collections::HashSet::new();
        let mut live = std::collections::HashSet::new();
        for entry in &self.entries {
            crate::lexicon::check_id(&entry.id, "entries.id")?;
            if !ids.insert(entry.id.as_str()) {
                return bad(format!("entries: {} appears twice", entry.id));
            }
            if entry.owner_id.as_ref().is_some_and(|o| o != &self.owner_id) {
                return bad(format!("entries: {} belongs to another owner", entry.id));
            }
            if entry.revision == 0 {
                return bad(format!("entries: {} has revision 0", entry.id));
            }
            let phrase = (entry.scope.clone(), crate::words::normalize(&entry.phrase));
            if entry.state.live() && !live.insert(phrase) {
                return bad(format!(
                    "entries: {} is a second live entry for its phrase",
                    entry.id
                ));
            }
        }
        Ok(())
    }

    /// Its entries, each owned by the bundle's owner.
    pub fn entries(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .cloned()
            .map(|e| e.into_entry(&self.owner_id))
            .collect()
    }
}
