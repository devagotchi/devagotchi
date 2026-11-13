//! The export against a stand-in ClickHouse that can be up, down, or take a send and lose
//! its answer, over real hosted boards in Postgres (the test server CI sets). What the real
//! ClickHouse does with the rows (one per event id, the statistics) is `tests/analytics_probe.py`.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde_json::Value;

use super::*;
use crate::{
    agenthub::{Actor, NewTicket, hosted::HostedBoard},
    orgs::Orgs,
    store::{Store, testing, transaction},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Up,
    Down,
    /// Takes the rows, then answers 500: as when the answer is lost on the way back.
    LoseAnswer,
}

#[derive(Clone)]
struct Fake {
    rows: Arc<Mutex<Vec<Value>>>,
    mode: Arc<Mutex<Mode>>,
    keys: Arc<Mutex<BTreeSet<String>>>,
}

async fn clickhouse(
    State(fake): State<Fake>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> (StatusCode, String) {
    if let Some(key) = headers
        .get("x-clickhouse-key")
        .and_then(|v| v.to_str().ok())
    {
        fake.keys.lock().unwrap().insert(key.to_string());
    }
    let mode = *fake.mode.lock().unwrap();
    if mode == Mode::Down {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Code: 210. DB::NetException: down".into(),
        );
    }
    if let Some(sql) = query.get("query")
        && sql.starts_with("INSERT")
    {
        let mut rows = fake.rows.lock().unwrap();
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            rows.push(serde_json::from_str(line).unwrap());
        }
        if mode == Mode::LoseAnswer {
            return (StatusCode::INTERNAL_SERVER_ERROR, "lost".into());
        }
    }
    (StatusCode::OK, String::new())
}

async fn fake() -> (String, Fake) {
    let fake = Fake {
        rows: Arc::default(),
        mode: Arc::new(Mutex::new(Mode::Up)),
        keys: Arc::default(),
    };
    let app = Router::new()
        .route("/", post(clickhouse))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, fake)
}

fn config(url: &str) -> ClickHouseConfig {
    ClickHouseConfig {
        url: url.to_string(),
        database: "devagotchi_test".into(),
        user: Some("devagotchi".into()),
    }
}

