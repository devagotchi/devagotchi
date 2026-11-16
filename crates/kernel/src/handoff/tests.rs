//! The handoff told as stories: the merge, the authority, the bundle, and a whole handover
//! and restore. The merge's promises (symmetric, order-free, one live entry per phrase, nothing
//! dropped) are also checked over many generated pairs of sides.

use std::collections::BTreeSet;

use super::*;
use crate::lexicon::{LexiconError, Provenance, Scope, State, Wording};
use crate::words::normalize;

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

// ---- The authority ----

#[test]
fn a_handover_and_a_restore_each_move_the_epoch_on() {
    let start = Authority::default();
    assert_eq!((start.holder, start.epoch), (Holder::Devagotchi, 1));
    assert!(start.fence(1).is_ok());

    let handing = start.begin_handover().unwrap();
    assert_eq!((handing.holder, handing.epoch), (Holder::HandingOver, 2));
    assert!(conflict(handing.fence(2)).contains("handing_over"));
    assert!(conflict(handing.complete_handover(1)).contains("epoch 1"));

    let clyffy = handing.complete_handover(2).unwrap();
    assert_eq!((clyffy.holder, clyffy.epoch), (Holder::Clyffy, 2));
    assert!(!clyffy.devagotchi_writes());
    conflict(clyffy.begin_handover());

    let restoring = clyffy.begin_restore().unwrap();
    assert_eq!((restoring.holder, restoring.epoch), (Holder::Restoring, 3));
    let back = restoring.complete_restore(3).unwrap();
    assert_eq!((back.holder, back.epoch), (Holder::Devagotchi, 3));
    assert!(
        conflict(back.fence(1)).contains("epoch 3"),
        "a write from before is fenced"
    );
    assert!(back.fence(3).is_ok());
}

#[test]
fn an_aborted_handover_fences_writes_from_before_it() {
    let handing = Authority::default().begin_handover().unwrap();
    let back = handing.abort_handover().unwrap();
    assert_eq!((back.holder, back.epoch), (Holder::Devagotchi, 3));
    conflict(back.fence(1));
    conflict(back.abort_handover());
    conflict(Authority::default().begin_restore());
}

fn clyffy_bundle(epoch: u64, entries: &[Entry]) -> LexiconBundleV1 {
    LexiconBundleV1::new(OWNER, Side::Clyffy, epoch, 99.0, entries)
}

#[test]
fn only_clyffys_current_bundle_is_taken_in() {
    let clyffy = Authority::default()
        .begin_handover()
        .and_then(|a| a.complete_handover(2))
        .unwrap();
    assert!(clyffy.accepts(&clyffy_bundle(2, &[])).is_ok());
    assert!(conflict(clyffy.accepts(&clyffy_bundle(1, &[]))).contains("epoch 1"));
    let ours = LexiconBundleV1::new(OWNER, Side::Devagotchi, 2, 1.0, &[]);
    assert!(conflict(clyffy.accepts(&ours)).contains("only Clyffy"));
    assert!(conflict(Authority::default().accepts(&clyffy_bundle(1, &[]))).contains("devagotchi"));

    let restoring = clyffy.begin_restore().unwrap();
    assert!(
        restoring.accepts(&clyffy_bundle(2, &[])).is_ok(),
        "Clyffy's last bundle"
    );
    assert!(restoring.accepts(&clyffy_bundle(3, &[])).is_ok());
    conflict(restoring.accepts(&clyffy_bundle(1, &[])));
}

// ---- The bundle ----

#[test]
fn an_entry_clyffy_wrote_reads_as_it_is() {
    let json = r#"{
        "format": "devagotchi.lexicon.bundle/1", "owner_id": "person-1", "from": "clyffy",
        "epoch": 2, "exported_at": 1700000000.5,
        "entries": [{
            "id": "lex_01", "owner_id": "person-1", "scope": {"kind": "personal"},
            "phrase": "the panel", "meaning": "the admin UI",
            "examples": ["open the panel"], "counterexamples": [],
            "state": "confirmed", "revision": 4,
            "created_at": 1699990000.0, "updated_at": 1699999999.25,
            "provenance": {"source_kind": "user", "project_id": null, "workflow_id": null,
                           "turn_id": null},
            "indexed_revision": 3
        }]
    }"#;
    let bundle = LexiconBundleV1::read(json).unwrap();
    assert_eq!(bundle.entries[0].indexed_revision, Some(3));
    let entries = bundle.entries();
    assert_eq!(entries[0].owner_id, "person-1");
    assert_eq!(entries[0].wording.examples, ["open the panel"]);
    assert!(entries[0].wording.concepts.is_empty());
    assert_eq!(entries[0].state, State::Confirmed);

    let again = serde_json::to_string(&bundle).unwrap();
    assert_eq!(
        LexiconBundleV1::read(&again).unwrap(),
        bundle,
        "it writes back as it read"
    );
}

