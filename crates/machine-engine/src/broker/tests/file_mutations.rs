//! Mutation ordering at the broker (`broker/files.rs`): which requests take the gate, what
//! cancellation does, and the bound on waiting for it. Real-client races through two MCP
//! adapters live in tests/file_mutation_concurrency.py.

use super::*;
use crate::broker::files::MUTATION_WAIT;

fn write(path: &Path, text: &str) -> RequestBody {
    RequestBody::FileWrite(FileWriteInput {
        path: path.display().to_string(),
        text: Some(text.into()),
        bytes_base64: None,
        mode: WriteMode::Overwrite,
        expected_sha256: None,
        create_dirs: false,
    })
}

fn path_of(path: &Path) -> String {
    path.display().to_string()
}

/// The gate as a running mutation holds it, for a test to keep other mutations waiting.
async fn hold(broker: &Broker) -> tokio::sync::OwnedMutexGuard<()> {
    broker.mutation_gate(Duration::from_secs(5)).await.unwrap()
}

/// `work` run as a mutation runs: the gate taken, then moved into the blocking worker.
async fn mutate<F>(broker: &Broker, work: F) -> BridgeResult<ResponseBody>
where
    F: FnOnce() -> BridgeResult<ResponseBody> + Send + 'static,
{
    let gate = broker.mutation_gate(MUTATION_WAIT).await?;
    broker
        .on_files_with(Some(gate), move |_| ("test mutation", work()))
        .await
}

#[tokio::test]
async fn every_mutation_waits_on_the_gate_but_readers_and_queries_do_not() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let root = directory.path();
    let original = root.join("original.txt");
    let source = root.join("source.txt");
    std::fs::write(&original, "old").unwrap();
    std::fs::write(&source, "source").unwrap();
    let guard = hold(&broker).await;
    let requests = [
        write(&root.join("new.txt"), "new"),
        RequestBody::FileEdit(FileEditInput {
            path: path_of(&original),
            old_text: "old".into(),
            new_text: "edited".into(),
            expected_replacements: 1,
        }),
        RequestBody::FileMkdir(DirectoryInput {
            path: path_of(&root.join("made/nested")),
        }),
        RequestBody::FileMove(FileMoveInput {
            source: path_of(&source),
            destination: path_of(&root.join("moved.txt")),
            overwrite: false,
        }),
    ];
    let mut pending = Vec::new();
    for request in requests {
        let broker = Arc::clone(&broker);
        pending.push(tokio::spawn(async move { broker.handle(request).await }));
    }
    sleep(Duration::from_millis(25)).await;
    assert!(pending.iter().all(|task| !task.is_finished()));
    assert!(!root.join("new.txt").exists());
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "old");
    assert!(source.exists());
    assert!(!root.join("made").exists());

    let read = broker.handle(RequestBody::FileRead(FileReadInput {
        path: path_of(&original),
        offset: 0,
        max_bytes: 1024,
        start_line: None,
        line_count: None,
    }));
    let ResponseBody::File(content) = timeout(Duration::from_secs(1), read)
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("expected the file's content")
    };
    assert_eq!(content.text.as_deref(), Some("old"));
    let queries = [
        RequestBody::ConfigGet(ConfigGetInput {}),
        RequestBody::OperationList(ListInput {
            offset: 0,
            limit: 10,
            scope: None,
            caller: None,
        }),
    ];
    for query in queries {
        timeout(Duration::from_secs(1), broker.handle(query))
            .await
            .expect("a query waited on the mutation gate")
            .unwrap();
    }

    drop(guard);
    for task in pending {
        timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "edited");
    assert_eq!(
        std::fs::read_to_string(root.join("new.txt")).unwrap(),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("moved.txt")).unwrap(),
        "source"
    );
    assert!(root.join("made/nested").is_dir());
    broker.shutdown().await;
}

