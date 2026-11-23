//! Owns: disposable native broker fixtures, no production state.
//! Serves: remote file effect and lifecycle regression tests.
//! Settings: test-only authority and capacity bounds.
//! Depends on: real broker, private journal and OS filesystem.
//! Native file effects, whole-file guards and durable retries through remote envelopes.
use super::*;
use sha2::{Digest, Sha256};

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn invocation(key: &str, operation: FileOperation) -> FileInvocation {
    FileInvocation {
        key: key.into(),
        intent: FileIntent::Operation { operation },
    }
}
fn write(key: &str, path: &str, text: &str, expected: Option<String>) -> FileInvocation {
    invocation(
        key,
        FileOperation::Write(FileWriteInput {
            path: path.into(),
            text: Some(text.into()),
            bytes_base64: None,
            mode: if expected.is_some() {
                WriteMode::Overwrite
            } else {
                WriteMode::Create
            },
            expected_sha256: expected,
            create_dirs: true,
        }),
    )
}
fn read(path: &str, offset: i64, max_bytes: usize) -> FileReadInput {
    FileReadInput {
        path: path.into(),
        offset,
        max_bytes,
        start_line: None,
        line_count: None,
    }
}
async fn wire(broker: &Broker, remote: RemoteRequest) -> BridgeResult<RemoteReply> {
    broker
        .handle_remote(RemoteWireRequest {
            api_version: API_VERSION,
            remote,
        })
        .await
}
async fn prepare(broker: &Broker, invocation: &FileInvocation) -> GrantAuthority {
    let origin = GrantOrigin {
        hub: "b".repeat(64),
        machine: "files".into(),
        generation: "one".into(),
    };
    let RemoteReply::Prepared {
        operation_key,
        input_sha256,
        call,
        ..
    } = wire(
        broker,
        RemoteRequest::PrepareFiles {
            origin: origin.clone(),
            actor: remote::actor(),
            level: GrantLevel::Unsandboxed,
            invocation: invocation.clone(),
        },
    )
    .await
    .unwrap()
    else {
        panic!("expected prepared files")
    };
    let now = now_ms() as i64;
    GrantAuthority {
        binding: GrantBinding {
            origin,
            account: remote::actor().account,
            principal: remote::actor().principal,
            admission_id: format!("file-{operation_key}"),
            grant_id: "file-grant".into(),
            grant_version: 1,
            operation_key,
            input_sha256,
            lease_sha256: None,
            effective: GrantItem {
                capability: call.capability,
                sandbox_level: call.level,
                selector: GrantSelector {
                    paths: Some(call.paths),
                    ..Default::default()
                },
            },
        },
        issued_at: now,
        expires_at: now + 30_000,
    }
}
async fn execute(
    broker: &Broker,
    authority: &GrantAuthority,
    invocation: &FileInvocation,
    step: FileStep,
) -> BridgeResult<(FileOutcome, bool)> {
    let RemoteReply::FileExecuted {
        outcome, replayed, ..
    } = wire(
        broker,
        RemoteRequest::ExecuteFiles {
            authority: authority.clone(),
            client: remote::actor().client,
            invocation: invocation.clone(),
            step,
        },
    )
    .await?
    else {
        panic!("expected executed files")
    };
    Ok((outcome, replayed))
}
async fn ordinary(
    broker: &Broker,
    authority: &GrantAuthority,
    invocation: &FileInvocation,
) -> BridgeResult<(FileOutcome, bool)> {
    execute(broker, authority, invocation, FileStep::Operation {}).await
}

#[tokio::test]
async fn prepare_and_failed_guard_create_no_directories() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let input = write(
        "guarded",
        "new/nested/file.txt",
        "new",
        Some(digest(b"missing")),
    );
    let auth = prepare(&broker, &input).await;
    assert!(!directory.path().join("new").exists());
    let (outcome, replayed) = ordinary(&broker, &auth, &input).await.unwrap();
    assert!(!replayed);
    assert_eq!(
        outcome.into_result().unwrap_err().code,
        "changed_since_read"
    );
    assert!(!directory.path().join("new").exists());
    assert!(ordinary(&broker, &auth, &input).await.unwrap().1);
    broker.shutdown().await;
}

