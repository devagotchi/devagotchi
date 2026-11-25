//! The authority's moves, the bundle as it crosses, and a whole handover and restore.

use super::*;

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
fn a_preview_shows_what_the_person_settles_before_a_handover() {
    let ours = entry("a", "the panel", "the admin UI", State::Confirmed, 1, 10.0);
    let theirs = entry(
        "b",
        "The Panel",
        "the billing page",
        State::Confirmed,
        1,
        20.0,
    );
    let bundle = clyffy_bundle(1, &[theirs.clone()]);
    let seen = preview(OWNER, &[ours.clone()], &bundle).unwrap();
    assert_eq!(seen.conflicts.len(), 1);
    assert_eq!(ids(&seen.conflicts[0].entries), ["a", "b"]);

    let handing = Authority::default().begin_handover().unwrap();
    let stale = export(&handing, OWNER, 30.0, &[ours.clone()], Some(&bundle));
    assert!(
        conflict(stale).contains("epoch 1"),
        "a bundle from before the handover"
    );
    let both = export(
        &handing,
        OWNER,
        30.0,
        &[ours],
        Some(&clyffy_bundle(2, &[theirs])),
    );
    assert!(conflict(both).contains("settle them first"));
    let ours = LexiconBundleV1::new(OWNER, Side::Devagotchi, 2, 1.0, &[]);
    assert!(matches!(
        preview(OWNER, &[], &ours),
        Err(LexiconError::Invalid(_))
    ));
    let mut bobs = clyffy_bundle(2, &[]);
    bobs.owner_id = "person-2".into();
    assert!(matches!(
        preview(OWNER, &[], &bobs),
        Err(LexiconError::Invalid(_))
    ));
}

#[test]
fn a_bundle_never_holds_two_live_entries_for_one_phrase() {
    let a = entry("a", "the panel", "the admin UI", State::Proposed, 1, 1.0);
    let b = entry(
        "b",
        "The  Panel",
        "the billing page",
        State::Proposed,
        1,
        2.0,
    );
    match clyffy_bundle(1, &[a.clone(), b]).check() {
        Err(LexiconError::Invalid(message)) => {
            assert!(message.contains("second live"), "{message}")
        }
        other => panic!("expected invalid, got {other:?}"),
    }
    let gone = entry("c", "the panel", "old", State::Retired, 2, 0.5);
    assert!(clyffy_bundle(1, &[a, gone]).check().is_ok());
}

#[test]
fn a_lexicon_goes_to_clyffy_and_comes_back_with_what_was_added_there() {
    let mut panel = entry("a", "the panel", "the admin UI", State::Confirmed, 1, 10.0);
    panel.wording.concepts = vec!["boundary/admin".into()];
    let guess = entry("g", "the board", "a guess", State::Proposed, 1, 5.0);
    let mut someone_elses = panel.clone();
    someone_elses.id = "z".into();
    someone_elses.owner_id = "person-2".into();
    let ours = vec![panel.clone(), guess.clone(), someone_elses];

    let authority = Authority::default();
    conflict(export(&authority, OWNER, 50.0, &ours, None));
    let handing = authority.begin_handover().unwrap();
    // Clyffy had its own word for the board, confirmed: it replaces Devagotchi's guess.
    let board = entry("b", "the board", "the kanban", State::Confirmed, 1, 8.0);
    let out = export(
        &handing,
        OWNER,
        50.0,
        &ours,
        Some(&clyffy_bundle(2, &[board.clone()])),
    )
    .unwrap();
    assert_eq!(
        (out.bundle.from, out.bundle.epoch, out.superseded),
        (Side::Devagotchi, 2, 1)
    );
    let mut dismissed = guess;
    dismissed.state = State::Dismissed;
    dismissed.revision = 2;
    dismissed.updated_at = 50.0;
    assert_eq!(
        out.bundle.entries(),
        vec![panel.clone(), board.clone(), dismissed.clone()],
        "only this person's, concepts kept, every id kept"
    );
    let clyffy = handing.complete_handover(2).unwrap();

    // Clyffy revised a word and added one; it sends its lexicon back through the JSON.
    let mut revised = panel.clone();
    revised.revision = 2;
    revised.updated_at = 60.0;
    revised.wording.examples = vec!["the panel is down".into()];
    let added = entry("d", "the deck", "the slides", State::Proposed, 1, 70.0);
    let now = [revised.clone(), board, dismissed, added];
    let sent = serde_json::to_string(&clyffy_bundle(2, &now)).unwrap();
    let bundle = LexiconBundleV1::read(&sent).unwrap();
    assert_eq!(
        take_in(&clyffy, OWNER, &bundle).unwrap(),
        now,
        "the mirror is Clyffy's lexicon"
    );

    let restoring = clyffy.begin_restore().unwrap();
    assert_eq!(
        take_in(&restoring, OWNER, &bundle).unwrap(),
        now,
        "its last bundle comes back"
    );
    let home = restoring.complete_restore(3).unwrap();
    assert!(home.fence(3).is_ok());
    assert!(conflict(take_in(&home, OWNER, &bundle)).contains("devagotchi"));
    assert!(matches!(
        take_in(&restoring, "person-2", &bundle),
        Err(LexiconError::Invalid(_))
    ));
}
