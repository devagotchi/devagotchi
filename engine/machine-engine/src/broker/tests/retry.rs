//! The retry lifecycle (DG-RETRY-1): a key's identity survives tuning, changed inputs and a
//! changed policy conflict, a revoked grant refuses, a refused start evicts no history, and a
//! smaller history (after a restart or live) retires operations without forgetting a key.

use super::*;

/// What the retried shell line has appended so far.
fn effects(directory: &tempfile::TempDir) -> Vec<u8> {
    std::fs::read(directory.path().join("effects.txt")).unwrap()
}

#[tokio::test]
async fn retry_tuning_preserves_one_effect_but_changed_inputs_and_revocation_refuse() {
    let directory = tempfile::tempdir().unwrap();
    let (executable, args) = shell(&["echo effect>>effects.txt"]);
    let config = limited_config(&directory, &[&executable], 4);
    let state = devagotchi_ipc::private_test_state(directory.path());
    // A minute, not input()'s five seconds: a loaded Windows runner starts cmd.exe slowly.
    let mut request = input("one-effect", &executable, args);
    request.deadline_ms = 60_000;
    let broker = Broker::open(config.clone(), &state).unwrap();
    let admitted = broker.start(request.clone()).unwrap();
    let id = admitted.operation.operation_id;
    let settled = settle(&broker, &id).await;
    assert!(
        matches!(settled.state, OperationState::Succeeded),
        "{settled:?}: {}",
        output_text(&broker, &id)
    );
    let effect = effects(&directory);
    assert!(!effect.is_empty());
    broker.shutdown().await;
    drop(broker);

    // More history, more running, more output and an unrelated grant: the same operation.
    let mut tuned = config.clone();
    tuned.max_operations = 8;
    tuned.max_concurrent = 2;
    tuned.output_limit_bytes *= 2;
    tuned
        .allowed_executables
        .push(test_executable().display().to_string());
    let broker = Broker::open(tuned.clone(), &state).unwrap();
    let replay = broker.start(request.clone()).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.operation.operation_id, id);
    // Other arguments, another deadline, another cwd: another request.
    let mut changed = request.clone();
    changed.args.push("different".into());
    assert_eq!(
        broker.start(changed).unwrap_err().code,
        "idempotency_conflict"
    );
    let mut changed = request.clone();
    changed.deadline_ms += 1;
    assert_eq!(
        broker.start(changed).unwrap_err().code,
        "idempotency_conflict"
    );
    let other = directory.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let mut changed = request.clone();
    changed.cwd = Some(other.display().to_string());
    assert_eq!(
        broker.start(changed).unwrap_err().code,
        "idempotency_conflict"
    );
    broker.shutdown().await;
    drop(broker);

    // Another policy is another identity.
    let mut account = tuned.clone();
    account.exec_policy = ExecPolicy::Account;
    let broker = Broker::open(account, &state).unwrap();
    assert_eq!(
        broker.start(request.clone()).unwrap_err().code,
        "idempotency_conflict"
    );
    broker.shutdown().await;
    drop(broker);

    // A revoked grant refuses even an exact replay; granted again, it replays.
    let mut revoked = tuned;
    revoked.allowed_executables.clear();
    let broker = Broker::open(revoked, &state).unwrap();
    assert_eq!(
        broker.start(request.clone()).unwrap_err().code,
        "executable_not_allowed"
    );
    broker.shutdown().await;
    drop(broker);
    let broker = Broker::open(config, &state).unwrap();
    assert!(broker.start(request).unwrap().replayed);
    assert_eq!(effects(&directory), effect, "the work ran once");
    broker.shutdown().await;
}

