//! A hosted board's life: one authority and atomic history, one winner among concurrent
//! claims, and the inbox and standing approvals the local board has, held the same here.

use devagotchi_store::{or_skip, transaction};

use crate::hosted::*;
use crate::{Actor, GENESIS, NewTicket, event_hash};

const TENANT: &str = "t-00000000-0000-4000-8000-000000000001";
const OTHER: &str = "t-00000000-0000-4000-8000-000000000002";

#[tokio::test]
async fn native_postgres_board_lifecycle_has_one_authority_and_atomic_history() {
    for test in super::stores().await {
        let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        let peer = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        let other = HostedBoard::new(test.store.clone(), OTHER, "demo").unwrap();
        assert!(board.initialize().await.unwrap());
        assert!(!peer.initialize().await.unwrap());
        assert!(other.initialize().await.unwrap());
        let owner = Actor::human("owner");
        let worker = Actor::agent("worker");
        let reviewer = Actor::agent("reviewer");
        let ticket = board
            .create(
                &owner,
                &NewTicket {
                    title: "Native Postgres".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(other.get(&ticket.id).await.unwrap_err().code, "not_found");
        let held = board.claim(&worker, &ticket.id, None).await.unwrap();
        assert_eq!(
            peer.claim(&reviewer, &ticket.id, None)
                .await
                .unwrap_err()
                .code,
            "held_by_other"
        );
        assert!(
            peer.claim(&worker, &ticket.id, None)
                .await
                .unwrap()
                .already_held
        );
        assert_eq!(
            board
                .heartbeat(&worker, &ticket.id, held.fence + 1, None)
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
        board
            .heartbeat(
                &worker,
                &ticket.id,
                held.fence,
                Some(serde_json::json!({"step":1})),
            )
            .await
            .unwrap();
        assert_eq!(
            peer.get(&ticket.id).await.unwrap().progress,
            Some(serde_json::json!({"step":1}))
        );
        assert_eq!(
            board
                .add_note(
                    &worker,
                    &ticket.id,
                    "decision",
                    "Keep the fence",
                    None,
                    None
                )
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
        board
            .add_note(
                &worker,
                &ticket.id,
                "decision",
                "Keep the fence",
                None,
                Some(held.fence),
            )
            .await
            .unwrap();
        let reviewed = board
            .complete(&worker, &ticket.id, held.fence, "Ready for review")
            .await
            .unwrap();
        assert_eq!(reviewed.state, "review");
        assert_eq!(
            board
                .claim(&worker, &ticket.id, None)
                .await
                .unwrap_err()
                .code,
            "distinct_required"
        );
        let review = peer.claim(&reviewer, &ticket.id, None).await.unwrap();
        assert!(review.fence > held.fence);
        assert_eq!(
            board
                .complete(&worker, &ticket.id, held.fence, "stale")
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
        assert_eq!(
            peer.complete(&reviewer, &ticket.id, review.fence, "Verified")
                .await
                .unwrap()
                .state,
            "done"
        );
        let events = board.events().await.unwrap();
        let mut previous = GENESIS.to_string();
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event.seq, index as i64 + 1);
            assert_eq!(event.prev_hash, previous);
            assert_eq!(event.hash, event_hash(&previous, event));
            previous = event.hash.clone();
        }
        transaction!(&test.store, |connection| {
            test.store.scope(connection, TENANT).await?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM board_outbox WHERE tenant_id=$1 AND board_id='demo'",
            )
            .bind(TENANT)
            .fetch_one(&mut *connection)
            .await?;
            assert_eq!(count, events.len() as i64);
            Ok(())
        })
        .unwrap();
    }
}

#[tokio::test]
async fn concurrent_native_claims_have_one_winner_and_reclaim_burns_fences() {
    for test in super::stores().await {
        let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        board.initialize().await.unwrap();
        let a = Actor::agent("a");
        let b = Actor::agent("b");
        let ticket = board
            .create(
                &a,
                &NewTicket {
                    title: "Racing claims".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let (one, two) = tokio::join!(
            board.claim(&a, &ticket.id, None),
            board.claim(&b, &ticket.id, None)
        );
        assert_ne!(one.is_ok(), two.is_ok());
        let (holder, other, held) = match (one, two) {
            (Ok(held), Err(error)) => {
                assert_eq!(error.code, "held_by_other");
                (&a, &b, held)
            }
            (Err(error), Ok(held)) => {
                assert_eq!(error.code, "held_by_other");
                (&b, &a, held)
            }
            _ => panic!("claims did not settle with one winner"),
        };
        board
            .release(holder, &ticket.id, held.fence, "handover")
            .await
            .unwrap();
        let next = board.claim(other, &ticket.id, None).await.unwrap();
        assert!(next.fence > held.fence);
        assert_eq!(
            board
                .heartbeat(holder, &ticket.id, held.fence, None)
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
    }
}

/// Approving ahead of time on a hosted board: granted, used and ended in Postgres
/// transactions, on the board's chain.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_board_approves_a_kind_ahead_of_time() {
    let test = or_skip!(super::postgres());
    let hosted = HostedBoard::new(test.store.clone(), TENANT, "ahead").unwrap();
    hosted.initialize().await.unwrap();
    let board = crate::Board::from_hosted(hosted).await.unwrap();
    tokio::task::spawn_blocking(move || super::standing::standing_approvals_hold(&board, None))
        .await
        .unwrap();
}

/// A note reaching who it is for on a hosted board (`agenthub::inbox`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_board_delivers_a_note_once() {
    let test = or_skip!(super::postgres());
    let hosted = HostedBoard::new(test.store.clone(), TENANT, "inbox").unwrap();
    hosted.initialize().await.unwrap();
    let board = crate::Board::from_hosted(hosted).await.unwrap();
    tokio::task::spawn_blocking(move || super::inbox::inbox_holds(&board))
        .await
        .unwrap();
}
