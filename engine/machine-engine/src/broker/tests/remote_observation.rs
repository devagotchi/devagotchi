//! Observation is current read authority, never permission to renew or change execution.
use super::remote::{actor, execute, launch, prepare, until_output};
use super::*;

fn proof(auth: &GrantAuthority) -> GrantObservation {
    let now = now_ms() as i64;
    GrantObservation {
        binding: auth.binding.clone(),
        issued_at: now,
        expires_at: now + 10_000,
    }
}
async fn observe(
    broker: &Broker,
    observation: GrantObservation,
    request: RequestBody,
) -> BridgeResult<RemoteReply> {
    broker
        .handle_remote(RemoteWireRequest {
            api_version: API_VERSION,
            remote: RemoteRequest::Observe {
                observation,
                client: actor().client,
                request,
            },
        })
        .await
}
fn get(id: &str) -> GetInput {
    GetInput {
        operation_id: id.into(),
        caller: None,
    }
}
fn wait(id: &str, timeout_ms: u64, output_after: Option<u64>) -> RequestBody {
    RequestBody::OperationWait(WaitInput {
        operation_id: id.into(),
        timeout_ms,
        output_after,
        caller: None,
    })
}

#[tokio::test]
async fn remote_observation_and_execute_reads_do_not_extend_native_execution() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("observe-expiry", &exe, args);
    let mut auth = prepare(&broker, &input).await;
    auth.expires_at = auth.issued_at + 1800;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    let before = *broker.shared.lock().operations[id]
        .authority
        .as_ref()
        .unwrap()
        .deadline
        .borrow();
    sleep(Duration::from_millis(10)).await;
    let observation = proof(&auth);
    assert!(matches!(
        observe(
            &broker,
            observation.clone(),
            RequestBody::OperationGet(get(id))
        )
        .await
        .unwrap(),
        RemoteReply::Observed { ready: true, .. }
    ));
    // Even an Execute read with a later credential must not act as Refresh.
    let mut later = auth.clone();
    later.issued_at = observation.issued_at;
    later.expires_at = observation.expires_at;
    execute(&broker, &later, RequestBody::OperationGet(get(id)))
        .await
        .unwrap();
    assert_eq!(
        before,
        *broker.shared.lock().operations[id]
            .authority
            .as_ref()
            .unwrap()
            .deadline
            .borrow()
    );
    assert!(matches!(
        settle(&broker, id).await.state,
        OperationState::Cancelled
    ));
    assert!(!output_text(&broker, id).contains("second"));
    // Completed output is still readable after execution expired and its version was revoked.
    broker
        .interrupt_authority(GrantInterruption {
            origin: auth.binding.origin.clone(),
            grant_id: auth.binding.grant_id.clone(),
            minimum_version: 2,
        })
        .unwrap();
    let RemoteReply::Observed {
        response: ResponseBody::Output(page),
        ready: true,
    } = observe(
        &broker,
        proof(&auth),
        RequestBody::OperationRead(ReadInput {
            operation_id: id.clone(),
            cursor: 0,
            max_bytes: 4096,
            caller: None,
        }),
    )
    .await
    .unwrap()
    else {
        panic!("expected historical output")
    };
    assert!(page.frames.iter().any(|f| f.text.contains("first")));
    assert!(page.operation.state.terminal());
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_observation_is_exactly_bound_and_cannot_start_or_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("observe-no-effects", &exe, args);
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    for request in [
        RequestBody::OperationCancel(get(id)),
        RequestBody::OperationStart(input),
    ] {
        assert_eq!(
            observe(&broker, proof(&auth), request)
                .await
                .unwrap_err()
                .code,
            "not_granted"
        );
    }
    for field in ["principal", "admission", "generation", "grant"] {
        let mut changed = proof(&auth);
        match field {
            "principal" => changed.binding.principal = "@agent:owner:other".into(),
            "admission" => changed.binding.admission_id.push('x'),
            "generation" => changed.binding.origin.generation.push('x'),
            _ => changed.binding.grant_id.push('x'),
        }
        assert_eq!(
            observe(&broker, changed, RequestBody::OperationGet(get(id)))
                .await
                .unwrap_err()
                .code,
            "authority_mismatch"
        );
    }
    let mut expired = proof(&auth);
    expired.expires_at = expired.issued_at;
    assert!(
        observe(&broker, expired, RequestBody::OperationGet(get(id)))
            .await
            .is_err()
    );
    assert_eq!(broker.shared.lock().operations.len(), 1);
    assert!(
        !broker.shared.lock().operations[id]
            .record
            .view
            .cancellation_requested
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_observation_wait_reports_partial_wait_and_validates_original_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("observe-wait", &exe, args);
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    for timeout in [0, MAX_WAIT_MS + 1] {
        assert!(
            observe(&broker, proof(&auth), wait(id, timeout, None))
                .await
                .is_err()
        );
    }
    let mut short = proof(&auth);
    short.expires_at = short.issued_at + 500;
    assert!(matches!(
        observe(&broker, short, wait(id, MAX_WAIT_MS, None))
            .await
            .unwrap(),
        RemoteReply::Observed { ready: false, .. }
    ));
    assert!(matches!(
        observe(&broker, proof(&auth), wait(id, MAX_WAIT_MS, Some(0)))
            .await
            .unwrap(),
        RemoteReply::Observed { ready: true, .. }
    ));
    broker.shutdown().await;
}