async fn tickets(board: &HostedBoard, from: usize, to: usize) {
    let human = Actor::human("alice");
    for n in from..to {
        board
            .create(
                &human,
                &NewTicket {
                    title: format!("ticket {n}"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
}

async fn events_in_postgres(store: &Store, tenant: &str) -> BTreeSet<String> {
    let ids: Vec<String> = transaction!(store, |connection| {
        store.scope(connection, tenant).await?;
        Ok(
            sqlx::query_scalar("SELECT event_id FROM board_outbox WHERE tenant_id = $1")
                .bind(tenant)
                .fetch_all(&mut *connection)
                .await?,
        )
    })
    .unwrap();
    ids.into_iter().collect()
}

fn distinct(fake: &Fake) -> BTreeSet<String> {
    fake.rows
        .lock()
        .unwrap()
        .iter()
        .map(|row| row["event_id"].as_str().unwrap().to_string())
        .collect()
}

/// Up, down, an answer lost, up again, and everything sent again: every event reaches
/// ClickHouse at least once, nothing is lost while it is down, the health page says how far
/// behind it is, and the rows ClickHouse holds carry each event id however often it came.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_event_reaches_clickhouse_at_least_once_and_nothing_is_lost_while_it_is_down() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    let org = Orgs::new(test.store.clone())
        .create("alice", "acme", "Acme")
        .await
        .unwrap();
    let board = HostedBoard::new(test.store.clone(), &org.id, "demo").unwrap();
    board.initialize().await.unwrap();
    let (url, fake) = fake().await;
    let warehouse =
        Warehouse::new(&config(&url), test.store.clone(), Some("s3cret".into())).unwrap();

    tickets(&board, 0, 3).await;
    let sent = warehouse.round().await.unwrap();
    assert_eq!(sent, events_in_postgres(&test.store, &org.id).await.len());
    let health = warehouse.health().await;
    assert_eq!(health["ready"], true, "{health}");
    assert_eq!(health["pending"], 0, "{health}");
    assert_eq!(health["degraded"], false, "{health}");
    assert_eq!(warehouse.round().await.unwrap(), 0, "nothing waits");
    let first = fake.rows.lock().unwrap()[0].clone();
    assert_eq!(first["tenant_id"], org.id.as_str());
    assert_eq!(first["project"], "demo");
    assert_eq!(first["actor"], "alice");
    assert!(first["at_ms"].as_i64().unwrap() > 0);
    // The password goes in the header ClickHouse reads, and nowhere else.
    assert_eq!(
        fake.keys.lock().unwrap().iter().collect::<Vec<_>>(),
        vec!["s3cret"]
    );

    // Down: the board keeps working, the events wait in Postgres, the health page says so.
    *fake.mode.lock().unwrap() = Mode::Down;
    tickets(&board, 3, 6).await;
    assert!(warehouse.round().await.is_err());
    let health = warehouse.health().await;
    assert_eq!(health["degraded"], true, "{health}");
    assert!(health["pending"].as_i64().unwrap() >= 3, "{health}");
    assert!(health["oldest_pending_secs"].is_number(), "{health}");
    assert!(
        health["last_error"].as_str().unwrap().contains("503"),
        "{health}"
    );
    assert!(!health.to_string().contains("s3cret"));

    // Taken, but the answer is lost: sent again on the next round.
    *fake.mode.lock().unwrap() = Mode::LoseAnswer;
    assert!(warehouse.round().await.is_err());
    *fake.mode.lock().unwrap() = Mode::Up;
    warehouse.round().await.unwrap();
    let all = events_in_postgres(&test.store, &org.id).await;
    assert_eq!(distinct(&fake), all, "every event, at least once");
    assert!(
        fake.rows.lock().unwrap().len() > all.len(),
        "some came twice"
    );
    let health = warehouse.health().await;
    assert_eq!(
        (health["pending"].clone(), health["degraded"].clone()),
        (0.into(), false.into()),
        "{health}"
    );

    // Everything sent again, as after a ClickHouse that lost its data.
    let marked = resend(&test.store).await.unwrap();
    assert_eq!(marked as usize, all.len());
    let sent = warehouse.round().await.unwrap();
    assert_eq!(sent, all.len());
    assert_eq!(distinct(&fake), all);
}

#[test]
fn a_board_event_becomes_one_row_and_anything_else_keeps_its_kind() {
    let event = serde_json::json!({
        "seq": 7, "at": 1_800_000_000_123_i64, "actor": "codex", "action": "ticket.claimed",
        "ticket": "dm-1", "data": {"lease": 1}, "fence": 3, "prev_hash": "p", "hash": "h"
    });
    let line: Value = serde_json::from_str(&row(
        "t-1",
        "demo",
        "event:7:h",
        "ticket.claimed",
        &event.to_string(),
        0,
        "org-t-1",
        "web",
    ))
    .unwrap();
    assert_eq!(line["seq"], 7);
    assert_eq!(line["at_ms"], 1_800_000_000_123_i64);
    assert_eq!(line["actor"], "codex");
    assert_eq!(line["ticket"], "dm-1");
    assert_eq!(line["fence"], 3);
    assert_eq!(line["data"], r#"{"lease":1}"#);
    let import: Value = serde_json::from_str(&row(
        "t-1",
        "demo",
        "import:x",
        "board.imported",
        r#"{"receipt":1}"#,
        42,
        "",
        "demo",
    ))
    .unwrap();
    assert_eq!(import["action"], "board.imported");
    assert_eq!(import["at_ms"], 42);
    assert_eq!(import["ticket"], Value::Null);
}

/// The real ClickHouse, when `DEVAGOTCHI_TEST_CLICKHOUSE_URL` names one (with
/// `DEVAGOTCHI_TEST_CLICKHOUSE_USER` and `_PASSWORD`): the schema is made, the rows go in,
/// a resend leaves one event per id in every count, and a board's statistics answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_real_clickhouse_keeps_one_event_per_id_and_answers_a_boards_statistics() {
    let (Some(test), Ok(url)) = (
        testing::postgres().await,
        std::env::var("DEVAGOTCHI_TEST_CLICKHOUSE_URL"),
    ) else {
        return;
    };
    let org = Orgs::new(test.store.clone())
        .create("alice", "acme", "Acme")
        .await
        .unwrap();
    let board = HostedBoard::new(test.store.clone(), &org.id, "demo").unwrap();
    board.initialize().await.unwrap();
    let database = format!("dvg_test_{}", hex::encode(crate::link::random::<6>()));
    let warehouse = Warehouse::new(
        &ClickHouseConfig {
            url,
            database: database.clone(),
            user: std::env::var("DEVAGOTCHI_TEST_CLICKHOUSE_USER").ok(),
        },
        test.store.clone(),
        std::env::var("DEVAGOTCHI_TEST_CLICKHOUSE_PASSWORD").ok(),
    )
    .unwrap();
    tickets(&board, 0, 4).await;
    let claimer = Actor::agent("codex");
    let ticket = board.list(&Default::default()).await.unwrap()[0].id.clone();
    let lease = board.claim(&claimer, &ticket, None).await.unwrap();
    board
        .complete(&claimer, &ticket, lease.fence, "done")
        .await
        .unwrap();
    let all = events_in_postgres(&test.store, &org.id).await;
    warehouse.round().await.unwrap();
    assert_eq!(
        warehouse.count(&org.id).await.unwrap(),
        (all.len() as i64, all.len() as i64)
    );

    resend(&test.store).await.unwrap();
    warehouse.round().await.unwrap();
    let (events, rows) = warehouse.count(&org.id).await.unwrap();
    assert_eq!(
        (events, rows),
        (all.len() as i64, all.len() as i64),
        "FINAL: one row per id"
    );

    let stats = warehouse
        .board_stats(&org.id, board.board_id(), 30)
        .await
        .unwrap();
    assert_eq!(stats["events"], all.len() as i64, "{stats}");
    let created: i64 = stats["throughput"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["action"] == "ticket.created")
        .map(|row| row["events"].as_i64().unwrap())
        .sum();
    assert_eq!(created, 4, "{stats}");
    assert!(
        stats["actors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["actor"] == "codex"),
        "{stats}"
    );
    assert_eq!(stats["completion"]["tickets"], 1, "{stats}");
    warehouse
        .execute(&format!("DROP DATABASE {database}"), None, &[])
        .await
        .unwrap();
}