#[test]
fn a_bundle_that_does_not_hold_together_is_refused() {
    let good = entry("a", "the panel", "the admin UI", State::Confirmed, 1, 1.0);
    let invalid = |bundle: LexiconBundleV1| match bundle.check() {
        Err(LexiconError::Invalid(message)) => message,
        other => panic!("expected invalid, got {other:?}"),
    };
    let mut wrong_format = clyffy_bundle(1, &[good.clone()]);
    wrong_format.format = "something/2".into();
    assert!(invalid(wrong_format).contains("format"));
    assert!(invalid(clyffy_bundle(1, &[good.clone(), good.clone()])).contains("twice"));
    let mut other_owner = clyffy_bundle(1, &[good.clone()]);
    other_owner.entries[0].owner_id = Some("person-2".into());
    assert!(invalid(other_owner).contains("another owner"));
    let mut zero = clyffy_bundle(1, &[good.clone()]);
    zero.entries[0].revision = 0;
    assert!(invalid(zero).contains("revision 0"));
    let mut bad_id = clyffy_bundle(1, &[good]);
    bad_id.entries[0].id = "has space".into();
    invalid(bad_id);
    let unknown = r#"{"format":"devagotchi.lexicon.bundle/1","owner_id":"o","from":"clyffy",
        "epoch":1,"exported_at":0,"entries":[],"extra":1}"#;
    assert!(matches!(
        LexiconBundleV1::read(unknown),
        Err(LexiconError::Invalid(_))
    ));
}

// ---- A whole handover and restore ----

#[test]
fn a_lexicon_goes_to_clyffy_and_comes_back_with_what_was_added_there() {
    let mut panel = entry("a", "the panel", "the admin UI", State::Confirmed, 1, 10.0);
    panel.wording.concepts = vec!["boundary/admin".into()];
    let mut someone_elses = panel.clone();
    someone_elses.id = "z".into();
    someone_elses.owner_id = "person-2".into();
    let ours = vec![panel.clone(), someone_elses];

    let authority = Authority::default();
    conflict(export(&authority, OWNER, 50.0, &ours));
    let handing = authority.begin_handover().unwrap();
    let out = export(&handing, OWNER, 50.0, &ours).unwrap();
    assert_eq!((out.from, out.epoch), (Side::Devagotchi, 2));
    assert_eq!(
        out.entries(),
        vec![panel.clone()],
        "only this person's, concepts kept"
    );
    let clyffy = handing.complete_handover(2).unwrap();

    // Clyffy added a word and revised one; it sends them back through the JSON.
    let mut revised = panel.clone();
    revised.revision = 2;
    revised.updated_at = 60.0;
    revised.wording.examples = vec!["the panel is down".into()];
    let added = entry("b", "the board", "the kanban", State::Proposed, 1, 70.0);
    let sent = serde_json::to_string(&clyffy_bundle(2, &[revised.clone(), added.clone()])).unwrap();
    let bundle = LexiconBundleV1::read(&sent).unwrap();

    let mirror = take_in(&clyffy, &ours, &bundle).unwrap();
    assert_eq!(mirror.entries, vec![revised.clone(), added.clone()]);

    let restoring = clyffy.begin_restore().unwrap();
    let back = take_in(&restoring, &ours, &bundle).unwrap();
    assert_eq!(back.entries, mirror.entries);
    let home = restoring.complete_restore(3).unwrap();
    assert!(home.fence(3).is_ok());
    assert!(conflict(take_in(&home, &ours, &bundle)).contains("devagotchi"));
}
