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
