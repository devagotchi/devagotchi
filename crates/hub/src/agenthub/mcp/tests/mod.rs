//! The board's MCP face: boards opened once, leases reaped when they lapse, the fence
//! memory, and refusals that name the next call.

use std::time::Duration;

use devagotchi_contracts::BridgeError;

use super::*;
use crate::{agenthub::Actor, config::HubConfig};

fn hub_config(project: &str, dir: &std::path::Path) -> HubConfig {
    HubConfig::parse(
        &serde_json::json!({
            "listen": "127.0.0.1:1",
            "public_url": "http://127.0.0.1:1",
            "auth": { "mode": "dev", "tokens_file": dir.join("tokens.json") },
            "brokers": [],
            "boards": [{ "project": project }],
            "grants": [{ "subject": "agent", "boards": [project] }],
        })
        .to_string(),
    )
    .expect("config")
}

/// The reaper sleeps until a lease lapses, then expires it: the ticket is back in its
/// queue within moments of the lapse, with no sweep timer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lapsed_lease_is_reaped_when_it_lapses() {
    let test = or_skip!(crate::store::testing::postgres());
    let dir = tempfile::tempdir().unwrap();
    let boards = Arc::new(Boards::new(&hub_config("site", dir.path()), test.store).unwrap());
    // A hosted board hears of its changes as `hub serve` does: Postgres notifications.
    let changes = Boards::postgres_listener(&test.url).await.unwrap();
    tokio::spawn(boards.clone().listen_postgres(changes));
    let board = boards.open("site").unwrap();
    let operator = Actor::human("jesse");
    let mut settings = board.settings().unwrap();
    settings.lease_min_ttl_ms = 100;
    board.put_settings(&operator, &settings).unwrap();
    let id = board
        .create(
            &operator,
            crate::agenthub::NewTicket {
                title: "lapses".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
    tokio::spawn(boards.clone().reap_on_time());
    let began = tokio::time::Instant::now();
    board
        .claim(&Actor::agent("claude"), &id, Some(400))
        .unwrap();
    let mut changes = board.changes();
    changes.borrow_and_update();
    let reaped = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            changes.changed().await.unwrap();
            if board.get(&id).unwrap().ticket.holder.is_none() {
                return board.get(&id).unwrap().ticket;
            }
        }
    })
    .await
    .expect("the lease was reaped when it lapsed");
    assert_eq!(reaped.state, "ready");
    let elapsed = began.elapsed();
    assert!(elapsed >= Duration::from_millis(350), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_board_opens_once_and_the_fence_memory_follows_the_lease() {
    let test = or_skip!(crate::store::testing::postgres());
    let dir = tempfile::tempdir().unwrap();
    let boards = Boards::new(&hub_config("devagotchi", dir.path()), test.store).unwrap();
    assert!(!boards.is_empty());
    let first = boards.open("devagotchi").unwrap();
    let again = boards.open("devagotchi").unwrap();
    assert!(Arc::ptr_eq(&first, &again), "one open board per project");
    assert_eq!(
        boards.open("other").err().expect("refused").code,
        "unknown_board",
        "a project the config does not name is no board"
    );

    let missing = boards
        .fence_for("agent", "devagotchi", "dvg-1", None)
        .unwrap_err();
    assert_eq!(missing.code, "stale_fence");
    assert_eq!(
        boards
            .fence_for("agent", "devagotchi", "dvg-1", Some(7))
            .unwrap(),
        7,
        "a given fence wins"
    );
    boards.remember_fence("agent", "devagotchi", "dvg-1", 3);
    assert_eq!(
        boards
            .fence_for("agent", "devagotchi", "dvg-1", None)
            .unwrap(),
        3
    );
    assert_eq!(
        boards
            .fence_for("someone-else", "devagotchi", "dvg-1", None)
            .unwrap_err()
            .code,
        "stale_fence",
        "fences are remembered per subject"
    );
    boards.forget_fence("agent", "devagotchi", "dvg-1");
    assert_eq!(
        boards
            .fence_for("agent", "devagotchi", "dvg-1", None)
            .unwrap_err()
            .code,
        "stale_fence"
    );
}

#[test]
fn refusals_name_what_to_call_next() {
    let refused = refuse(BridgeError::new("held_by_other", "leased"), Some("dvg-1"));
    assert_eq!(refused.0.code, "held_by_other");
    assert_eq!(refused.0.ticket.as_deref(), Some("dvg-1"));
    assert!(
        refused.0.next.contains("board_get dvg-1"),
        "{}",
        refused.0.next
    );
    let unknown = refuse(BridgeError::new("something_new", "?"), None);
    assert!(unknown.0.next.contains("board_prime"));
    let environment = refuse(
        BridgeError::new(
            "unknown_environment",
            "environment \"qa\" is not one of this board's: dev, prod",
        ),
        None,
    );
    assert!(
        environment.0.next.contains("(dev, prod)"),
        "{}",
        environment.0.next
    );
}

mod accounts;
mod contract;
mod held_updates;
mod lease_receipts;
mod mutation_receipts;
mod registered_agents;
mod tickets;
