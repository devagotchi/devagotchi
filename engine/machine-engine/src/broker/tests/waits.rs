//! Waits: on settlement, on new output, timing out, the operations revision, the cap.

use super::*;

#[tokio::test]
async fn a_wait_answers_when_the_operation_settles() {
    let directory = tempfile::tempdir().unwrap();
    let (shell, args) = pause_between_lines();
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "settles", &shell, args, 20_000);
    let began = Instant::now();
    let view = broker.wait(wait_for(&id, None, MAX_WAIT_MS)).await.unwrap();
    assert!(matches!(view.state, OperationState::Succeeded), "{view:?}");
    // Answered when the work ended, not at the end of the 50 s wait.
    assert!(
        began.elapsed() < Duration::from_secs(30),
        "{:?}",
        began.elapsed()
    );
    assert!(output_text(&broker, &id).contains("second"));
    broker.shutdown().await;
}

#[tokio::test]
async fn a_wait_with_output_after_wakes_on_new_output() {
    let directory = tempfile::tempdir().unwrap();
    let (shell, args) = pause_between_lines();
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "output", &shell, args, 20_000);
    let first = broker
        .wait(wait_for(&id, Some(0), MAX_WAIT_MS))
        .await
        .unwrap();
    // Woken by `first`, while the command still pauses before `second`.
    assert!(!first.state.terminal(), "{first:?}");
    assert!(output_text(&broker, &id).contains("first"));
    let view = settle(&broker, &id).await;
    assert!(matches!(view.state, OperationState::Succeeded), "{view:?}");
    broker.shutdown().await;
}

#[tokio::test]
async fn a_wait_that_times_out_returns_the_operation_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let (shell, args) = pause_between_lines();
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "times-out", &shell, args, 20_000);
    let began = Instant::now();
    let view = broker.wait(wait_for(&id, None, 300)).await.unwrap();
    assert!(!view.state.terminal(), "{view:?}");
    assert!(began.elapsed() >= Duration::from_millis(300));
    for bad in [0, MAX_WAIT_MS + 1] {
        let error = broker.wait(wait_for(&id, None, bad)).await.unwrap_err();
        assert_eq!(error.code, "invalid_request", "{error:?}");
    }
    let unknown = uuid::Uuid::new_v4().to_string();
    let missing = broker
        .wait(wait_for(&unknown, None, 300))
        .await
        .unwrap_err();
    assert_eq!(missing.code, "not_found", "{missing:?}");
    settle(&broker, &id).await;
    broker.shutdown().await;
}

#[tokio::test]
async fn the_operations_revision_moves_on_admission_and_state_not_on_output() {
    let directory = tempfile::tempdir().unwrap();
    let (shell, args) = pause_between_lines();
    let broker = open_broker(&directory, &[&shell]);
    let changes = |after: Option<u64>, timeout_ms: u64| {
        let broker = Arc::clone(&broker);
        async move {
            broker
                .operations_changed(ChangesInput {
                    after_revision: after,
                    timeout_ms,
                })
                .await
                .unwrap()
                .revision
        }
    };
    let quiet = changes(None, 1).await;
    let began = Instant::now();
    assert_eq!(changes(Some(quiet), 300).await, quiet, "nothing happened");
    assert!(began.elapsed() >= Duration::from_millis(300));
    let id = start(&broker, "roster", &shell, args, 20_000);
    let admitted = changes(Some(quiet), MAX_WAIT_MS).await;
    assert_ne!(admitted, quiet, "an admission changes the list");
    // Woken by `first`; the command then pauses without changing state.
    broker
        .wait(wait_for(&id, Some(0), MAX_WAIT_MS))
        .await
        .unwrap();
    let printing = changes(None, 1).await;
    assert_eq!(
        changes(Some(printing), 1_000).await,
        printing,
        "output alone is not a change"
    );
    settle(&broker, &id).await;
    assert_ne!(
        changes(Some(printing), MAX_WAIT_MS).await,
        printing,
        "settling is a change"
    );
    // A restarted broker counts from zero again: a stale revision answers at once.
    let began = Instant::now();
    changes(Some(u64::MAX), MAX_WAIT_MS).await;
    assert!(began.elapsed() < Duration::from_secs(5));
    broker.shutdown().await;
}

#[tokio::test]
async fn waits_are_capped_and_a_cancellation_wakes_every_waiter() {
    let directory = tempfile::tempdir().unwrap();
    let (shell, args) = pause_between_lines();
    let broker = open_broker(&directory, &[&shell]);
    let id = start(&broker, "many-waiters", &shell, args, 20_000);
    let mut waiters = tokio::task::JoinSet::new();
    for _ in 0..MAX_WAITERS {
        let (broker, id) = (Arc::clone(&broker), id.clone());
        waiters.spawn(async move { broker.wait(wait_for(&id, None, MAX_WAIT_MS)).await });
    }
    while broker.shared.waiters.available_permits() > 0 {
        sleep(Duration::from_millis(10)).await;
    }
    let busy = broker.wait(wait_for(&id, None, 1_000)).await.unwrap_err();
    assert_eq!(busy.code, "wait_busy");
    let began = Instant::now();
    broker.cancel(&id, None).unwrap();
    while let Some(answer) = waiters.join_next().await {
        let view = answer.unwrap().unwrap();
        assert!(matches!(view.state, OperationState::Cancelled), "{view:?}");
    }
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "{:?}",
        began.elapsed()
    );
    broker.shutdown().await;
}
