//! Admission and the operation lifecycle: output and exit status, the account policy,
//! retirement, owned descendants, deadlines, refused configs, journal failure, recovery.

use super::*;

#[tokio::test]
async fn exit_status_and_output_are_recorded() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let (shell, args) = shell(&["echo devagotchi;", "exit 3"]);
    #[cfg(windows)]
    let (shell, args) = shell(&["echo", "devagotchi&", "exit", "3"]);
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "exit-status", &shell, args, 20_000);
    let view = settle(&broker, &id).await;
    assert!(matches!(view.state, OperationState::Failed), "{view:?}");
    assert_eq!(view.exit_code, Some(3));
    assert!(view.output_complete);
    assert_eq!(view.ownership_scope, SCOPE);
    assert!(output_text(&broker, &id).contains("devagotchi"));
    broker.shutdown().await;
}

fn account_broker(directory: &tempfile::TempDir) -> Arc<Broker> {
    let config = BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: vec![],
        exec_policy: ExecPolicy::Account,
        output_limit_bytes: 65_536,
        max_operations: 8,
        max_concurrent: 4,
    };
    Broker::open(
        config,
        &devagotchi_ipc::private_test_state(directory.path()),
    )
    .unwrap()
}

fn command_line(key: &str, line: &str, cwd: Option<String>) -> StartInput {
    StartInput {
        idempotency_key: key.into(),
        executable: String::new(),
        args: vec![],
        command: Some(line.into()),
        cwd,
        deadline_ms: 120_000,
        interactive: None,
        combined_output: false,
        caller: None,
    }
}

fn limited_config(directory: &tempfile::TempDir, grants: &[&Path], max: usize) -> BrokerConfig {
    BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: grants.iter().map(|g| g.display().to_string()).collect(),
        output_limit_bytes: 8192,
        max_operations: max,
        max_concurrent: 1,
        exec_policy: ExecPolicy::Granted,
    }
}

fn input(key: &str, executable: &Path, args: Vec<String>) -> StartInput {
    StartInput {
        idempotency_key: key.into(),
        executable: executable.display().to_string(),
        args,
        command: None,
        cwd: None,
        deadline_ms: 5_000,
        interactive: None,
        combined_output: false,
        caller: None,
    }
}

#[tokio::test]
async fn the_account_policy_runs_a_command_line_in_the_machines_shell() {
    let directory = tempfile::tempdir().unwrap();
    let broker = account_broker(&directory);
    #[cfg(unix)]
    let line = "echo $((6*7)); test -n \"$HOME\" && echo home-set; exit 3";
    #[cfg(windows)]
    let line = "Write-Output (6*7); if ($env:PATHEXT) { Write-Output 'pathext-set' }; exit 3";
    // Outside the workspace: the account's own permissions are the boundary.
    let elsewhere = tempfile::tempdir().unwrap();
    let id = broker
        .start(command_line(
            "line",
            line,
            Some(elsewhere.path().display().to_string()),
        ))
        .unwrap()
        .operation
        .operation_id;
    let view = settle_within(&broker, &id, 120).await;
    assert_eq!(view.exit_code, Some(3), "{view:?}");
    let output = output_text(&broker, &id);
    assert!(output.contains("42"), "{output}");
    // The account's environment reaches the command (a cleared one would not).
    #[cfg(unix)]
    assert!(output.contains("home-set"), "{output}");
    #[cfg(windows)]
    assert!(output.contains("pathext-set"), "{output}");
    // A program nobody granted runs too.
    let program = start(&broker, "program", &test_executable(), vec![], 20_000);
    let view = settle(&broker, &program).await;
    assert!(matches!(view.state, OperationState::Succeeded), "{view:?}");
    broker.shutdown().await;
}

#[tokio::test]
async fn finished_operations_are_retired_so_admission_never_runs_out() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let config = limited_config(&directory, &[&executable], 2);
    let state = devagotchi_ipc::private_test_state(directory.path());
    let broker = Broker::open(config.clone(), &state).unwrap();
    // More operations than the history holds: every one is admitted.
    let mut ids = Vec::new();
    for n in 0..5 {
        let id = start(&broker, &format!("key-{n}"), &executable, vec![], 5_000);
        settle(&broker, &id).await;
        ids.push(id);
    }
    // The two newest remain; the older ones are retired and no longer found.
    assert_eq!(broker.get(&ids[0], None).unwrap_err().code, "not_found");
    assert!(broker.get(&ids[3], None).is_ok() && broker.get(&ids[4], None).is_ok());
    // A retired key is refused, never run again; a held key still replays.
    let retired = broker
        .start(input("key-0", &executable, vec![]))
        .unwrap_err();
    assert_eq!(retired.code, "idempotency_key_retired");
    let replay = broker.start(input("key-4", &executable, vec![])).unwrap();
    assert!(replay.replayed);
    broker.shutdown().await;
    // Both survive a restart.
    let broker = Broker::open(config, &state).unwrap();
    let retired = broker
        .start(input("key-1", &executable, vec![]))
        .unwrap_err();
    assert_eq!(retired.code, "idempotency_key_retired");
    assert_eq!(broker.get(&ids[1], None).unwrap_err().code, "not_found");
    assert!(broker.get(&ids[4], None).is_ok());
    broker.shutdown().await;
}

