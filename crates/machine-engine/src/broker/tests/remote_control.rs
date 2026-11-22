//! Real native process control and permission retained through the input queue.
use super::remote::{actor, execute, launch, prepare, until_output};
use super::*;

fn proof(auth: &GrantAuthority, action: ProcessControl) -> GrantControl {
    let now = now_ms() as i64;
    GrantControl {
        binding: auth.binding.clone(),
        action,
        issued_at: now,
        expires_at: now + 10_000,
    }
}
async fn control(
    broker: &Broker,
    proof: GrantControl,
    request: RequestBody,
) -> BridgeResult<RemoteReply> {
    broker
        .handle_remote(RemoteWireRequest {
            api_version: API_VERSION,
            remote: RemoteRequest::Control {
                control: proof,
                client: actor().client,
                request,
            },
        })
        .await
}
fn cancel(id: &str) -> RequestBody {
    RequestBody::OperationCancel(GetInput {
        operation_id: id.into(),
        caller: None,
    })
}
fn typed(id: &str, text: &str, keys: Vec<Key>) -> RequestBody {
    RequestBody::OperationInput(SendInput {
        operation_id: id.into(),
        text: Some(text.into()),
        keys,
        take_over: false,
        caller: None,
    })
}

#[tokio::test]
async fn remote_control_resizes_the_actual_native_terminal_under_its_original_grant() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let mut input = input("controlled-terminal", &exe, args);
    input.interactive = Some(InteractiveInput::default());
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    let request = RequestBody::OperationResize(ResizeInput {
        operation_id: id.clone(),
        cols: 100,
        rows: 42,
        take_over: false,
        caller: None,
    });
    let RemoteReply::Controlled(ResponseBody::Operation(view)) =
        control(&broker, proof(&auth, ProcessControl::Resize), request)
            .await
            .unwrap()
    else {
        panic!("expected resize receipt")
    };
    let terminal = view.interactive.unwrap();
    assert_eq!((terminal.cols, terminal.rows), (100, 42));
    broker.shutdown().await;
}
#[tokio::test]
async fn remote_control_real_pipe_input_obeys_original_grant_without_renewing_execution() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let exe = system32("sort.exe");
    #[cfg(unix)]
    let exe = PathBuf::from("/bin/cat");
    let broker = open_broker(&dir, &[&exe]);
    let mut input = input("controlled-pipe", &exe, vec![]);
    input.interactive = Some(InteractiveInput {
        mode: InputMode::Pipe,
        ..Default::default()
    });
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    let before = *broker.shared.lock().operations[id]
        .authority
        .as_ref()
        .unwrap()
        .deadline
        .borrow();
    assert!(matches!(
        control(
            &broker,
            proof(&auth, ProcessControl::Input),
            typed(id, "pear\nabove\n", vec![])
        )
        .await
        .unwrap(),
        RemoteReply::Controlled(ResponseBody::InputSent(_))
    ));
    assert_eq!(
        before,
        *broker.shared.lock().operations[id]
            .authority
            .as_ref()
            .unwrap()
            .deadline
            .borrow()
    );
    control(
        &broker,
        proof(&auth, ProcessControl::Input),
        typed(id, "", vec![Key::CtrlD]),
    )
    .await
    .unwrap();
    assert!(matches!(
        settle(&broker, id).await.state,
        OperationState::Succeeded
    ));
    let text = output_text(&broker, id);
    assert!(text.contains("pear") && text.contains("above"), "{text}");
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_control_action_identity_expiry_and_legacy_bypass_are_checked_before_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let input = input("controlled-cancel", &exe, args);
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    assert_eq!(
        control(&broker, proof(&auth, ProcessControl::Input), cancel(id))
            .await
            .unwrap_err()
            .code,
        "authority_mismatch"
    );
    let mut foreign = proof(&auth, ProcessControl::Cancel);
    foreign.binding.principal = "@agent:owner:other".into();
    assert_eq!(
        control(&broker, foreign, cancel(id))
            .await
            .unwrap_err()
            .code,
        "authority_mismatch"
    );
    let mut expired = proof(&auth, ProcessControl::Cancel);
    expired.issued_at -= 20_000;
    expired.expires_at -= 20_000;
    assert_eq!(
        control(&broker, expired, cancel(id))
            .await
            .unwrap_err()
            .code,
        "authority_expired"
    );
    assert_eq!(
        execute(&broker, &auth, cancel(id)).await.unwrap_err().code,
        "remote_operation_unavailable"
    );
    assert!(
        !broker.shared.lock().operations[id]
            .record
            .view
            .cancellation_requested
    );
    control(&broker, proof(&auth, ProcessControl::Cancel), cancel(id))
        .await
        .unwrap();
    assert!(matches!(
        settle(&broker, id).await.state,
        OperationState::Cancelled
    ));
    assert!(!output_text(&broker, id).contains("second"));
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_control_queued_input_retains_its_deadline_and_current_revocation_check() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, args) = pause_between_lines();
    let broker = open_broker(&dir, &[&exe]);
    let mut input = input("controlled-queue", &exe, args);
    input.interactive = Some(InteractiveInput {
        mode: InputMode::Pipe,
        ..Default::default()
    });
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    let (sender, mut queued) = mpsc::channel(1);
    broker.shared.lock().operations.get_mut(id).unwrap().input = Some(sender);
    let mut short = proof(&auth, ProcessControl::Input);
    short.expires_at = short.issued_at + 300;
    let delivery = control(&broker, short, typed(id, "queued", vec![]));
    let consume = async {
        let request = queued.recv().await.unwrap();
        let permit = request
            .permit
            .as_ref()
            .expect("remote input must carry its permission");
        permit.check().unwrap();
        sleep(Duration::from_millis(350)).await;
        let error = permit.check().unwrap_err();
        assert_eq!(error.code, "authority_expired");
        request.reply.send(Err(error)).unwrap();
    };
    let (result, ()) = tokio::join!(delivery, consume);
    assert_eq!(result.unwrap_err().code, "authority_expired");
    let delivery = control(
        &broker,
        proof(&auth, ProcessControl::Input),
        typed(id, "revoked", vec![]),
    );
    let consume = async {
        let request = queued.recv().await.unwrap();
        let permit = request.permit.as_ref().unwrap();
        permit.check().unwrap();
        broker
            .interrupt_authority(GrantInterruption {
                origin: auth.binding.origin.clone(),
                grant_id: auth.binding.grant_id.clone(),
                minimum_version: 2,
            })
            .unwrap();
        let error = permit.check().unwrap_err();
        assert_eq!(error.code, "authority_revoked");
        request.reply.send(Err(error)).unwrap();
    };
    let (result, ()) = tokio::join!(delivery, consume);
    assert_eq!(result.unwrap_err().code, "authority_revoked");
    broker.shutdown().await;
}

