//! Two sides' entries made one: what both hold, with each phrase meaning one thing.
//!
//! The merge is pure and symmetric: `merge(a, b)` and `merge(b, a)` give the same result,
//! whatever order either side lists its entries in. It never picks between two meanings the
//! person confirmed: those come back as a [`Conflict`] for the person, and neither is in the
//! merged entries until they choose.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::Serialize;

use crate::{
    lexicon::{Entry, MAX_ITEMS, Scope, State},
    words::normalize,
};

/// What two sides' entries came to.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Merged {
    /// One entry per id, at most one live entry per phrase in a scope; ordered by id.
    pub entries: Vec<Entry>,
    /// Live entries another entry for the same phrase replaced (a confirmed meaning over a
    /// proposal, a newer proposal over an older one, one of two equal confirmed meanings).
    pub superseded: Vec<Entry>,
    /// Phrases with two confirmed meanings, or one entry the two sides changed differently at
    /// the same revision: only the person can say which stands.
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Conflict {
    pub scope: Scope,
    /// The phrase, normalized.
    pub phrase: String,
    /// The entries in conflict, ordered by id.
    pub entries: Vec<Entry>,
}

/// An entry's whole content, for telling two copies apart and ordering them the same way
/// whichever side listed them first.
fn content(entry: &Entry) -> String {
    serde_json::to_string(entry).unwrap_or_default()
}

/// What a confirmed entry means, for telling two meanings apart: the meaning as normalized
/// and its concepts as a set.
fn meaning(entry: &Entry) -> (String, BTreeSet<String>) {
    (
        normalize(&entry.wording.meaning),
        entry.wording.concepts.iter().cloned().collect(),
    )
}

fn scope_key(scope: &Scope) -> String {
    match scope {
        Scope::Personal => String::new(),
        Scope::Project { project_id } => format!("project:{project_id}"),
    }
}

/// `ours` and `theirs` as one.
pub fn merge(ours: &[Entry], theirs: &[Entry]) -> Merged {
    let mut conflicts = Vec::new();
    // 1. One copy per id: the later revision, then the later change; two copies at the same
    //    revision and time that differ are a conflict.
    let mut by_id: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for entry in ours.iter().chain(theirs) {
        by_id
            .entry(entry.id.clone())
            .or_default()
            .push(entry.clone());
    }
    let mut chosen = Vec::new();
    for (_, mut copies) in by_id {
        copies.sort_by(|a, b| {
            (b.revision, b.updated_at.to_bits(), content(b)).cmp(&(
                a.revision,
                a.updated_at.to_bits(),
                content(a),
            ))
        });
        copies.dedup_by(|a, b| content(a) == content(b));
        let top = &copies[0];
        let rivals: Vec<Entry> = copies
            .iter()
            .filter(|c| {
                c.revision == top.revision && c.updated_at.to_bits() == top.updated_at.to_bits()
            })
            .cloned()
            .collect();
        if rivals.len() > 1 {
            conflicts.push(Conflict {
                scope: top.scope.clone(),
                phrase: normalize(&top.wording.phrase),
                entries: rivals,
            });
        } else {
            chosen.push(copies.swap_remove(0));
        }
    }
    // 2. One live entry per phrase in a scope.
    let mut entries = Vec::new();
    let mut superseded = Vec::new();
    let mut live: BTreeMap<(String, String), Vec<Entry>> = BTreeMap::new();
    for entry in chosen {
        if entry.state.live() {
            live.entry((scope_key(&entry.scope), normalize(&entry.wording.phrase)))
                .or_default()
                .push(entry);
        } else {
            entries.push(entry);
        }
    }
    for ((_, phrase), mut group) in live {
        group.sort_by(|a, b| a.id.cmp(&b.id));
        if group.len() == 1 {
            entries.extend(group);
            continue;
        }
        let confirmed: Vec<&Entry> = group
            .iter()
            .filter(|e| e.state == State::Confirmed)
            .collect();
        let meanings: BTreeSet<_> = confirmed.iter().map(|e| meaning(e)).collect();
        if meanings.len() > 1 {
            let (held, rest): (Vec<Entry>, Vec<Entry>) =
                group.into_iter().partition(|e| e.state == State::Confirmed);
            conflicts.push(Conflict {
                scope: held[0].scope.clone(),
                phrase,
                entries: held,
            });
            superseded.extend(rest);
            continue;
        }
        let keep = if confirmed.is_empty() {
            // The newest proposal; the lowest id among equals.
            group
                .iter()
                .max_by(|a, b| {
                    a.updated_at
                        .total_cmp(&b.updated_at)
                        .then_with(|| b.id.cmp(&a.id))
                })
                .map(|e| e.id.clone())
        } else {
            // The first to be confirmed; the lowest id among equals.
            confirmed
                .iter()
                .min_by(|a, b| {
                    a.created_at
                        .total_cmp(&b.created_at)
                        .then_with(|| a.id.cmp(&b.id))
                })
                .map(|e| e.id.clone())
        };
        let keep = keep.unwrap_or_default();
        let (mut kept, rest): (Vec<Entry>, Vec<Entry>) =
            group.into_iter().partition(|e| e.id == keep);
        let mut kept = kept.remove(0);
        if kept.state == State::Confirmed {
            // Equal meanings: the examples of every confirmed copy, once each.
            for other in rest.iter().filter(|e| e.state == State::Confirmed) {
                kept.wording.examples = union(&kept.wording.examples, &other.wording.examples);
                kept.wording.counterexamples = union(
                    &kept.wording.counterexamples,
                    &other.wording.counterexamples,
                );
            }
        }
        entries.push(kept);
        superseded.extend(rest);
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    superseded.sort_by(|a, b| a.id.cmp(&b.id));
    conflicts
        .sort_by(|a, b| (scope_key(&a.scope), &a.phrase).cmp(&(scope_key(&b.scope), &b.phrase)));
    Merged {
        entries,
        superseded,
        conflicts,
    }
}

/// What `a` and `b` hold, each once (compared trimmed and without case, the first spelling in
/// sort order kept), sorted, at most [`MAX_ITEMS`]: the same whichever side is `a`.
fn union(a: &[String], b: &[String]) -> Vec<String> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for item in a.iter().chain(b) {
        let item = item.trim().to_string();
        let held = seen.entry(normalize(&item)).or_insert_with(|| item.clone());
        if item < *held {
            *held = item;
        }
    }
    seen.into_values().take(MAX_ITEMS).collect()
}
