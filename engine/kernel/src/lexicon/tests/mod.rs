use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

use super::*;

mod resolve;
use crate::dictionary::{Dictionary, LabelKind};

/// The memory store answers at once, so one poll finishes every call.
pub(super) fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the memory store never waits"),
    }
}

pub(super) const ALICE: &str = "github:1001";
pub(super) const BOB: &str = "github:1002";

pub(super) fn lexicon() -> Lexicon<'static, MemoryStore> {
    Lexicon::new(MemoryStore::new(), Dictionary::seed()).with_clock(|| 1_700_000_000.0)
}

pub(super) fn wording(phrase: &str, meaning: &str, concepts: &[&str]) -> Wording {
    Wording {
        phrase: phrase.into(),
        meaning: meaning.into(),
        concepts: concepts.iter().map(|c| c.to_string()).collect(),
        examples: vec![],
        counterexamples: vec![],
    }
}

pub(super) fn change(request: &str, owner: &str, scope: Scope, operation: Operation) -> Change {
    Change {
        request_id: request.into(),
        owner_id: owner.into(),
        scope,
        record_id: None,
        expected_revision: None,
        provenance: Provenance::user(),
        operation,
    }
}

pub(super) fn on(entry: &Entry, request: &str, operation: Operation) -> Change {
    Change {
        record_id: Some(entry.id.clone()),
        expected_revision: Some(entry.revision),
        ..change(request, &entry.owner_id, entry.scope.clone(), operation)
    }
}

pub(super) fn project(id: &str) -> Scope {
    Scope::Project {
        project_id: id.into(),
    }
}

pub(super) fn define(
    l: &Lexicon<'static, MemoryStore>,
    request: &str,
    scope: Scope,
    w: Wording,
) -> Entry {
    run(l.apply(&change(
        request,
        ALICE,
        scope,
        Operation::Define { wording: w },
    )))
    .unwrap()
}

