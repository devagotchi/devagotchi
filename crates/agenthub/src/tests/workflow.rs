//! A board on the previous default workflow gets the reopen transitions once; a customised one is left as it is.

use super::*;

/// Store `workflow` as the board's, the way an earlier build wrote it: no event.
fn store_workflow(path: &std::path::Path, workflow: &Workflow) {
    rusqlite::Connection::open(path)
        .expect("open raw")
        .execute(
            "UPDATE settings SET value = ?1 WHERE key = 'workflow'",
            [serde_json::to_string(workflow).expect("json")],
        )
        .expect("old workflow");
}

#[test]
fn a_board_on_the_previous_default_workflow_gets_the_reopen_transitions_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("board.sqlite3");
    let jesse = Actor::human("jesse");
    let previous = Workflow::previous_default();
    assert!(!previous.allows("cancelled", "ready") && !previous.allows("done", "ready"));
    assert!(previous.validate().is_ok());
    let (id, events) = {
        let board = Board::open(&path, "devagotchi").expect("open");
        let id = board
            .create(
                &jesse,
                NewTicket {
                    title: "closed before the upgrade".into(),
                    ..NewTicket::default()
                },
            )
            .expect("create")
            .id;
        let ticket = board.get(&id).expect("get").ticket;
        board
            .update(
                &jesse,
                &id,
                ticket.version,
                Patch {
                    state: Some("cancelled".into()),
                    ..Patch::default()
                },
            )
            .expect("cancel");
        (id, board.events(0, 1000).expect("events"))
    };
    store_workflow(&path, &previous);

    let board = Board::open(&path, "devagotchi").expect("reopen");
    assert_eq!(
        board.workflow().expect("workflow"),
        Workflow::default_workflow()
    );
    let after = board.events(0, 1000).expect("events");
    assert_eq!(&after[..events.len()], &events[..], "every event kept");
    let upgraded = after.last().expect("upgrade event");
    assert_eq!(upgraded.action, "board.workflow_upgraded");
    assert_eq!(upgraded.actor, "system");
    assert_eq!(
        upgraded.data,
        serde_json::json!({ "id": "default", "added": [
            { "from": "done", "to": "ready" }, { "from": "cancelled", "to": "ready" }] })
    );
    let chain = board.verify_chain().expect("verify");
    assert!(chain.ok, "{chain:?}");
    assert_eq!(chain.entries as usize, events.len() + 1);

    // The ticket closed before the upgrade reopens now.
    let ticket = board.get(&id).expect("get").ticket;
    let reopened = board
        .update(
            &jesse,
            &id,
            ticket.version,
            Patch {
                state: Some("ready".into()),
                ..Patch::default()
            },
        )
        .expect("cancelled -> ready");
    assert_eq!(reopened.state, "ready");
    drop(board);
    let again = Board::open(&path, "devagotchi").expect("reopen again");
    let upgrades = again
        .events(0, 1000)
        .unwrap()
        .iter()
        .filter(|e| e.action == "board.workflow_upgraded")
        .count();
    assert_eq!(upgrades, 1, "upgraded once");
    assert!(again.verify_chain().expect("verify").ok);
}

#[test]
fn a_customised_workflow_is_left_as_it_is() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("board.sqlite3");
    drop(Board::open(&path, "devagotchi").expect("open"));
    // The previous default with one change of its owner's: no longer the default.
    let mut ours = Workflow::previous_default();
    ours.name = "Ours".into();
    store_workflow(&path, &ours);
    let board = Board::open(&path, "devagotchi").expect("reopen");
    assert_eq!(board.workflow().expect("workflow"), ours);
    assert!(
        board
            .events(0, 1000)
            .unwrap()
            .iter()
            .all(|e| e.action != "board.workflow_upgraded")
    );
    // The current default, and a new board, are left alone too.
    let fresh = dir.path().join("fresh.sqlite3");
    drop(Board::open(&fresh, "devagotchi").expect("new"));
    let board = Board::open(&fresh, "devagotchi").expect("reopen new");
    assert_eq!(
        board.workflow().expect("workflow"),
        Workflow::default_workflow()
    );
    assert!(
        board
            .events(0, 1000)
            .unwrap()
            .iter()
            .all(|e| e.action != "board.workflow_upgraded")
    );
}
