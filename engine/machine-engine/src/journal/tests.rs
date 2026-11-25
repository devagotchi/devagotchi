use super::*;
use devagotchi_contracts::OperationState;

#[test]
fn journal_reopens_exact_bytes_and_idempotency_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let state = devagotchi_ipc::private_test_state(directory.path());
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut journal = Journal::open(&state).unwrap();
        journal
            .insert(&JournalOperation {
                key: "retry-key".into(),
                request_digest: "request-digest".into(),
                config_identity: "configuration-digest".into(),
                owner: None,
                grant: None,
                view: OperationView {
                    operation_id: id.clone(),
                    executable: "/usr/bin/test".into(),
                    args: vec![],
                    cwd: "/tmp".into(),
                    deadline_ms: 1000,
                    state: OperationState::Accepted,
                    created_at_ms: 1,
                    updated_at_ms: 1,
                    exit_code: None,
                    detail: None,
                    ownership_scope: "test".into(),
                    cancellation_requested: false,
                    output_complete: false,
                    interactive: None,
                },
                latest_sequence: 0,
                frames: VecDeque::new(),
            })
            .unwrap();
        journal
            .append(
                &id,
                &JournalFrame {
                    sequence: 1,
                    stream: "stdout".into(),
                    bytes: vec![0, 255, 128],
                    text: "\0��".into(),
                },
                1,
            )
            .unwrap();
    }
    let journal = Journal::open(&state).unwrap();
    let loaded = journal.load().unwrap();
    assert_eq!(loaded[0].key, "retry-key");
    assert_eq!(loaded[0].config_identity, "configuration-digest");
    assert_eq!(loaded[0].frames[0].bytes, [0, 255, 128]);
    assert_eq!(loaded[0].latest_sequence, 1);
    assert!(matches!(loaded[0].view.state, OperationState::Accepted));
}