#[tokio::test]
async fn mutation_receipt_survives_reopen_without_repeating_the_effect() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let input = write("create-once", "new/file.txt", "first", None);
    let auth = prepare(&broker, &input).await;
    let (outcome, replayed) = ordinary(&broker, &auth, &input).await.unwrap();
    assert!(!replayed);
    assert!(matches!(
        outcome.into_result().unwrap(),
        ResponseBody::Written(_)
    ));
    std::fs::write(directory.path().join("new/file.txt"), "external editor").unwrap();
    let config = broker.shared.config.clone();
    #[cfg(windows)]
    let state = directory.path().join("state");
    #[cfg(unix)]
    let state = directory.path().to_path_buf();
    broker.shutdown().await;
    drop(broker);
    let broker = Broker::open(config, &state).unwrap();
    let resumed = prepare(&broker, &input).await;
    assert_eq!(resumed.binding, auth.binding);
    let (outcome, replayed) = ordinary(&broker, &resumed, &input).await.unwrap();
    assert!(replayed);
    let ResponseBody::Written(written) = outcome.into_result().unwrap() else {
        panic!()
    };
    assert_eq!(written.sha256, digest(b"first"));
    assert_eq!(
        std::fs::read(directory.path().join("new/file.txt")).unwrap(),
        b"external editor"
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn complete_read_reports_whole_sha_but_partial_read_does_not() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("text"), b"abcdef").unwrap();
    let broker = open_broker(&directory, &[]);
    for (key, length, expected) in [("part", 2, None), ("whole", 6, Some(digest(b"abcdef")))] {
        let input = invocation(key, FileOperation::Read(read("text", 0, length)));
        let auth = prepare(&broker, &input).await;
        let ResponseBody::File(file) = ordinary(&broker, &auth, &input)
            .await
            .unwrap()
            .0
            .into_result()
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(file.sha256, expected);
    }
    let input = invocation(
        "info",
        FileOperation::Info(FileInfoInput {
            path: "text".into(),
            sha256: true,
        }),
    );
    let auth = prepare(&broker, &input).await;
    let ResponseBody::Info(info) = ordinary(&broker, &auth, &input)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(info.sha256, Some(digest(b"abcdef")));
    broker.shutdown().await;
}

#[tokio::test]
async fn guarded_move_and_delete_replay_after_source_is_gone() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("source"), b"content").unwrap();
    let broker = open_broker(&directory, &[]);
    for input in [
        invocation(
            "move",
            FileOperation::Move(FileMoveInput {
                source: "source".into(),
                destination: "target".into(),
                overwrite: false,
                expected_sha256: Some(digest(b"content")),
            }),
        ),
        invocation(
            "delete",
            FileOperation::Delete(FileDeleteInput {
                path: "target".into(),
                expected_sha256: digest(b"content"),
            }),
        ),
    ] {
        let auth = prepare(&broker, &input).await;
        assert!(!ordinary(&broker, &auth, &input).await.unwrap().1);
        let retry = prepare(&broker, &input).await;
        assert_eq!(retry.binding, auth.binding);
        assert!(ordinary(&broker, &retry, &input).await.unwrap().1);
    }
    assert!(!directory.path().join("source").exists());
    assert!(!directory.path().join("target").exists());
    broker.shutdown().await;
}

#[tokio::test]
async fn batch_uses_one_identity_and_finishes_without_journaling_contents() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("a"), b"private-a").unwrap();
    std::fs::write(directory.path().join("b"), b"private-b").unwrap();
    let broker = open_broker(&directory, &[]);
    let input = FileInvocation {
        key: "batch".into(),
        intent: FileIntent::Batch {
            paths: vec!["a".into(), "b".into()],
            max_bytes: 100,
        },
    };
    let auth = prepare(&broker, &input).await;
    for path in ["a", "b"] {
        execute(
            &broker,
            &auth,
            &input,
            FileStep::Read {
                input: read(path, 0, 100),
            },
        )
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    }
    assert!(
        broker
            .shared
            .lock()
            .journal
            .file_record("batch")
            .unwrap()
            .unwrap()
            .outcome
            .is_none()
    );
    assert_eq!(
        execute(
            &broker,
            &auth,
            &input,
            FileStep::Read {
                input: read("a", 0, 100)
            }
        )
        .await
        .unwrap_err()
        .code,
        "authority_expired"
    );
    wire(
        &broker,
        RemoteRequest::FinishFiles {
            binding: auth.binding.clone(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        wire(
            &broker,
            RemoteRequest::Status {
                binding: auth.binding
            }
        )
        .await
        .unwrap(),
        RemoteReply::FileStatus {
            state: FileExecutionState::Finished { .. },
            ..
        }
    ));
    broker.shutdown().await;
}

