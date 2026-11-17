//! Where entries are kept. The kernel owns no database: a host implements [`Store`] on its
//! own (the hub on its Postgres, a standalone AgentHub on its own), and [`MemoryStore`] is the
//! reference every implementation is tested against.

use std::{
    collections::HashMap,
    future::{Future, ready},
    sync::Mutex,
};

use super::model::*;
use crate::words::normalize;

/// The first answer to a request id, kept so a retry gets it again.
#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    pub request_id: String,
    /// SHA-256 of the change as asked: the same id with another change is refused.
    pub payload_hash: String,
    pub entry: Entry,
}

/// One change to keep, whole or not at all.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    pub entry: Entry,
    /// The revision the entry must be at; none for a new entry, whose id must be unused.
    pub expected: Option<u64>,
    /// The event to append; its `seq` is the store's to set.
    pub event: Event,
    pub receipt: Receipt,
}

/// A lexicon's storage. Every method is scoped to one owner; an owner never reads another's.
///
/// [`Store::commit`] is the one write and it is atomic: it keeps the entry, appends the event
/// and keeps the receipt together, or does none of it. It refuses with
/// [`LexiconError::Conflict`] when the entry is not at `expected` (or exists, for a new one),
/// when another live entry holds the same normalized phrase in the same scope, or when the
/// request id already has a receipt. When the entry becomes `deleted`, the store also erases
/// the words of that entry's earlier events and receipts: a deleted entry leaves no text.
pub trait Store: Send + Sync {
    fn receipt(
        &self,
        owner: &str,
        request_id: &str,
    ) -> impl Future<Output = Result<Option<Receipt>>> + Send;

    fn get(&self, owner: &str, id: &str) -> impl Future<Output = Result<Option<Entry>>> + Send;

    /// Every entry of `owner` in `scope`, any state, ordered by normalized phrase then id, at
    /// most [`MAX_ENTRIES`].
    fn list(&self, owner: &str, scope: &Scope) -> impl Future<Output = Result<Vec<Entry>>> + Send;

    fn commit(&self, write: Write) -> impl Future<Output = Result<Event>> + Send;

    /// The entry's events, oldest first.
    fn history(&self, owner: &str, id: &str) -> impl Future<Output = Result<Vec<Event>>> + Send;
}

/// Entries in memory: the reference implementation, for tests and for a host with nothing
/// to keep.
#[derive(Debug, Default)]
pub struct MemoryStore {
    inner: Mutex<Memory>,
}

#[derive(Debug, Default)]
struct Memory {
    entries: HashMap<(String, String), Entry>,
    events: Vec<(String, Event)>,
    receipts: HashMap<(String, String), Receipt>,
    seq: u64,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut Memory) -> Result<T>) -> Result<T> {
        let mut memory = self
            .inner
            .lock()
            .map_err(|_| LexiconError::Store("the lexicon's memory lock is poisoned".into()))?;
        f(&mut memory)
    }
}

impl Store for MemoryStore {
    fn receipt(
        &self,
        owner: &str,
        request_id: &str,
    ) -> impl Future<Output = Result<Option<Receipt>>> + Send {
        ready(self.with(|m| {
            Ok(m.receipts
                .get(&(owner.to_string(), request_id.to_string()))
                .cloned())
        }))
    }

    fn get(&self, owner: &str, id: &str) -> impl Future<Output = Result<Option<Entry>>> + Send {
        ready(self.with(|m| Ok(m.entries.get(&(owner.to_string(), id.to_string())).cloned())))
    }

    fn list(&self, owner: &str, scope: &Scope) -> impl Future<Output = Result<Vec<Entry>>> + Send {
        ready(self.with(|m| {
            let mut found: Vec<Entry> = m
                .entries
                .values()
                .filter(|e| e.owner_id == owner && &e.scope == scope)
                .cloned()
                .collect();
            found.sort_by(|a, b| {
                (normalize(&a.wording.phrase), &a.id).cmp(&(normalize(&b.wording.phrase), &b.id))
            });
            found.truncate(MAX_ENTRIES);
            Ok(found)
        }))
    }

    fn commit(&self, write: Write) -> impl Future<Output = Result<Event>> + Send {
        ready(self.with(|m| {
            let owner = write.entry.owner_id.clone();
            let key = (owner.clone(), write.entry.id.clone());
            let receipt_key = (owner.clone(), write.receipt.request_id.clone());
            let conflict = |s: &str| Err(LexiconError::Conflict(s.to_string()));
            if m.receipts.contains_key(&receipt_key) {
                return conflict("the request id was used");
            }
            match (m.entries.get(&key), write.expected) {
                (None, None) => {}
                (Some(old), Some(expected)) if old.revision == expected => {}
                (Some(_), None) => return conflict("the entry id is taken"),
                (None, Some(_)) => return Err(LexiconError::NotFound),
                (Some(old), Some(expected)) => {
                    return conflict(&format!(
                        "the entry is at revision {}, not {expected}",
                        old.revision
                    ));
                }
            }
            if write.entry.state.live() {
                let phrase = normalize(&write.entry.wording.phrase);
                let taken = m.entries.values().any(|e| {
                    e.owner_id == owner
                        && e.id != write.entry.id
                        && e.scope == write.entry.scope
                        && e.state.live()
                        && normalize(&e.wording.phrase) == phrase
                });
                if taken {
                    return conflict("another live entry has this phrase in this scope");
                }
            }
            if write.entry.state == State::Deleted {
                for (_, event) in m
                    .events
                    .iter_mut()
                    .filter(|(o, e)| o == &owner && e.entry_id == write.entry.id)
                {
                    event.entry.wording = Wording::default();
                }
                for receipt in m
                    .receipts
                    .values_mut()
                    .filter(|r| r.entry.owner_id == owner && r.entry.id == write.entry.id)
                {
                    receipt.entry.wording = Wording::default();
                }
            }
            m.seq += 1;
            let event = Event {
                seq: m.seq,
                ..write.event
            };
            m.entries.insert(key, write.entry);
            m.events.push((owner, event.clone()));
            m.receipts.insert(receipt_key, write.receipt);
            Ok(event)
        }))
    }

    fn history(&self, owner: &str, id: &str) -> impl Future<Output = Result<Vec<Event>>> + Send {
        ready(self.with(|m| {
            Ok(m.events
                .iter()
                .filter(|(o, e)| o == owner && e.entry_id == id)
                .map(|(_, e)| e.clone())
                .collect())
        }))
    }
}
