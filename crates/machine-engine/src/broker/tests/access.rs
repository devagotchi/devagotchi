//! Who reaches an operation: its own account (any of its conversations), this machine's own
//! account and the inspector (no caller); never another account, which does not even learn
//! that it exists.

use super::*;

fn who(account: &str, client: &str) -> Option<Caller> {
    Some(Caller {
        account: account.into(),
        client: client.into(),
    })
}

#[tokio::test]
async fn only_its_account_reaches_an_operation_and_another_learns_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let broker = open_broker(&directory, &[&executable]);
    let owner = who("hub:alice", "chat-1");
    let start = StartInput {
        idempotency_key: "alice-job".into(),
        executable: executable.display().to_string(),
        args: vec![],
        command: None,
        cwd: None,
        deadline_ms: 30_000,
        interactive: None,
        caller: owner.clone(),
    };
    let id = broker.start(start.clone()).unwrap().operation.operation_id;
    settle(&broker, &id).await;

    let stranger = who("hub:mallory", "chat-9");
    let not_found = |result: BridgeResult<_>| match result {
        Err(BridgeError { code, .. }) => code == "not_found",
        Ok(_) => false,
    };
    assert!(not_found(broker.get(&id, stranger.as_ref()).map(|_| ())));
    assert!(not_found(broker.cancel(&id, stranger.as_ref()).map(|_| ())));
    let read = ReadInput {
        operation_id: id.clone(),
        cursor: 0,
        max_bytes: MAX_OUTPUT_PAGE_BYTES,
        caller: stranger.clone(),
    };
    assert!(not_found(broker.read(read).map(|_| ())));
    let wait = WaitInput {
        operation_id: id.clone(),
        output_after: None,
        timeout_ms: 1_000,
        caller: stranger.clone(),
    };
    assert!(not_found(broker.wait(wait).await.map(|_| ())));
    // Nor does its retry key hand it the operation.
    let replay = StartInput {
        caller: stranger,
        ..start.clone()
    };
    assert_eq!(
        broker.start(replay).unwrap_err().code,
        "idempotency_conflict"
    );

    // Its own account from any conversation, this machine's own account, the inspector.
    for caller in [
        owner.clone(),
        who("hub:alice", "chat-2"),
        who(LOCAL_ACCOUNT, "adapter-1"),
        None,
    ] {
        let view = broker.get(&id, caller.as_ref()).unwrap();
        assert_eq!(view.operation_id, id, "{caller:?}");
    }
    assert!(broker.start(start).unwrap().replayed);
}

#[tokio::test]
async fn another_account_does_not_reach_what_this_machine_ran_for_itself() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let broker = open_broker(&directory, &[&executable]);
    let id = broker
        .start(StartInput {
            idempotency_key: "local-job".into(),
            executable: executable.display().to_string(),
            args: vec![],
            command: None,
            cwd: None,
            deadline_ms: 30_000,
            interactive: None,
            caller: who(LOCAL_ACCOUNT, "adapter-1"),
        })
        .unwrap()
        .operation
        .operation_id;
    settle(&broker, &id).await;
    let hub = who("hub:alice", "chat-1");
    assert_eq!(broker.get(&id, hub.as_ref()).unwrap_err().code, "not_found");
    assert!(broker.get(&id, None).is_ok());
}