#[test]
fn an_entry_moves_through_its_states_and_keeps_every_revision() {
    let l = lexicon();
    let proposed = run(l.apply(&change(
        "r1",
        ALICE,
        Scope::Personal,
        Operation::Propose {
            wording: wording("  the  pod ", "my dev environment", &[]),
        },
    )))
    .unwrap();
    assert_eq!(
        (
            proposed.state,
            proposed.revision,
            proposed.wording.phrase.as_str()
        ),
        (State::Proposed, 1, "the pod")
    );
    assert!(proposed.id.starts_with("lx_") && proposed.id.len() == 35);
    let confirmed = run(l.apply(&on(&proposed, "r2", Operation::Confirm))).unwrap();
    let retired = run(l.apply(&on(&confirmed, "r3", Operation::Retire))).unwrap();
    assert_eq!((retired.state, retired.revision), (State::Retired, 3));
    let refused = run(l.apply(&on(&retired, "r4", Operation::Confirm))).unwrap_err();
    assert!(
        matches!(refused, LexiconError::Conflict(m) if m.contains("confirm is not allowed on a retired"))
    );
    let history = run(l.history(ALICE, &proposed.id)).unwrap();
    let ops: Vec<&str> = history.iter().map(|e| e.operation.as_str()).collect();
    assert_eq!(ops, ["propose", "confirm", "retire"]);
    assert!(history.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[test]
fn a_stale_revision_and_a_taken_phrase_are_conflicts() {
    let l = lexicon();
    let first = define(
        &l,
        "r1",
        Scope::Personal,
        wording("ship it", "merge and deploy", &[]),
    );
    let revised = run(l.apply(&on(
        &first,
        "r2",
        Operation::Revise {
            wording: wording("ship it", "merge, deploy and announce", &[]),
        },
    )))
    .unwrap();
    assert_eq!(revised.revision, 2);
    let stale = run(l.apply(&on(&first, "r3", Operation::Dismiss))).unwrap_err();
    assert!(matches!(stale, LexiconError::Conflict(m) if m.contains("revision 2, not 1")));
    let again = change(
        "r4",
        ALICE,
        Scope::Personal,
        Operation::Propose {
            wording: wording("Ship  IT", "something else", &[]),
        },
    );
    assert!(matches!(
        run(l.apply(&again)),
        Err(LexiconError::Conflict(_))
    ));
    // The same phrase is free in a project, and for another person.
    let in_project = Change {
        scope: project("p1"),
        request_id: "r5".into(),
        ..again.clone()
    };
    assert!(run(l.apply(&in_project)).is_ok());
    let bobs = Change {
        owner_id: BOB.into(),
        ..again
    };
    assert!(run(l.apply(&bobs)).is_ok());
    // Once dismissed, the phrase is free again.
    let dismissed = run(l.apply(&on(&revised, "r6", Operation::Dismiss))).unwrap();
    assert_eq!(dismissed.state, State::Dismissed);
    define(
        &l,
        "r7",
        Scope::Personal,
        wording("ship it", "tag a release", &[]),
    );
}

#[test]
fn a_request_id_answers_once_and_refuses_a_different_change() {
    let l = lexicon();
    let ask = change(
        "same",
        ALICE,
        Scope::Personal,
        Operation::Define {
            wording: wording("standup", "the 9am call", &[]),
        },
    );
    let first = run(l.apply(&ask)).unwrap();
    assert_eq!(run(l.apply(&ask)).unwrap(), first);
    let other = Change {
        operation: Operation::Define {
            wording: wording("standup", "the 10am call", &[]),
        },
        ..ask.clone()
    };
    let refused = run(l.apply(&other)).unwrap_err();
    assert!(matches!(refused, LexiconError::Conflict(m) if m.contains("another change")));
    // Request ids are per person.
    let bobs = Change {
        owner_id: BOB.into(),
        ..ask
    };
    assert_ne!(run(l.apply(&bobs)).unwrap().id, first.id);
}

#[test]
fn people_never_see_each_others_entries() {
    let l = lexicon();
    let alices = define(
        &l,
        "r1",
        Scope::Personal,
        wording("the box", "my laptop", &[]),
    );
    assert_eq!(run(l.get(BOB, &alices.id)), Err(LexiconError::NotFound));
    let mut bobs_change = on(&alices, "r2", Operation::Dismiss);
    bobs_change.owner_id = BOB.into();
    assert_eq!(run(l.apply(&bobs_change)), Err(LexiconError::NotFound));
    assert!(
        run(l.list(BOB, &Scope::Personal, Filter::All))
            .unwrap()
            .is_empty()
    );
    assert!(run(l.history(BOB, &alices.id)).unwrap().is_empty());
}

#[test]
fn deleting_erases_the_words_everywhere_they_were_kept() {
    let l = lexicon();
    let entry = define(
        &l,
        "r1",
        Scope::Personal,
        wording("the thing", "a secret project", &[]),
    );
    let deleted = run(l.apply(&on(&entry, "r2", Operation::Delete))).unwrap();
    assert_eq!(
        (deleted.state, deleted.wording.clone()),
        (State::Deleted, Wording::default())
    );
    for event in run(l.history(ALICE, &entry.id)).unwrap() {
        assert_eq!(
            event.entry.wording,
            Wording::default(),
            "{}",
            event.operation
        );
    }
    let retried = change(
        "r1",
        ALICE,
        Scope::Personal,
        Operation::Define {
            wording: wording("the thing", "a secret project", &[]),
        },
    );
    assert_eq!(run(l.apply(&retried)).unwrap().wording, Wording::default());
    assert!(matches!(
        run(l.apply(&on(&deleted, "r3", Operation::Delete))),
        Err(LexiconError::Conflict(_))
    ));
    let listed = run(l.list(ALICE, &Scope::Personal, Filter::Reviewable)).unwrap();
    assert!(listed.is_empty());
}

#[test]
fn bad_requests_are_refused_before_anything_is_kept() {
    let l = lexicon();
    let bad = |w: Wording| {
        run(l.apply(&change(
            "x",
            ALICE,
            Scope::Personal,
            Operation::Define { wording: w },
        )))
        .unwrap_err()
    };
    assert!(
        matches!(bad(wording("", "m", &[])), LexiconError::Invalid(m) if m.starts_with("phrase"))
    );
    assert!(
        matches!(bad(wording("p", " ", &[])), LexiconError::Invalid(m) if m.starts_with("meaning"))
    );
    assert!(
        matches!(bad(wording("p", "m", &["boundary/billing"])), LexiconError::Invalid(m) if m.contains("not in the dictionary"))
    );
    assert!(
        matches!(bad(wording("p", "m", &["boundary/iam", "boundary/iam"])), LexiconError::Invalid(m) if m.contains("twice"))
    );
    let mut many = wording("p", "m", &[]);
    many.examples = vec!["e".into(); MAX_ITEMS + 1];
    assert!(matches!(bad(many), LexiconError::Invalid(m) if m.starts_with("examples")));
    let mut spaced = change("x", "has space", Scope::Personal, Operation::Confirm);
    assert!(
        matches!(run(l.apply(&spaced)), Err(LexiconError::Invalid(m)) if m.starts_with("owner_id"))
    );
    spaced.owner_id = ALICE.into();
    assert!(
        matches!(run(l.apply(&spaced)), Err(LexiconError::Invalid(m)) if m.contains("record_id"))
    );
    let mut provenance = change("x", ALICE, project("p1"), Operation::Confirm);
    provenance.provenance.project_id = Some("p2".into());
    assert!(
        matches!(run(l.apply(&provenance)), Err(LexiconError::Invalid(m)) if m.contains("project"))
    );
    assert!(
        run(l.list(ALICE, &Scope::Personal, Filter::All))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_parked_phrase_needs_its_meaning_before_it_is_confirmed() {
    let l = lexicon();
    let parked = run(l.apply(&change(
        "r1",
        ALICE,
        Scope::Personal,
        Operation::Park {
            wording: wording("the gizmo", "", &[]),
        },
    )))
    .unwrap();
    assert_eq!(parked.state, State::Inconclusive);
    let early = run(l.apply(&on(&parked, "r2", Operation::Confirm))).unwrap_err();
    assert!(matches!(early, LexiconError::Conflict(m) if m.contains("says what it means")));
    let said = run(l.apply(&on(
        &parked,
        "r3",
        Operation::Revise {
            wording: wording("the gizmo", "the sign-in page", &["boundary/iam"]),
        },
    )))
    .unwrap();
    assert_eq!(said.state, State::Inconclusive);
    let confirmed = run(l.apply(&on(&said, "r4", Operation::Confirm))).unwrap();
    assert_eq!(confirmed.state, State::Confirmed);
}

#[test]
fn a_projects_meaning_wins_over_the_persons_where_both_define_a_phrase() {
    let l = lexicon();
    define(
        &l,
        "r1",
        Scope::Personal,
        wording("the core", "my notes app", &[]),
    );
    define(
        &l,
        "r2",
        Scope::Personal,
        wording("the box", "my laptop", &[]),
    );
    define(
        &l,
        "r3",
        project("p1"),
        wording("the core", "the Rust engine", &["stack/rust"]),
    );
    let effective = run(l.effective(ALICE, "p1")).unwrap();
    let got: Vec<(&str, &str)> = effective
        .iter()
        .map(|e| (e.wording.phrase.as_str(), e.wording.meaning.as_str()))
        .collect();
    assert_eq!(
        got,
        [("the box", "my laptop"), ("the core", "the Rust engine")]
    );
    let r = run(l.resolve(ALICE, Some("p1"), "the core", "")).unwrap();
    assert_eq!(
        r.outcome,
        Outcome::Resolved {
            concept: "stack/rust".into()
        }
    );
    assert_eq!(r.origin, Some(Origin::Project));
    let personal = run(l.resolve(ALICE, None, "the core", "")).unwrap();
    assert_eq!(
        (personal.outcome, personal.origin),
        (Outcome::Personal, Some(Origin::Personal))
    );
    assert_eq!(personal.meaning.as_deref(), Some("my notes app"));
}
