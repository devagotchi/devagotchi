//! The handoff told as stories: the merge, the authority, the bundle, and a whole handover
//! and restore. The merge's promises (symmetric, order-free, one live entry per phrase, nothing
//! dropped) are also checked over many generated pairs of sides.

use std::collections::BTreeSet;

use super::*;
use crate::lexicon::{LexiconError, Provenance, Scope, State, Wording};
use crate::words::normalize;

mod flow;

const OWNER: &str = "person-1";

fn entry(id: &str, phrase: &str, meaning: &str, state: State, revision: u64, at: f64) -> Entry {
    Entry {
        id: id.into(),
        owner_id: OWNER.into(),
        scope: Scope::Personal,
        wording: Wording {
            phrase: phrase.into(),
            meaning: meaning.into(),
            ..Wording::default()
        },
        state,
        revision,
        created_at: at,
        updated_at: at,
        provenance: Provenance::user(),
    }
}

fn ids(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.id.as_str()).collect()
}

fn conflict(why: Result<impl std::fmt::Debug>) -> String {
    match why {
        Err(LexiconError::Conflict(message)) => message,
        other => panic!("expected a conflict, got {other:?}"),
    }
}

// ---- The merge, case by case ----

#[test]
fn the_later_revision_of_an_entry_wins_on_either_side() {
    let old = entry("e1", "the panel", "the admin UI", State::Proposed, 1, 10.0);
    let mut new = entry(
        "e1",
        "the panel",
        "the admin console",
        State::Confirmed,
        2,
        20.0,
    );
    new.created_at = 10.0;
    for merged in [
        merge(&[old.clone()], &[new.clone()]),
        merge(&[new.clone()], &[old]),
    ] {
        assert_eq!(merged.entries, vec![new.clone()]);
        assert!(merged.superseded.is_empty() && merged.conflicts.is_empty());
    }
}

#[test]
fn one_entry_changed_two_ways_at_once_goes_to_the_person() {
    let ours = entry("e1", "the panel", "the admin UI", State::Confirmed, 3, 30.0);
    let theirs = entry(
        "e1",
        "the panel",
        "the billing page",
        State::Confirmed,
        3,
        30.0,
    );
    let merged = merge(&[ours], &[theirs]);
    assert!(merged.entries.is_empty());
    assert_eq!(merged.conflicts.len(), 1);
    assert_eq!(merged.conflicts[0].phrase, "the panel");
    assert_eq!(merged.conflicts[0].entries.len(), 2);
}

#[test]
fn a_confirmed_meaning_replaces_a_proposal_for_the_same_phrase() {
    let proposal = entry(
        "p1",
        "The Panel",
        "maybe the admin UI",
        State::Proposed,
        1,
        50.0,
    );
    let confirmed = entry(
        "c1",
        "the  panel",
        "the admin UI",
        State::Confirmed,
        1,
        10.0,
    );
    let merged = merge(&[proposal.clone()], &[confirmed.clone()]);
    assert_eq!(merged.entries, vec![confirmed]);
    assert_eq!(merged.superseded, vec![proposal]);
}

#[test]
fn two_confirmed_copies_of_one_meaning_keep_the_first_with_both_examples() {
    let mut first = entry("b", "the panel", "The admin UI", State::Confirmed, 1, 10.0);
    first.wording.examples = vec!["open the panel".into(), "Shared".into()];
    let mut second = entry("a", "the panel", "the  admin UI", State::Confirmed, 1, 20.0);
    second.wording.examples = vec!["shared".into(), "the panel is slow".into()];
    let merged = merge(&[first.clone()], &[second.clone()]);
    assert_eq!(ids(&merged.entries), ["b"], "the earlier one stays");
    assert_eq!(
        merged.entries[0].wording.examples,
        ["open the panel", "Shared", "the panel is slow"],
        "each example once, in one order"
    );
    assert_eq!(ids(&merged.superseded), ["a"]);
    assert_eq!(merge(&[second], &[first]), merged);
}

#[test]
fn two_confirmed_meanings_are_never_chosen_between() {
    let ours = entry("a", "the panel", "the admin UI", State::Confirmed, 1, 10.0);
    let theirs = entry(
        "b",
        "the panel",
        "the billing page",
        State::Confirmed,
        1,
        20.0,
    );
    let proposal = entry("c", "the panel", "a guess", State::Proposed, 1, 30.0);
    let merged = merge(&[ours, proposal], &[theirs]);
    assert!(
        merged.entries.is_empty(),
        "neither meaning stands until the person picks"
    );
    assert_eq!(ids(&merged.conflicts[0].entries), ["a", "b"]);
    assert_eq!(ids(&merged.superseded), ["c"]);
}

#[test]
fn the_same_meaning_under_other_concepts_is_another_meaning() {
    let mut ours = entry("a", "auth", "sign-in", State::Confirmed, 1, 10.0);
    ours.wording.concepts = vec!["boundary/iam".into()];
    let theirs = entry("b", "auth", "sign-in", State::Confirmed, 1, 20.0);
    assert_eq!(merge(&[ours], &[theirs]).conflicts.len(), 1);
}