#[cfg(unix)]
#[test]
fn journal_rejects_hardlinks_and_sidecar_collisions_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("unrelated");
    std::fs::write(&target, b"keep this exact content").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    let state = directory.path().join("state");
    std::fs::create_dir(&state).unwrap();
    std::fs::hard_link(&target, state.join("operations.sqlite3")).unwrap();
    assert!(Journal::open(&state).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep this exact content");
    std::fs::remove_file(state.join("operations.sqlite3")).unwrap();
    std::os::unix::fs::symlink(&target, state.join("operations.sqlite3-wal")).unwrap();
    assert!(Journal::open(&state).is_err());
    assert!(!state.join("operations.sqlite3").exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep this exact content");
}

#[cfg(windows)]
#[test]
fn journal_rejects_hardlinks_and_sidecar_collisions_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("unrelated");
    std::fs::write(&target, b"keep this exact content").unwrap();
    let state = devagotchi_ipc::private_test_state(directory.path());
    std::fs::hard_link(&target, state.join("operations.sqlite3")).unwrap();
    assert!(Journal::open(&state).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep this exact content");
    std::fs::remove_file(state.join("operations.sqlite3")).unwrap();
    std::fs::hard_link(&target, state.join("operations.sqlite3-wal")).unwrap();
    assert!(Journal::open(&state).is_err());
    assert!(!state.join("operations.sqlite3").exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep this exact content");
}

#[test]
fn an_interactive_operations_owner_and_input_state_outlive_a_restart() {
    use devagotchi_contracts::{Caller, InputMode, InteractiveView};
    let directory = tempfile::tempdir().unwrap();
    let state = devagotchi_ipc::private_test_state(directory.path());
    let id = uuid::Uuid::new_v4().to_string();
    let owner = Caller {
        account: "hub:jesse".into(),
        client: "chat-1".into(),
    };
    let mut view = OperationView {
        operation_id: id.clone(),
        executable: "/bin/sh".into(),
        args: vec![],
        cwd: "/tmp".into(),
        deadline_ms: 1000,
        state: OperationState::Running,
        created_at_ms: 1,
        updated_at_ms: 1,
        exit_code: None,
        detail: None,
        ownership_scope: "test".into(),
        cancellation_requested: false,
        output_complete: false,
        interactive: Some(InteractiveView {
            mode: InputMode::Terminal,
            cols: 80,
            rows: 24,
            input_open: true,
            writer_until_ms: None,
        }),
    };
    {
        let mut journal = Journal::open(&state).unwrap();
        journal
            .insert(&JournalOperation {
                key: "interactive".into(),
                request_digest: "digest".into(),
                config_identity: "config".into(),
                owner: Some(owner.clone()),
                grant: None,
                view: view.clone(),
                latest_sequence: 0,
                frames: VecDeque::new(),
            })
            .unwrap();
        view.interactive.as_mut().unwrap().cols = 132;
        journal.update(&view).unwrap();
    }
    let loaded = Journal::open(&state).unwrap().load().unwrap();
    assert_eq!(loaded[0].owner.as_ref(), Some(&owner));
    let kept = loaded[0]
        .view
        .interactive
        .as_ref()
        .expect("its input state");
    assert_eq!(
        (kept.mode, kept.cols, kept.rows),
        (InputMode::Terminal, 132, 24)
    );
}

fn accepted(id: &str) -> JournalOperation {
    JournalOperation {
        key: format!("key-of-{id}"),
        request_digest: "digest".into(),
        config_identity: "config".into(),
        owner: None,
        grant: None,
        view: OperationView {
            operation_id: id.into(),
            executable: "/usr/bin/test".into(),
            args: vec![],
            cwd: "/tmp".into(),
            deadline_ms: 1000,
            state: OperationState::Succeeded,
            created_at_ms: 1,
            updated_at_ms: 1,
            exit_code: Some(0),
            detail: None,
            ownership_scope: "test".into(),
            cancellation_requested: false,
            output_complete: true,
            interactive: None,
        },
        latest_sequence: 0,
        frames: VecDeque::new(),
    }
}

fn tombstones(journal: &Journal) -> i64 {
    journal
        .connection
        .query_row("SELECT COUNT(*) FROM retired_keys", [], |row| row.get(0))
        .unwrap()
}

/// Past the 4096 the journal once kept, a retirement forgets no key, and no statement on
/// any connection deletes, changes or replaces a tombstone; a transaction that tries rolls
/// back whole.
#[test]
fn retry_evidence_is_permanent_and_immutable() {
    let directory = tempfile::tempdir().unwrap();
    let state = devagotchi_ipc::private_test_state(directory.path());
    let mut journal = Journal::open(&state).unwrap();
    // One transaction for the fixture: 4097 fsynced commits would be most of the test's time.
    let transaction = journal.connection.transaction().unwrap();
    for n in 0..4097 {
        transaction
            .execute(
                "INSERT INTO retired_keys VALUES(?1, ?2, ?3)",
                params![format!("old-{n}"), format!("digest-{n}"), n],
            )
            .unwrap();
    }
    transaction.commit().unwrap();

    // The real retirement keeps every earlier key.
    let id = uuid::Uuid::new_v4().to_string();
    let operation = accepted(&id);
    journal.insert(&operation).unwrap();
    journal
        .retire(&id, &operation.key, &operation.request_digest, 5000)
        .unwrap();
    assert_eq!(tombstones(&journal), 4098);
    assert!(journal.was_retired("old-0").unwrap());
    assert!(journal.was_retired(&operation.key).unwrap());

    // Retiring a key again fails and leaves the operation it would have retired in place.
    let again = uuid::Uuid::new_v4().to_string();
    let mut duplicate = accepted(&again);
    duplicate.key = "old-1".into();
    journal.insert(&duplicate).unwrap();
    assert!(journal.retire(&again, "old-1", "changed", 6000).is_err());
    assert_eq!(
        journal.load().unwrap().len(),
        1,
        "the failed retirement rolled back"
    );

    // A pruning DELETE inside a retirement fails, and the whole transaction rolls back.
    let transaction = journal.connection.transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO retired_keys VALUES('latest', 'digest', 7000)",
            [],
        )
        .unwrap();
    let pruning = transaction.execute(
        "DELETE FROM retired_keys WHERE rowid NOT IN (SELECT rowid FROM retired_keys ORDER BY retired_at_ms DESC, rowid DESC LIMIT ?1)",
        [4096],
    );
    assert!(pruning.is_err(), "a tombstone was deleted");
    transaction.rollback().unwrap();
    assert!(!journal.was_retired("latest").unwrap());
    let update = "UPDATE retired_keys SET request_digest='changed' WHERE idempotency_key='old-0'";
    assert!(journal.connection.execute(update, []).is_err());
    // REPLACE skips delete triggers with recursive_triggers off (SQLite's default).
    journal
        .connection
        .execute_batch("PRAGMA recursive_triggers=OFF;")
        .unwrap();
    let replace = "INSERT OR REPLACE INTO retired_keys VALUES('old-0', 'changed', 8000)";
    assert!(journal.connection.execute(replace, []).is_err());
    drop(journal);

    let reopened = Journal::open(&state).unwrap();
    assert_eq!(tombstones(&reopened), 4098);
    assert!(reopened.was_retired("old-0").unwrap() && reopened.was_retired("old-4096").unwrap());
    let digest: String = reopened
        .connection
        .query_row(
            "SELECT request_digest FROM retired_keys WHERE idempotency_key='old-0'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(digest, "digest-0");
}

/// What `.ai/docs/retry-lifecycle.md` says of a build that prunes to the newest 4096 (main
/// before DG-RETRY-1) on a journal this build opened: its retirement commits while the
/// journal holds fewer than 4096 tombstones, and fails once its prune would delete one.
#[test]
fn a_pruning_build_fails_only_once_it_would_forget_a_key() {
    fn older_retirement(journal: &mut Journal, key: &str, at: i64) -> rusqlite::Result<()> {
        let transaction = journal.connection.transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO retired_keys(idempotency_key,request_digest,retired_at_ms) VALUES(?1,'digest',?2)",
            params![key, at],
        )?;
        transaction.execute(
            "DELETE FROM retired_keys WHERE rowid NOT IN (SELECT rowid FROM retired_keys ORDER BY retired_at_ms DESC, rowid DESC LIMIT 4096)",
            [],
        )?;
        transaction.commit()
    }
    let directory = tempfile::tempdir().unwrap();
    let state = devagotchi_ipc::private_test_state(directory.path());
    let mut journal = Journal::open(&state).unwrap();
    let transaction = journal.connection.transaction().unwrap();
    for n in 0..4095 {
        transaction
            .execute(
                "INSERT INTO retired_keys VALUES(?1, 'digest', ?2)",
                params![format!("old-{n}"), n],
            )
            .unwrap();
    }
    transaction.commit().unwrap();

    older_retirement(&mut journal, "the-4096th", 5000).unwrap();
    assert_eq!(tombstones(&journal), 4096);
    assert!(older_retirement(&mut journal, "the-4097th", 5001).is_err());
    assert_eq!(
        tombstones(&journal),
        4096,
        "the failed retirement rolled back"
    );
    assert!(journal.was_retired("old-0").unwrap() && !journal.was_retired("the-4097th").unwrap());
}
