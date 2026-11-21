//! Real native execution, permanent admission identity, and revocation enforcement.
use super::*;

fn origin() -> GrantOrigin {
    GrantOrigin {
        hub: "a".repeat(64),
        machine: "workstation".into(),
        generation: "enrollment-one".into(),
    }
}
fn actor() -> GrantActor {
    GrantActor {
        account: "owner".into(),
        principal: "@agent:owner:builder".into(),
        client: "first-client".into(),
    }
}
async fn prepare(broker: &Broker, input: &StartInput) -> GrantAuthority {
    let RemoteReply::Prepared {
        operation_key,
        input_sha256,
        call,
        ..
    } = broker
        .handle_remote(RemoteWireRequest {
            api_version: API_VERSION,
            remote: RemoteRequest::Prepare {
                origin: origin(),
                actor: actor(),
                level: GrantLevel::Unsandboxed,
                request: RequestBody::OperationStart(input.clone()),
            },
        })
        .await
        .unwrap()
    else {
        panic!("expected prepared operation")
    };
    let now = now_ms() as i64;
    GrantAuthority {
        binding: GrantBinding {
            origin: origin(),
            account: actor().account,
            principal: actor().principal,
            admission_id: format!("admission-{operation_key}"),
            grant_id: "approved-grant".into(),
            grant_version: 1,
            operation_key,
            input_sha256,
            effective: GrantItem {
                capability: call.capability,
                sandbox_level: call.level,
                selector: GrantSelector {
                    paths: Some(call.paths),
                    argv_prefix: Some(vec![call.argv]),
                    max_runtime_s: call.runtime_s,
                    ..Default::default()
                },
            },
            lease_sha256: None,
        },
        issued_at: now,
        expires_at: now + 30_000,
    }
}
async fn execute(
    broker: &Broker,
    authority: &GrantAuthority,
    request: RequestBody,
) -> BridgeResult<RemoteReply> {
    broker
        .handle_remote(RemoteWireRequest {
            api_version: API_VERSION,
            remote: RemoteRequest::Execute {
                authority: authority.clone(),
                client: actor().client,
                request,
            },
        })
        .await
}
async fn launch(broker: &Broker, authority: &GrantAuthority, input: &StartInput) -> StartOutput {
    match execute(
        broker,
        authority,
        RequestBody::OperationStart(input.clone()),
    )
    .await
    .unwrap()
    {
        RemoteReply::Executed(ResponseBody::Start(out)) => out,
        _ => panic!("expected start"),
    }
}
async fn until_output(broker: &Broker, id: &str) {
    let until = Instant::now() + Duration::from_secs(5);
    while !output_text(broker, id).contains("first") {
        assert!(
            Instant::now() < until,
            "native process did not produce initial output"
        );
        sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn remote_scope_and_canonical_input_are_checked_before_admission() {
    let dir = tempfile::tempdir().unwrap();
    let exe = test_executable();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-scope", &exe, vec![]);
    let auth = prepare(&broker, &input).await;
    let mut changed = input.clone();
    changed.args.push("unapproved".into());
    assert_eq!(
        execute(&broker, &auth, RequestBody::OperationStart(changed))
            .await
            .unwrap_err()
            .code,
        "authority_mismatch"
    );
    let mut denied = auth.clone();
    denied.binding.effective.selector.argv_prefix = Some(vec![]);
    assert_eq!(
        execute(&broker, &denied, RequestBody::OperationStart(input.clone()))
            .await
            .unwrap_err()
            .code,
        "outside_grant"
    );
    denied = auth.clone();
    denied.binding.effective.sandbox_level = GrantLevel::ReadOnly;
    assert_eq!(
        execute(&broker, &denied, RequestBody::OperationStart(input))
            .await
            .unwrap_err()
            .code,
        "sandbox_unavailable"
    );
    assert!(broker.shared.lock().operations.is_empty());
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_retry_cannot_change_principal_enrollment_or_effective_scope() {
    let dir = tempfile::tempdir().unwrap();
    let exe = test_executable();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-retry", &exe, vec![]);
    let auth = prepare(&broker, &input).await;
    let first = launch(&broker, &auth, &input).await;
    settle(&broker, &first.operation.operation_id).await;
    assert!(launch(&broker, &auth, &input).await.replayed);
    for field in 0..4 {
        let mut changed = auth.clone();
        match field {
            0 => changed.binding.principal = "@agent:owner:other".into(),
            1 => changed.binding.origin.generation = "reenrolled".into(),
            2 => changed.binding.effective.selector.max_runtime_s = Some(99),
            _ => changed.binding.admission_id = "replacement".into(),
        }
        assert_eq!(
            execute(
                &broker,
                &changed,
                RequestBody::OperationStart(input.clone())
            )
            .await
            .unwrap_err()
            .code,
            "idempotency_conflict"
        );
    }
    assert_eq!(
        broker.start(input).unwrap_err().code,
        "idempotency_conflict"
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_expiry_stops_native_process_and_cannot_be_refreshed_or_retried() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-expiry", &exe, args);
    let mut auth = prepare(&broker, &input).await;
    auth.expires_at = auth.issued_at + 1800;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    let ended = settle(&broker, id).await;
    assert!(matches!(ended.state, OperationState::Cancelled));
    assert!(!output_text(&broker, id).contains("second"));
    assert_eq!(
        broker.refresh_authority(auth.clone()).unwrap_err().code,
        "authority_expired"
    );
    assert_eq!(
        execute(&broker, &auth, RequestBody::OperationStart(input))
            .await
            .unwrap_err()
            .code,
        "authority_expired"
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_interruption_stops_native_process_and_floor_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-revoke", &exe, args);
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    let notice = GrantInterruption {
        origin: origin(),
        grant_id: auth.binding.grant_id.clone(),
        minimum_version: 2,
    };
    assert!(matches!(
        broker.interrupt_authority(notice.clone()).unwrap(),
        RemoteReply::Interrupted { count: 1 }
    ));
    assert!(matches!(
        settle(&broker, id).await.state,
        OperationState::Cancelled
    ));
    assert!(!output_text(&broker, id).contains("second"));
    let config = broker.shared.config.clone();
    broker.shutdown().await;
    drop(broker);
    #[cfg(windows)]
    let state = dir.path().join("state");
    #[cfg(unix)]
    let state = dir.path().to_path_buf();
    let broker = Broker::open(config, &state).unwrap();
    let mut older = notice;
    older.minimum_version = 1;
    broker.interrupt_authority(older).unwrap();
    assert_eq!(
        execute(&broker, &auth, RequestBody::OperationStart(input))
            .await
            .unwrap_err()
            .code,
        "authority_revoked"
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_delayed_interruption_does_not_stop_new_grant_version() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-resumed-version", &exe, args);
    let mut auth = prepare(&broker, &input).await;
    auth.binding.grant_version = 2;
    let started = launch(&broker, &auth, &input).await;
    until_output(&broker, &started.operation.operation_id).await;
    assert!(matches!(
        broker
            .interrupt_authority(GrantInterruption {
                origin: origin(),
                grant_id: auth.binding.grant_id.clone(),
                minimum_version: 2
            })
            .unwrap(),
        RemoteReply::Interrupted { count: 0 }
    ));
    assert!(matches!(
        settle(&broker, &started.operation.operation_id).await.state,
        OperationState::Succeeded
    ));
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_read_and_cancel_cannot_borrow_another_admission() {
    let dir = tempfile::tempdir().unwrap();
    let exe = test_executable();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-owner", &exe, vec![]);
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    settle(&broker, &started.operation.operation_id).await;
    let mut stranger = auth.clone();
    stranger.binding.principal = "@agent:owner:other".into();
    stranger.binding.operation_key = "other-key".into();
    stranger.binding.admission_id = "other-admission".into();
    for request in [
        RequestBody::OperationGet(GetInput {
            operation_id: started.operation.operation_id.clone(),
            caller: None,
        }),
        RequestBody::OperationCancel(GetInput {
            operation_id: started.operation.operation_id.clone(),
            caller: None,
        }),
    ] {
        assert_eq!(
            execute(&broker, &stranger, request).await.unwrap_err().code,
            "authority_mismatch"
        );
    }
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_refresh_extends_live_clock_but_same_issue_cannot_extend_or_change_it() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("remote-refresh", &exe, args);
    let mut auth = prepare(&broker, &input).await;
    auth.expires_at = auth.issued_at + 1800;
    let started = launch(&broker, &auth, &input).await;
    until_output(&broker, &started.operation.operation_id).await;
    let before = *broker.shared.lock().operations[&started.operation.operation_id]
        .authority
        .as_ref()
        .unwrap()
        .deadline
        .borrow();
    broker.refresh_authority(auth.clone()).unwrap();
    assert_eq!(
        before,
        *broker.shared.lock().operations[&started.operation.operation_id]
            .authority
            .as_ref()
            .unwrap()
            .deadline
            .borrow()
    );
    let mut invalid = auth.clone();
    invalid.expires_at += 1;
    assert_eq!(
        broker.refresh_authority(invalid).unwrap_err().code,
        "authority_stale"
    );
    sleep(Duration::from_millis(10)).await;
    auth.issued_at = now_ms() as i64;
    auth.expires_at = auth.issued_at + 10_000;
    broker.refresh_authority(auth).unwrap();
    assert!(matches!(
        settle(&broker, &started.operation.operation_id).await.state,
        OperationState::Succeeded
    ));
    broker.shutdown().await;
}