#[tokio::test]
async fn listing_searching_and_file_info_never_take_the_gate() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let root = directory.path();
    std::fs::write(root.join("found.txt"), "needle\n").unwrap();
    let _guard = hold(&broker).await;
    let search: FileSearchInput = serde_json::from_value(serde_json::json!({
        "path": path_of(root),
        "pattern": "needle",
        "target": "contents",
    }))
    .unwrap();
    let reads = [
        RequestBody::FileList(FileListInput {
            path: path_of(root),
            depth: 1,
            max_entries: 10,
            cursor: None,
        }),
        RequestBody::FileInfo(FileInfoInput {
            path: path_of(&root.join("found.txt")),
        }),
        RequestBody::FileSearch(search),
    ];
    for read in reads {
        timeout(Duration::from_secs(5), broker.handle(read))
            .await
            .expect("a read waited on the mutation gate")
            .unwrap();
    }
    broker.shutdown().await;
}

#[tokio::test]
async fn cancelling_a_waiting_mutation_has_no_effect() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let guard = hold(&broker).await;
    let file = directory.path().join("cancelled.txt");
    let request = write(&file, "must not be written");
    let caller = {
        let broker = Arc::clone(&broker);
        tokio::spawn(async move { broker.handle(request).await })
    };
    tokio::task::yield_now().await;
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    drop(guard);
    broker
        .handle(write(&directory.path().join("next.txt"), "ok"))
        .await
        .unwrap();
    assert!(!file.exists());
    broker.shutdown().await;
}

#[tokio::test]
async fn cancelled_caller_cannot_release_a_running_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let first = {
        let broker = Arc::clone(&broker);
        tokio::spawn(async move {
            mutate(&broker, move || {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Err(BridgeError::new("test_done", "released"))
            })
            .await
        })
    };
    started_rx.await.unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    let blocked = broker
        .mutation_gate(Duration::from_millis(25))
        .await
        .is_err();
    // Release the blocking worker before asserting, so a regression cannot hang the test.
    release_tx.send(()).unwrap();
    assert!(
        blocked,
        "cancelling the caller released an in-flight mutation's gate"
    );
    broker
        .handle(write(&directory.path().join("after.txt"), "ok"))
        .await
        .unwrap();
    broker.shutdown().await;
}

#[tokio::test]
async fn a_failed_or_panicked_mutation_releases_the_gate() {
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let error = mutate(&broker, || Err(BridgeError::new("injected", "failed")))
        .await
        .unwrap_err();
    assert_eq!(error.code, "injected");
    let error = mutate(&broker, || panic!("injected worker failure"))
        .await
        .unwrap_err();
    assert_eq!(error.code, "internal_error");
    timeout(
        Duration::from_secs(5),
        broker.handle(write(&directory.path().join("after.txt"), "ok")),
    )
    .await
    .unwrap()
    .unwrap();
    broker.shutdown().await;
}

/// A mutation queued behind one that holds the gate past the bound is refused before the
/// local client's deadline, and never lands afterwards.
#[tokio::test]
async fn a_mutation_that_waits_past_the_bound_is_refused_and_never_lands() {
    assert!(MUTATION_WAIT < devagotchi_ipc::IO_TIMEOUT);
    let directory = tempfile::tempdir().unwrap();
    let broker = open_broker(&directory, &[]);
    let file = directory.path().join("late.txt");
    let guard = hold(&broker).await;
    let asked = Instant::now();
    let refused = timeout(
        devagotchi_ipc::IO_TIMEOUT * 2,
        broker.handle(write(&file, "must not land late")),
    )
    .await
    .expect("a queued mutation kept waiting past the local client's deadline")
    .unwrap_err();
    let waited = asked.elapsed();
    assert_eq!(refused.code, "mutation_busy", "{refused:?}");
    assert!(waited >= MUTATION_WAIT, "refused after {waited:?}");
    assert!(
        waited < devagotchi_ipc::IO_TIMEOUT,
        "refused only after {waited:?}, when a local client has already given up"
    );
    assert!(!file.exists());
    drop(guard);
    // Nothing is left queued to apply the refused write once the gate is free.
    broker
        .handle(write(&directory.path().join("next.txt"), "ok"))
        .await
        .unwrap();
    assert!(!file.exists());
    broker.shutdown().await;
}