#[tokio::test]
async fn only_the_account_policy_takes_a_command_line_and_never_with_a_program() {
    let directory = tempfile::tempdir().unwrap();
    let granted = open_broker(&directory, &[&test_executable()]);
    let refused = granted
        .start(command_line("line", "echo hi", None))
        .unwrap_err();
    assert_eq!(refused.code, "command_not_allowed");
    granted.shutdown().await;
    let other = tempfile::tempdir().unwrap();
    let account = account_broker(&other);
    let mut both = command_line("both", "echo hi", None);
    both.executable = test_executable().display().to_string();
    assert_eq!(account.start(both).unwrap_err().code, "invalid_request");
    let blank = account
        .start(command_line("blank", "  ", None))
        .unwrap_err();
    assert_eq!(blank.code, "invalid_request");
    account.shutdown().await;
}

#[tokio::test]
async fn admission_waits_only_while_every_held_operation_runs() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let (long, args) = shell(&["/bin/sleep 30"]);
    #[cfg(windows)]
    let (long, args) = (
        system32("PING.EXE"),
        vec!["-n".to_owned(), "30".to_owned(), "127.0.0.1".to_owned()],
    );
    let quick = test_executable();
    let broker = Broker::open(
        limited_config(&directory, &[&long, &quick], 1),
        &devagotchi_ipc::private_test_state(directory.path()),
    )
    .unwrap();
    let running = start(&broker, "long", &long, args, 60_000);
    let refused = broker.start(input("quick", &quick, vec![])).unwrap_err();
    assert_eq!(refused.code, "operation_limit_reached");
    broker.cancel(&running, None).unwrap();
    settle(&broker, &running).await;
    let admitted = broker.start(input("quick", &quick, vec![])).unwrap();
    settle(&broker, &admitted.operation.operation_id).await;
    broker.shutdown().await;
}

#[tokio::test]
async fn owned_descendants_keep_the_operation_running_until_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    // The root exits at once and leaves one descendant running.
    #[cfg(unix)]
    let (shell, args) = shell(&["/bin/sleep 30 &", "exit 0"]);
    #[cfg(windows)]
    let ping = system32("PING.EXE");
    #[cfg(windows)]
    let (shell, args) = shell(&[
        "start",
        "/b",
        "",
        ping.to_str().unwrap(),
        "-n",
        "30",
        "127.0.0.1",
    ]);
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "descendant", &shell, args, 60_000);
    sleep(Duration::from_millis(1500)).await;
    let view = broker.get(&id, None).unwrap();
    assert!(
        matches!(view.state, OperationState::Running),
        "a descendant outlived its root: {view:?}"
    );
    broker.cancel(&id, None).unwrap();
    let view = settle(&broker, &id).await;
    assert!(matches!(view.state, OperationState::Cancelled), "{view:?}");
    assert!(view.cancellation_requested);
    assert!(view.output_complete, "{view:?}");
    assert!(
        view.detail
            .unwrap()
            .contains(platform::TERMINATION_VERIFIED_DETAIL)
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn the_deadline_ends_the_owned_tree() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let (shell, args) = shell(&["/bin/sleep 30"]);
    #[cfg(windows)]
    let (shell, args) = (
        system32("PING.EXE"),
        vec!["-n".to_owned(), "30".to_owned(), "127.0.0.1".to_owned()],
    );
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "deadline", &shell, args, 500);
    let view = settle(&broker, &id).await;
    assert!(matches!(view.state, OperationState::TimedOut), "{view:?}");
    assert!(
        view.detail
            .unwrap()
            .contains(platform::TERMINATION_VERIFIED_DETAIL)
    );
    broker.shutdown().await;
}