#[tokio::test]
async fn concurrency_refusal_preserves_history_and_smaller_limit_preserves_retry_keys() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let (long, args) = shell(&["/bin/sleep 30"]);
    #[cfg(windows)]
    let (long, args) = (
        system32("PING.EXE"),
        vec!["-n".to_owned(), "30".to_owned(), "127.0.0.1".to_owned()],
    );
    let quick = test_executable();
    let config = limited_config(&directory, &[&long, &quick], 2);
    let state = devagotchi_ipc::private_test_state(directory.path());
    let broker = Broker::open(config.clone(), &state).unwrap();
    let first = start(&broker, "history", &quick, vec![], 5_000);
    settle(&broker, &first).await;
    let second = start(&broker, "active", &long, args, 60_000);
    let refused = broker.start(input("refused", &quick, vec![])).unwrap_err();
    assert_eq!(refused.code, "concurrency_limit_reached");
    assert!(
        broker.get(&first, None).is_ok(),
        "a refused start evicted finished history"
    );
    assert!(!broker.shared.lock().journal.was_retired("history").unwrap());
    broker.cancel(&second, None).unwrap();
    settle(&broker, &second).await;
    broker.shutdown().await;
    drop(broker);

    // Restarted holding fewer: the next start retires down to the limit, keys kept.
    let mut smaller = config;
    smaller.max_operations = 1;
    let broker = Broker::open(smaller, &state).unwrap();
    let third = start(&broker, "new", &quick, vec![], 5_000);
    settle(&broker, &third).await;
    let listed = broker.list(ListInput {
        offset: 0,
        limit: 64,
        scope: None,
        caller: None,
    });
    assert_eq!(listed.unwrap().operations.len(), 1);
    let retired = broker.start(input("history", &quick, vec![])).unwrap_err();
    assert_eq!(retired.code, "idempotency_key_retired");
    assert!(broker.shared.lock().journal.was_retired("active").unwrap());
    broker.shutdown().await;
}

/// An account broker started from a broker.json holding four operations, and its state.
fn from_file(directory: &tempfile::TempDir, file: &Path) -> (Arc<Broker>, PathBuf) {
    let mut config = limited_config(directory, &[], 4);
    config.exec_policy = ExecPolicy::Account;
    std::fs::write(file, serde_json::to_vec(&config).unwrap()).unwrap();
    let state = devagotchi_ipc::private_test_state(directory.path());
    let broker = Broker::open_at(config, &state, Some(file.to_path_buf())).unwrap();
    (broker, state)
}

fn set(broker: &Broker, key: &str, value: u64) -> BridgeResult<ConfigView> {
    broker.set_config(ConfigSetInput {
        key: key.into(),
        value,
    })
}

fn saved_max_operations(file: &Path) -> u64 {
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    saved["max_operations"].as_u64().unwrap()
}

#[tokio::test]
async fn a_live_smaller_history_is_saved_first_and_keeps_every_retired_key() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("broker.json");
    let (broker, _) = from_file(&directory, &file);
    let quick = test_executable();
    for key in ["one", "two", "three"] {
        let id = start(&broker, key, &quick, vec![], 5_000);
        settle(&broker, &id).await;
    }
    let view = set(&broker, "max_operations", 1).unwrap();
    assert_eq!((view.max_operations, view.operations_held), (1, 1));
    assert_eq!(saved_max_operations(&file), 1);
    for key in ["one", "two"] {
        let retired = broker.start(input(key, &quick, vec![])).unwrap_err();
        assert_eq!(retired.code, "idempotency_key_retired");
    }
    assert!(
        broker
            .start(input("three", &quick, vec![]))
            .unwrap()
            .replayed
    );

    // Not saved, not applied: the live limit stays what broker.json says.
    std::fs::remove_file(&file).unwrap();
    let refused = set(&broker, "max_operations", 2).unwrap_err();
    assert_eq!(refused.code, "config_not_saved");
    assert_eq!(broker.config_view(None).unwrap().max_operations, 1);
    broker.shutdown().await;
}

#[tokio::test]
async fn a_journal_error_while_retiring_is_returned_not_swallowed_or_faulted() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("broker.json");
    let (broker, state) = from_file(&directory, &file);
    let quick = test_executable();
    let mut ids = Vec::new();
    for key in ["oldest", "middle", "newest"] {
        let id = start(&broker, key, &quick, vec![], 5_000);
        settle(&broker, &id).await;
        ids.push(id);
    }
    // A tombstone already holding the oldest operation's key: retiring it must fail.
    rusqlite::Connection::open(state.join("operations.sqlite3"))
        .unwrap()
        .execute("INSERT INTO retired_keys VALUES('oldest', 'other', 1)", [])
        .unwrap();
    let stopped = set(&broker, "max_operations", 1).unwrap_err();
    assert_eq!(stopped.code, "persistence_error");
    assert!(stopped.message.contains("stopped"), "{}", stopped.message);
    // Saved and applied before retiring began: the file and the live limit agree.
    assert_eq!(saved_max_operations(&file), 1);
    assert_eq!(broker.config_view(None).unwrap().max_operations, 1);
    // The retirement rolled back whole, and admission propagates the same error.
    assert!(broker.get(&ids[0], None).is_ok());
    let refused = broker.start(input("next", &quick, vec![])).unwrap_err();
    assert_eq!(refused.code, "persistence_error");
    assert!(broker.shared.lock().persistence_fault.is_none());
    broker.shutdown().await;
}