#[tokio::test]
async fn pending_mutation_is_uncertain_and_never_repeated() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let input = write("crashed", "new.txt", "never", None);
    let auth = prepare(&broker, &input).await;
    // Simulate a crash after durable admission, before a provable outcome.
    let actor = remote::actor();
    let actor_hash = digest(
        &serde_json::to_vec(&(&auth.binding.origin, &actor.account, &actor.principal)).unwrap(),
    );
    let intent_hash = digest(&serde_json::to_vec(&input).unwrap());
    let mut call = GrantCall::new(GrantCapability::FileWrite, GrantLevel::Unsandboxed);
    call.paths = auth.binding.effective.selector.paths.clone().unwrap();
    broker
        .shared
        .lock()
        .journal
        .insert_file(
            &auth,
            crate::journal::files::NewFile {
                id: "pending",
                actor_hash: &actor_hash,
                intent_hash: &intent_hash,
                config_identity: &broker.shared.config_identity,
                call: &call,
                mutation: true,
            },
        )
        .unwrap();
    let config = broker.shared.config.clone();
    #[cfg(windows)]
    let state = directory.path().join("state");
    #[cfg(unix)]
    let state = directory.path().to_path_buf();
    broker.shutdown().await;
    drop(broker);
    let broker = Broker::open(config, &state).unwrap();
    assert_eq!(
        ordinary(&broker, &auth, &input).await.unwrap_err().code,
        "file_outcome_uncertain"
    );
    assert!(!directory.path().join("new.txt").exists());
    assert!(matches!(
        wire(
            &broker,
            RemoteRequest::Status {
                binding: auth.binding
            }
        )
        .await
        .unwrap(),
        RemoteReply::FileStatus {
            state: FileExecutionState::Uncertain {},
            ..
        }
    ));
    broker.shutdown().await;
}

#[tokio::test]
async fn revocation_while_mutation_waits_prevents_the_effect() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let input = write("queued", "queued.txt", "never", None);
    let auth = prepare(&broker, &input).await;
    let gate = broker.mutation_gate(Duration::from_secs(1)).await.unwrap();
    let pending = {
        let broker = Arc::clone(&broker);
        let auth = auth.clone();
        let input = input.clone();
        tokio::spawn(async move { ordinary(&broker, &auth, &input).await })
    };
    sleep(Duration::from_millis(25)).await;
    wire(
        &broker,
        RemoteRequest::Interrupt {
            notice: GrantInterruption {
                origin: auth.binding.origin.clone(),
                grant_id: auth.binding.grant_id.clone(),
                minimum_version: 2,
            },
        },
    )
    .await
    .unwrap();
    drop(gate);
    assert_eq!(
        pending.await.unwrap().unwrap_err().code,
        "authority_revoked"
    );
    assert!(!directory.path().join("queued.txt").exists());
    broker.shutdown().await;
}

#[tokio::test]
async fn substituted_link_is_refused_before_read_or_write() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("inside"), b"inside").unwrap();
    std::fs::write(outside.path().join("secret"), b"secret").unwrap();
    #[cfg(unix)]
    let symlink = |a: &Path, b: &Path| std::os::unix::fs::symlink(a, b);
    #[cfg(windows)]
    let symlink = |a: &Path, b: &Path| std::os::windows::fs::symlink_file(a, b);
    symlink(Path::new("inside"), &directory.path().join("link")).unwrap();
    let broker = open_broker(&directory, &[]);
    let input = invocation("changed-link", FileOperation::Read(read("link", 0, 100)));
    let auth = prepare(&broker, &input).await;
    std::fs::remove_file(directory.path().join("link")).unwrap();
    symlink(
        &outside.path().join("secret"),
        &directory.path().join("link"),
    )
    .unwrap();
    assert_eq!(
        ordinary(&broker, &auth, &input).await.unwrap_err().code,
        "authority_mismatch"
    );
    broker.shutdown().await;
}

mod lifecycle;
