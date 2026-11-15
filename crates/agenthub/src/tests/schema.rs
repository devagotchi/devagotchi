//! The board file: the change signal, and a version 1 board migrated, an unknown version refused.

use super::*;

#[test]
fn a_committed_change_advances_the_signal_and_an_empty_reap_does_not() {
    let f = fixture();
    let mut changes = f.board.changes();
    assert!(!changes.has_changed().unwrap());
    let id = f.new_ticket(&Actor::human("jesse"), "wake the waiters");
    assert!(
        changes.has_changed().unwrap(),
        "a committed event wakes waiters"
    );
    changes.borrow_and_update();
    assert_eq!(f.board.reap().unwrap(), 0);
    assert!(
        !changes.has_changed().unwrap(),
        "a write that records nothing wakes nobody"
    );
    assert_eq!(f.board.next_lease_expiry().unwrap(), None);
    f.board
        .claim(&Actor::agent("claude"), &id, Some(30_000))
        .unwrap();
    let now = f.now.load(Ordering::SeqCst);
    assert_eq!(f.board.next_lease_expiry().unwrap(), Some(now + 30_000));
    assert!(changes.has_changed().unwrap());
}

/// Turn a schema-2 file back into what schema 1 wrote: no anchor columns or indexes,
/// settings without `repo` / `environments`, version 1.
fn downgrade_to_v1(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).expect("open raw");
    connection
        .execute_batch(
            "DROP INDEX tickets_machine; DROP INDEX tickets_environment; DROP INDEX tickets_fingerprint;
             ALTER TABLE tickets DROP COLUMN machine; ALTER TABLE tickets DROP COLUMN repo;
             ALTER TABLE tickets DROP COLUMN environment; ALTER TABLE tickets DROP COLUMN agent;
             ALTER TABLE tickets DROP COLUMN session; ALTER TABLE tickets DROP COLUMN fingerprint;
             ALTER TABLE tickets DROP COLUMN occurrences; ALTER TABLE tickets DROP COLUMN last_seen_at;
             UPDATE meta SET value = '1' WHERE key = 'schema_version';",
        )
        .expect("downgrade");
    let raw: String = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'settings'",
            [],
            |r| r.get(0),
        )
        .expect("settings");
    let mut settings: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let object = settings.as_object_mut().expect("object");
    object.remove("repo");
    object.remove("environments");
    connection
        .execute(
            "UPDATE settings SET value = ?1 WHERE key = 'settings'",
            [settings.to_string()],
        )
        .expect("old settings");
}

#[test]
fn a_version_1_board_migrates_and_keeps_every_ticket_note_and_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("board.sqlite3");
    let jesse = Actor::human("jesse");
    let (ids, notes, events) = {
        let board = Board::open(&path, "devagotchi").expect("open");
        let a = board
            .create(
                &jesse,
                NewTicket {
                    title: "one".into(),
                    ..NewTicket::default()
                },
            )
            .expect("a");
        let b = board
            .create(
                &jesse,
                NewTicket {
                    title: "two".into(),
                    parent: Some(a.id.clone()),
                    ..NewTicket::default()
                },
            )
            .expect("b");
        board
            .add_note(&jesse, &a.id, "note", "kept", None, None)
            .expect("note");
        board
            .add_dep(&jesse, &b.id, &a.id, DepKind::Blocks)
            .expect("dep");
        let events = board.events(0, 1000).expect("events");
        (
            vec![a.id, b.id],
            board.get(&events[0].ticket.clone().unwrap()).unwrap().notes,
            events,
        )
    };
    downgrade_to_v1(&path);

    let board = Board::open(&path, "devagotchi").expect("a version-1 board opens");
    let listed: Vec<String> = board
        .list(&Filter::default())
        .expect("list")
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(listed.len(), 2);
    for id in &ids {
        assert!(listed.contains(id), "{id} kept");
        let ticket = board.get(id).expect("get").ticket;
        assert_eq!(ticket.machine, None);
        assert_eq!(ticket.occurrences, 1);
    }
    assert_eq!(board.get(&ids[0]).unwrap().notes, notes);
    assert_eq!(
        board.get(&ids[1]).unwrap().ticket.blocked_by,
        vec![ids[0].clone()]
    );
    let after = board.events(0, 1000).expect("events");
    assert_eq!(&after[..events.len()], &events[..], "every event kept");
    let migrated = after.last().expect("migration event");
    assert_eq!(migrated.action, "board.migrated");
    assert_eq!(migrated.data, serde_json::json!({ "from": 1, "to": 2 }));
    let chain = board.verify_chain().expect("verify");
    assert!(chain.ok, "{chain:?}");
    assert_eq!(chain.entries as usize, events.len() + 1);
    assert_eq!(board.settings().expect("settings"), Settings::default());
    board
        .create(
            &jesse,
            NewTicket {
                title: "anchored after migrating".into(),
                anchors: Anchors {
                    machine: Some("dabo".into()),
                    ..Anchors::default()
                },
                ..NewTicket::default()
            },
        )
        .expect("the new columns work");
    drop(board);
    let again = Board::open(&path, "devagotchi").expect("reopen");
    assert_eq!(
        again
            .events(0, 1000)
            .unwrap()
            .iter()
            .filter(|e| e.action == "board.migrated")
            .count(),
        1
    );
}

#[test]
fn an_unknown_schema_version_is_still_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("board.sqlite3");
    drop(Board::open(&path, "devagotchi").expect("open"));
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE meta SET value = '3' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
    match Board::open(&path, "devagotchi") {
        Ok(_) => panic!("schema 3 must be refused"),
        Err(error) => assert_eq!(error.code, "board_version"),
    }
}