#[cfg(windows)]
#[test]
fn batch_files_are_not_executable_grants() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("grant.cmd");
    std::fs::write(&script, b"@echo off\r\n").unwrap();
    let config = BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: vec![script.display().to_string()],
        output_limit_bytes: 8192,
        max_operations: 1,
        max_concurrent: 1,
        exec_policy: ExecPolicy::Granted,
    };
    let error = Broker::open(
        config,
        &devagotchi_ipc::private_test_state(directory.path()),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "invalid_config");
    assert!(error.message.contains("cmd.exe"));
}

#[test]
fn unknown_config_fields_and_unsafe_limits_fail_closed() {
    assert!(
        serde_json::from_str::<BrokerConfig>(
            r#"{"workspace":"/tmp","allowed_executables":[],"unexpected":true}"#
        )
        .is_err()
    );
    let directory = tempfile::tempdir().unwrap();
    let config = BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: vec![],
        output_limit_bytes: 8192,
        max_operations: 1,
        max_concurrent: 2,
        exec_policy: ExecPolicy::Granted,
    };
    assert!(Broker::open(config, directory.path()).is_err());
}

#[tokio::test]
async fn critical_journal_failure_is_visible_and_blocks_new_execution() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let config = BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: vec![executable.display().to_string()],
        output_limit_bytes: 8192,
        max_operations: 4,
        max_concurrent: 2,
        exec_policy: ExecPolicy::Granted,
    };
    let state = devagotchi_ipc::private_test_state(directory.path());
    let broker = Broker::open(config, &state).unwrap();
    let injection = rusqlite::Connection::open(state.join("operations.sqlite3")).unwrap();
    injection.execute_batch("CREATE TRIGGER injected_failure BEFORE UPDATE ON operations BEGIN SELECT RAISE(FAIL, 'test persistence failure'); END;").unwrap();
    let input = StartInput {
        idempotency_key: "before-fault".into(),
        executable: executable.display().to_string(),
        args: vec![],
        command: None,
        cwd: None,
        deadline_ms: 3000,
        interactive: None,
        combined_output: false,
        caller: None,
    };
    let start = broker.start(input.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let operation = broker.get(&start.operation.operation_id, None).unwrap();
        if matches!(operation.state, OperationState::Unknown) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Persistence failure must become visible"
        );
        sleep(Duration::from_millis(10)).await;
    }
    let mut second = input;
    second.idempotency_key = "after-fault".into();
    assert_eq!(broker.start(second).unwrap_err().code, "persistence_error");
    broker.shutdown().await;
    let operation = broker.get(&start.operation.operation_id, None).unwrap();
    assert!(matches!(operation.state, OperationState::Unknown));
    assert!(!operation.output_complete);
    assert!(operation.detail.unwrap().contains("persistence"));
}

#[tokio::test]
async fn recovery_never_replays_unsettled_admissions() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let config = BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: vec![executable.display().to_string()],
        output_limit_bytes: 8192,
        max_operations: 4,
        max_concurrent: 2,
        exec_policy: ExecPolicy::Granted,
    };
    let input = StartInput {
        idempotency_key: "survives-restart".into(),
        executable: executable.display().to_string(),
        args: vec![],
        command: None,
        cwd: None,
        deadline_ms: 1000,
        interactive: None,
        combined_output: false,
        caller: None,
    };
    let id = uuid::Uuid::new_v4().to_string();
    let state = devagotchi_ipc::private_test_state(directory.path());
    {
        let broker = Broker::open(config.clone(), &state).unwrap();
        let (spec, digest) = broker.normalize(&input).unwrap();
        let record = JournalOperation {
            key: input.idempotency_key.clone(),
            request_digest: digest,
            config_identity: broker.shared.config_identity.clone(),
            owner: None,
            view: OperationView {
                operation_id: id.clone(),
                executable: path_string(&spec.executable).unwrap(),
                args: spec.args,
                cwd: path_string(&spec.cwd).unwrap(),
                deadline_ms: spec.deadline_ms,
                state: OperationState::Running,
                created_at_ms: 1,
                updated_at_ms: 2,
                exit_code: None,
                detail: None,
                ownership_scope: SCOPE.into(),
                cancellation_requested: false,
                output_complete: false,
                interactive: None,
            },
            latest_sequence: 0,
            frames: VecDeque::new(),
        };
        broker.shared.lock().journal.insert(&record).unwrap();
    }
    let broker = Broker::open(config, &state).unwrap();
    let replay = broker.start(input).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.operation.operation_id, id);
    assert!(matches!(replay.operation.state, OperationState::Unknown));
    assert!(
        replay
            .operation
            .detail
            .unwrap()
            .contains("processes may still be running")
    );
    assert_eq!(broker.shared.lock().active_count, 0);
    broker.shutdown().await;
}
