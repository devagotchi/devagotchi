//! The lexicon: the phrases a person and their projects use, what each means, and where it
//! stands (proposed, confirmed, inconclusive, dismissed, retired, deleted).
//!
//! [`Lexicon`] is the front door. It checks a change ([`apply`]), keeps it in a host's
//! [`Store`] with its event and receipt, and resolves phrases ([`resolve`]) against the
//! person's entries and the [`Dictionary`]. A project's confirmed meaning wins over the
//! person's for the same phrase, and the person's wins over the dictionary.

mod apply;
pub mod model;
pub mod resolve;
pub mod store;
#[cfg(test)]
mod tests;

pub use model::*;
pub use resolve::{Found, Origin, Outcome, Resolution};
pub use store::{MemoryStore, Receipt, Store, Write};

use sha2::{Digest, Sha256};

use crate::{dictionary::Dictionary, words::words};

/// Which entries a list returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    /// `confirmed` only: what is in use.
    #[default]
    Confirmed,
    /// Every state but `deleted`: what the person reviews.
    Reviewable,
    /// Everything, tombstones included.
    All,
}

impl Filter {
    fn keeps(self, state: State) -> bool {
        match self {
            Self::Confirmed => state == State::Confirmed,
            Self::Reviewable => state != State::Deleted,
            Self::All => true,
        }
    }
}

/// A lexicon over a host's store and a dictionary.
pub struct Lexicon<'d, S> {
    store: S,
    dictionary: &'d Dictionary,
    clock: fn() -> f64,
}

impl<'d, S: Store> Lexicon<'d, S> {
    pub fn new(store: S, dictionary: &'d Dictionary) -> Self {
        Self {
            store,
            dictionary,
            clock: now,
        }
    }

    /// The same, with a clock (seconds since the epoch) for tests.
    pub fn with_clock(mut self, clock: fn() -> f64) -> Self {
        self.clock = clock;
        self
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn dictionary(&self) -> &'d Dictionary {
        self.dictionary
    }

    /// Make or move an entry. A request id already answered returns that answer again when
    /// the change is the same, and is refused when it is not.
    pub async fn apply(&self, change: &Change) -> Result<Entry> {
        apply::check_change(change)?;
        let payload_hash = hash(change)?;
        if let Some(answered) = self.answered(change, &payload_hash).await? {
            return Ok(answered);
        }
        let old = match &change.record_id {
            Some(id) => self.store.get(&change.owner_id, id).await?,
            None => None,
        };
        let entry = apply::next(
            old.as_ref(),
            change,
            self.dictionary,
            (self.clock)(),
            new_id,
        )?;
        let write = Write {
            expected: change.expected_revision,
            event: Event {
                seq: 0,
                entry_id: entry.id.clone(),
                revision: entry.revision,
                request_id: change.request_id.clone(),
                operation: change.operation.name().to_string(),
                at: entry.updated_at,
                entry: entry.clone(),
            },
            receipt: Receipt {
                request_id: change.request_id.clone(),
                payload_hash: payload_hash.clone(),
                entry: entry.clone(),
            },
            entry,
        };
        match self.store.commit(write).await {
            Ok(event) => Ok(event.entry),
            // The same request, committed by a racing call meanwhile: its answer is ours.
            Err(LexiconError::Conflict(why)) => match self.answered(change, &payload_hash).await? {
                Some(answered) => Ok(answered),
                None => Err(LexiconError::Conflict(why)),
            },
            Err(other) => Err(other),
        }
    }

    async fn answered(&self, change: &Change, payload_hash: &str) -> Result<Option<Entry>> {
        match self
            .store
            .receipt(&change.owner_id, &change.request_id)
            .await?
        {
            Some(r) if r.payload_hash == payload_hash => Ok(Some(r.entry)),
            Some(_) => Err(LexiconError::Conflict(
                "the request id was used for another change".into(),
            )),
            None => Ok(None),
        }
    }

    pub async fn get(&self, owner: &str, id: &str) -> Result<Entry> {
        apply::check_id(owner, "owner_id")?;
        apply::check_id(id, "id")?;
        self.store
            .get(owner, id)
            .await?
            .ok_or(LexiconError::NotFound)
    }

    pub async fn list(&self, owner: &str, scope: &Scope, filter: Filter) -> Result<Vec<Entry>> {
        apply::check_id(owner, "owner_id")?;
        apply::check_scope(scope)?;
        let mut entries = self.store.list(owner, scope).await?;
        entries.retain(|e| filter.keeps(e.state));
        Ok(entries)
    }

    /// The entries in reach for resolving: the project's, then the person's, any state but
    /// `deleted`.
    pub async fn in_reach(&self, owner: &str, project: Option<&str>) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        if let Some(project_id) = project {
            let scope = Scope::Project {
                project_id: project_id.to_string(),
            };
            entries.extend(self.list(owner, &scope, Filter::Reviewable).await?);
        }
        entries.extend(
            self.list(owner, &Scope::Personal, Filter::Reviewable)
                .await?,
        );
        Ok(entries)
    }

    /// What is in use: the project's confirmed meanings, and the person's for every phrase
    /// the project does not define.
    pub async fn effective(&self, owner: &str, project: &str) -> Result<Vec<Entry>> {
        let scope = Scope::Project {
            project_id: project.to_string(),
        };
        let project_entries = self.list(owner, &scope, Filter::Confirmed).await?;
        let defined: std::collections::HashSet<String> = project_entries
            .iter()
            .map(|e| crate::words::normalize(&e.wording.phrase))
            .collect();
        let mut entries = project_entries;
        entries.extend(
            self.list(owner, &Scope::Personal, Filter::Confirmed)
                .await?
                .into_iter()
                .filter(|e| !defined.contains(&crate::words::normalize(&e.wording.phrase))),
        );
        entries.sort_by_key(|e| crate::words::normalize(&e.wording.phrase));
        Ok(entries)
    }

    pub async fn history(&self, owner: &str, id: &str) -> Result<Vec<Event>> {
        apply::check_id(owner, "owner_id")?;
        apply::check_id(id, "id")?;
        self.store.history(owner, id).await
    }

    /// `phrase`, said in `context` (the sentence around it; empty for the phrase alone).
    pub async fn resolve(
        &self,
        owner: &str,
        project: Option<&str>,
        phrase: &str,
        context: &str,
    ) -> Result<Resolution> {
        let entries = self.in_reach(owner, project).await?;
        let context = if context.trim().is_empty() {
            words(phrase)
        } else {
            words(context)
        };
        Ok(resolve::resolve(
            self.dictionary,
            &entries,
            phrase,
            &context,
        ))
    }

    /// Every known phrase in `text`, resolved.
    pub async fn scan(
        &self,
        owner: &str,
        project: Option<&str>,
        text: &str,
        weak: bool,
    ) -> Result<Vec<Found>> {
        let entries = self.in_reach(owner, project).await?;
        Ok(resolve::scan(self.dictionary, &entries, text, weak))
    }
}

fn hash(change: &Change) -> Result<String> {
    let encoded = serde_json::to_vec(change)
        .map_err(|e| LexiconError::Invalid(format!("the change does not encode: {e}")))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `lx_` and 32 hex characters, as Clyffy names entries.
fn new_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // No randomness from the OS: the time and a counter still make it unique here.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        bytes[..8].copy_from_slice(&now().to_bits().to_le_bytes());
        bytes[8..].copy_from_slice(&n.to_le_bytes());
    }
    format!("lx_{}", hex::encode(bytes))
}