#[tokio::test]
async fn remote_control_expired_input_is_not_delivered_after_actual_pipe_backpressure() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("read-input-now");
    #[cfg(windows)]
    let (exe, args) = {
        let script = dir.path().join("blocked-input.ps1");
        let gate = gate.to_string_lossy().replace('\'', "''");
        std::fs::write(&script,format!("[Console]::WriteLine('first'); while (-not (Test-Path -LiteralPath '{gate}')) {{ Start-Sleep -Milliseconds 20 }}; [Console]::Write([Console]::In.ReadToEnd())")).unwrap();
        (
            system32("WindowsPowerShell/v1.0/powershell.exe"),
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                script.to_string_lossy().into_owned(),
            ],
        )
    };
    #[cfg(unix)]
    let (exe, args) = {
        let gate = gate.to_string_lossy().replace('\'', "'\"'\"'");
        (
            PathBuf::from("/bin/sh"),
            vec![
                "-c".into(),
                format!(
                    "printf 'first\\n'; while [ ! -f '{gate}' ]; do /bin/sleep 0.02; done; /bin/cat"
                ),
            ],
        )
    };
    let broker = open_broker(&dir, &[&exe]);
    let mut input = input("controlled-backpressure", &exe, args);
    input.deadline_ms = 30_000;
    input.interactive = Some(InteractiveInput {
        mode: InputMode::Pipe,
        ..Default::default()
    });
    let auth = prepare(&broker, &input).await;
    let started = launch(&broker, &auth, &input).await;
    let id = &started.operation.operation_id;
    until_output(&broker, id).await;
    let (capture, mut requests) = mpsc::channel(2);
    let native = {
        let mut state = broker.shared.lock();
        let operation = state.operations.get_mut(id).unwrap();
        operation.input.replace(capture).unwrap()
    };
    let mut short = proof(&auth, ProcessControl::Input);
    short.expires_at = short.issued_at + 500;
    let fill = control(&broker, short.clone(), typed(id, "fill", vec![]));
    let denied = control(
        &broker,
        short,
        typed(id, "expired-marker-must-never-arrive\n", vec![]),
    );
    let forward = async {
        let mut first = requests.recv().await.unwrap();
        assert_eq!(first.steps, vec![InputStep::Bytes(b"fill".to_vec())]);
        // The fixture saturates native buffers directly; the public input limit stays 16KiB.
        // Both replies still come from the actual execution worker, not this test.
        first.steps = vec![InputStep::Bytes(vec![b'x'; 2 * 1024 * 1024])];
        assert!(native.try_send(first).is_ok());
        let second = requests.recv().await.unwrap();
        assert_eq!(
            second.steps,
            vec![InputStep::Bytes(
                b"expired-marker-must-never-arrive\n".to_vec()
            )]
        );
        assert!(native.try_send(second).is_ok());
        sleep(Duration::from_millis(650)).await;
        std::fs::write(&gate, b"read").unwrap();
    };
    let (fill, denied, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(fill, denied, forward)
    })
    .await
    .unwrap();
    assert_eq!(fill.unwrap_err().code, "authority_expired");
    assert_eq!(denied.unwrap_err().code, "authority_expired");
    broker.shared.lock().operations.get_mut(id).unwrap().input = Some(native);
    control(
        &broker,
        proof(&auth, ProcessControl::Input),
        typed(id, "", vec![Key::CtrlD]),
    )
    .await
    .unwrap();
    assert!(matches!(
        settle_within(&broker, id, 30).await.state,
        OperationState::Succeeded
    ));
    let mut output = String::new();
    let mut cursor = 0;
    loop {
        let page = broker
            .read(ReadInput {
                operation_id: id.clone(),
                cursor,
                max_bytes: MAX_OUTPUT_PAGE_BYTES,
                caller: None,
            })
            .unwrap();
        for frame in &page.frames {
            output.push_str(&frame.text);
        }
        if !page.has_more {
            break;
        }
        assert!(page.next_cursor > cursor, "output cursor must advance");
        cursor = page.next_cursor;
    }
    assert!(output.contains('x'), "the native pipe was not exercised");
    assert!(
        !output.contains("expired-marker-must-never-arrive"),
        "expired queued input reached the child"
    );
    broker.shutdown().await;
}
