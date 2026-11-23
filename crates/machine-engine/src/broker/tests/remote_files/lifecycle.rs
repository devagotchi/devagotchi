use super::*;

#[tokio::test]
async fn failed_read_completion_keeps_only_diagnostic_and_is_repeatable() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let input = invocation("missing", FileOperation::Read(read("missing", 0, 100)));
    let auth = prepare(&broker, &input).await;
    let error = ordinary(&broker, &auth, &input)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap_err();
    for _ in 0..2 {
        let reply = wire(
            &broker,
            RemoteRequest::FinishFiles {
                binding: auth.binding.clone(),
            },
        )
        .await
        .unwrap();
        let RemoteReply::FileFinished { error_code, .. } = reply else {
            panic!()
        };
        assert_eq!(error_code.as_deref(), Some(error.code.as_str()));
    }
    let record = broker
        .shared
        .lock()
        .journal
        .file_record("missing")
        .unwrap()
        .unwrap();
    let FileOutcome::Error(saved) = record.outcome.unwrap() else {
        panic!()
    };
    assert_eq!(saved.message, "file read failed");
    broker.shutdown().await;
}

#[tokio::test]
async fn retained_receipts_are_bounded_and_retired_keys_cannot_execute_again() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    broker.shared.lock().limits.max_operations = 1;
    let first = write("first", "first", "first", None);
    let auth = prepare(&broker, &first).await;
    ordinary(&broker, &auth, &first)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    let second = write("second", "second", "second", None);
    let second_auth = prepare(&broker, &second).await;
    ordinary(&broker, &second_auth, &second)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    let retired = broker
        .shared
        .lock()
        .journal
        .file_record("first")
        .unwrap()
        .unwrap();
    assert!(matches!(
        retired.state,
        crate::journal::files::FilePhase::Retired
    ));
    assert!(retired.call.is_none() && retired.outcome.is_none());
    assert_eq!(
        ordinary(&broker, &auth, &first).await.unwrap_err().code,
        "idempotency_key_retired"
    );
    assert_eq!(
        std::fs::read(directory.path().join("first")).unwrap(),
        b"first"
    );
    broker.shutdown().await;
}

#[tokio::test]
async fn active_read_capacity_refuses_before_any_new_mutation() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("read"), b"read").unwrap();
    let broker = open_broker(&directory, &[]);
    broker.shared.lock().limits.max_operations = 1;
    let input = invocation("read", FileOperation::Read(read("read", 0, 100)));
    let auth = prepare(&broker, &input).await;
    ordinary(&broker, &auth, &input)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    let next = write("blocked", "blocked", "never", None);
    let next_auth = prepare(&broker, &next).await;
    assert_eq!(
        ordinary(&broker, &next_auth, &next).await.unwrap_err().code,
        "file_capacity"
    );
    assert!(!directory.path().join("blocked").exists());
    wire(
        &broker,
        RemoteRequest::FinishFiles {
            binding: auth.binding,
        },
    )
    .await
    .unwrap();
    ordinary(&broker, &next_auth, &next)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    broker.shutdown().await;
}

#[tokio::test]
async fn concurrent_guarded_remote_writes_have_one_winner_and_delete_refuses_directory() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shared"), b"before").unwrap();
    let broker = open_broker(&directory, &[]);
    let a = write("a", "shared", "winner-a", Some(digest(b"before")));
    let b = write("b", "shared", "winner-b", Some(digest(b"before")));
    let aa = prepare(&broker, &a).await;
    let ba = prepare(&broker, &b).await;
    let (left, right) = tokio::join!(ordinary(&broker, &aa, &a), ordinary(&broker, &ba, &b));
    let results = [
        left.unwrap().0.into_result(),
        right.unwrap().0.into_result(),
    ];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.into_iter().find_map(Result::err).unwrap().code,
        "changed_since_read"
    );
    std::fs::create_dir(directory.path().join("directory")).unwrap();
    let delete = invocation(
        "delete-dir",
        FileOperation::Delete(FileDeleteInput {
            path: "directory".into(),
            expected_sha256: digest(b""),
        }),
    );
    let auth = prepare(&broker, &delete).await;
    assert_eq!(
        ordinary(&broker, &auth, &delete)
            .await
            .unwrap()
            .0
            .into_result()
            .unwrap_err()
            .code,
        "not_a_file"
    );
    assert!(directory.path().join("directory").is_dir());
    broker.shutdown().await;
}

#[tokio::test]
async fn file_and_process_retry_identities_cannot_collide_in_either_direction() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let broker = open_broker(&directory, &[&executable]);
    let file = write("file-first", "first", "content", None);
    let auth = prepare(&broker, &file).await;
    ordinary(&broker, &auth, &file)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    let process = super::super::input("file-first", &executable, vec![]);
    let process_auth = remote::prepare(&broker, &process).await;
    assert!(
        remote::execute(&broker, &process_auth, RequestBody::OperationStart(process))
            .await
            .is_err()
    );
    assert!(!broker.shared.lock().keys.contains_key("file-first"));
    let process = super::super::input("process-first", &executable, vec![]);
    let process_auth = remote::prepare(&broker, &process).await;
    remote::execute(&broker, &process_auth, RequestBody::OperationStart(process))
        .await
        .unwrap();
    let error = wire(
        &broker,
        RemoteRequest::PrepareFiles {
            origin: auth.binding.origin,
            actor: remote::actor(),
            level: GrantLevel::Unsandboxed,
            invocation: write("process-first", "never", "never", None),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "idempotency_conflict");
    assert!(!directory.path().join("never").exists());
    broker.shutdown().await;
}

#[tokio::test]
async fn changed_retry_material_is_refused_without_new_effects() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let original = write("same-key", "first", "original", None);
    let auth = prepare(&broker, &original).await;
    ordinary(&broker, &auth, &original)
        .await
        .unwrap()
        .0
        .into_result()
        .unwrap();
    let changed = write("same-key", "second", "changed", None);
    assert_eq!(
        ordinary(&broker, &auth, &changed).await.unwrap_err().code,
        "idempotency_conflict"
    );
    assert_eq!(
        std::fs::read(directory.path().join("first")).unwrap(),
        b"original"
    );
    assert!(!directory.path().join("second").exists());
    broker.shutdown().await;
}