#[test]
fn of_two_proposals_the_newer_stays() {
    let older = entry("a", "the panel", "the admin UI", State::Proposed, 1, 10.0);
    let newer = entry(
        "b",
        "the panel",
        "the settings",
        State::Inconclusive,
        1,
        20.0,
    );
    let merged = merge(&[older], &[newer]);
    assert_eq!(ids(&merged.entries), ["b"]);
    assert_eq!(ids(&merged.superseded), ["a"]);
}

#[test]
fn phrases_in_other_scopes_and_finished_entries_stand_apart() {
    let personal = entry("a", "the panel", "the admin UI", State::Confirmed, 1, 10.0);
    let mut project = entry(
        "b",
        "the panel",
        "the solar panel",
        State::Confirmed,
        1,
        20.0,
    );
    project.scope = Scope::Project {
        project_id: "roof".into(),
    };
    let retired = entry("c", "the panel", "the old admin UI", State::Retired, 2, 5.0);
    let deleted = entry("d", "", "", State::Deleted, 3, 6.0);
    let merged = merge(&[personal, retired], &[project, deleted]);
    assert_eq!(ids(&merged.entries), ["a", "b", "c", "d"]);
    assert!(merged.superseded.is_empty() && merged.conflicts.is_empty());
}

// ---- The merge's promises, over generated sides ----

/// xorshift64*: a few lines, the same numbers everywhere, no new dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

const STATES: [State; 6] = [
    State::Proposed,
    State::Confirmed,
    State::Inconclusive,
    State::Dismissed,
    State::Retired,
    State::Deleted,
];

/// The other side as it usually is: some of `ours` held again, each the same or changed in
/// one field, with `fresh` filling the ids left over.
fn echo(rng: &mut Rng, ours: &[Entry], fresh: Vec<Entry>) -> Vec<Entry> {
    let mut theirs: Vec<Entry> = Vec::new();
    for e in ours {
        if rng.below(2) == 0 {
            continue;
        }
        let mut copy = e.clone();
        match rng.below(6) {
            0 => copy.created_at += 1.0,
            1 => copy.provenance.source_kind = "clyffy".into(),
            2 => copy.wording.examples.push("w".into()),
            3 => copy.state = STATES[rng.below(STATES.len())],
            4 => copy.revision += 1,
            _ => {}
        }
        theirs.push(copy);
    }
    let held: BTreeSet<String> = theirs.iter().map(|e| e.id.clone()).collect();
    theirs.extend(fresh.into_iter().filter(|e| !held.contains(&e.id)));
    theirs
}

/// One side: a handful of entries drawn from small pools, so ids, phrases and meanings meet.
fn side(rng: &mut Rng) -> Vec<Entry> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for _ in 0..rng.below(7) {
        let id = rng.pick(&["a", "b", "c", "d", "e", "f"]);
        if !seen.insert(id) {
            continue;
        }
        let mut e = entry(
            id,
            rng.pick(&["the panel", "The Panel", "auth", "the board"]),
            rng.pick(&["the admin UI", "the admin  ui", "billing"]),
            STATES[rng.below(STATES.len())],
            1 + rng.below(3) as u64,
            (1 + rng.below(4)) as f64,
        );
        e.created_at = (1 + rng.below(3)) as f64;
        e.wording.examples = vec![rng.pick(&["x", "X", "y", "z"]).into()];
        if rng.below(4) == 0 {
            e.scope = Scope::Project {
                project_id: "p".into(),
            };
        }
        entries.push(e);
    }
    entries
}

#[test]
fn the_merge_is_symmetric_order_free_and_drops_nothing() {
    for seed in 1..=2_000u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let ours = side(&mut rng);
        let fresh = side(&mut rng);
        let theirs = echo(&mut rng, &ours, fresh);
        let merged = merge(&ours, &theirs);
        assert_eq!(merge(&theirs, &ours), merged, "seed {seed}: symmetric");
        let (mut a, mut b) = (ours.clone(), theirs.clone());
        rng.shuffle(&mut a);
        rng.shuffle(&mut b);
        assert_eq!(merge(&a, &b), merged, "seed {seed}: order-free");

        let mut live = BTreeSet::new();
        let mut kept = BTreeSet::new();
        for e in &merged.entries {
            assert!(kept.insert(e.id.clone()), "seed {seed}: one entry per id");
            if e.state.live() {
                let key = (format!("{:?}", e.scope), normalize(&e.wording.phrase));
                assert!(live.insert(key), "seed {seed}: one live entry per phrase");
            }
        }
        let accounted: BTreeSet<&str> = merged
            .entries
            .iter()
            .chain(&merged.superseded)
            .chain(merged.conflicts.iter().flat_map(|c| &c.entries))
            .map(|e| e.id.as_str())
            .collect();
        for e in ours.iter().chain(&theirs) {
            assert!(
                accounted.contains(e.id.as_str()),
                "seed {seed}: {} kept",
                e.id
            );
        }
        for gone in merged
            .superseded
            .iter()
            .filter(|e| e.state == State::Confirmed)
        {
            assert!(
                merged.entries.iter().any(|e| e.state == State::Confirmed
                    && e.scope == gone.scope
                    && normalize(&e.wording.phrase) == normalize(&gone.wording.phrase)
                    && normalize(&e.wording.meaning) == normalize(&gone.wording.meaning)),
                "seed {seed}: a confirmed meaning only gives way to the same meaning"
            );
        }
    }
}
