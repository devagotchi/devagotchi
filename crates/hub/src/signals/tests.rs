//! The signal contract: its published schema, and a sink over real sources in Postgres (the
//! test server CI sets) against a stand-in receiver that can be up or down.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};

use super::*;
use crate::{
    audit::AuditEntry,
    agenthub::{Actor, NewTicket, hosted::HostedBoard},
    config::SinkConfig,
    orgs::{Orgs, Role},
    store::testing,
};

/// The schema `docs/signals/v1.schema.json` publishes is the one the type makes. To change the
/// contract, change `Signal`, then write the file again with
/// `DEVAGOTCHI_WRITE_SIGNAL_SCHEMA=1 cargo test -p devagotchi-hub signals::tests::the_published`.
#[test]
fn the_published_schema_is_the_one_the_type_makes() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/signals/v1.schema.json");
    let made = serde_json::to_string_pretty(&json_schema()).unwrap() + "\n";
    if std::env::var("DEVAGOTCHI_WRITE_SIGNAL_SCHEMA").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &made).unwrap();
    }
    let published = std::fs::read_to_string(&path)
        .expect("docs/signals/v1.schema.json")
        .replace("\r\n", "\n");
    assert_eq!(published, made, "the published schema is out of date");
    let schema = json_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        required,
        vec!["schema", "id", "source", "kind", "at_ms"],
        "what every signal carries"
    );
}

#[test]
fn the_audit_chains_times_become_milliseconds() {
    assert_eq!(rfc3339_ms("1970-01-01T00:00:00.000Z"), Some(0));
    assert_eq!(
        rfc3339_ms("2026-11-10T12:00:00.500Z"),
        Some(1_794_312_000_500)
    );
    assert_eq!(
        rfc3339_ms("2000-02-29T23:59:59.999Z"),
        Some(951_868_799_999)
    );
    assert_eq!(rfc3339_ms("not a time"), None);
}

#[derive(Clone)]
struct Receiver {
    up: Arc<Mutex<bool>>,
    lines: Arc<Mutex<Vec<Value>>>,
    signed: Arc<Mutex<Vec<bool>>>,
    tokens: Arc<Mutex<BTreeSet<String>>>,
}

async fn receive(State(r): State<Receiver>, headers: HeaderMap, body: String) -> StatusCode {
    if !*r.up.lock().unwrap() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    assert_eq!(headers["devagotchi-signal-schema"], SCHEMA);
    assert_eq!(headers["content-type"], "application/x-ndjson");
    if let Some(auth) = headers.get("authorization") {
        r.tokens
            .lock()
            .unwrap()
            .insert(auth.to_str().unwrap().to_string());
    }
    let signature = headers
        .get("devagotchi-signature")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("sha256="))
        .and_then(|v| hex::decode(v).ok());
    let mut mac = Hmac::<Sha256>::new_from_slice(b"a shared secret").unwrap();
    mac.update(body.as_bytes());
    r.signed
        .lock()
        .unwrap()
        .push(signature.is_some_and(|s| mac.verify_slice(&s).is_ok()));
    let mut lines = r.lines.lock().unwrap();
    for line in body.lines() {
        lines.push(serde_json::from_str(line).unwrap());
    }
    StatusCode::ACCEPTED
}

async fn receiver() -> (String, Receiver) {
    let r = Receiver {
        up: Arc::new(Mutex::new(true)),
        lines: Arc::default(),
        signed: Arc::default(),
        tokens: Arc::default(),
    };
    let app = Router::new()
        .route("/signals", post(receive))
        .with_state(r.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://127.0.0.1:{}/signals",
        listener.local_addr().unwrap().port()
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, r)
}

fn sink_config(name: &str, url: &str, fields: SinkFields) -> SinkConfig {
    SinkConfig {
        name: name.into(),
        url: url.into(),
        token_env: Some("SINK_TOKEN".into()),
        secret_env: Some("SINK_SECRET".into()),
        fields,
        sources: Vec::new(),
    }
}

fn env(name: &str) -> Option<String> {
    match name {
        "SINK_TOKEN" => Some("a-bearer-token".into()),
        "SINK_SECRET" => Some("a shared secret".into()),
        _ => None,
    }
}

/// Every board event, tenant event and tool call reaches the sink once it answers; while it
/// is down nothing moves and it says how far behind it is; a `metadata` sink never gets a
/// signal's `data`; every batch is signed and carries the token; a second sink is not held
/// back by the first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sink_gets_every_signal_once_it_answers_and_never_more_than_it_takes() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    let orgs = Orgs::new(test.store.clone());
    let org = orgs.create("alice", "acme", "Acme").await.unwrap();
    orgs.set_member(&org, "alice", "bob", Role::Member)
        .await
        .unwrap();
    let board = HostedBoard::for_owner(
        test.store.clone(),
        &org.id,
        &crate::orgs::owner_of(&org.id),
        "site",
    )
    .unwrap();
    board.initialize().await.unwrap();
    for n in 0..3 {
        board
            .create(
                &Actor::human("alice"),
                &NewTicket {
                    title: format!("ticket {n} with private words"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let audit = Arc::new(Audit::open(&test.dir.path().join("audit.sqlite3")).unwrap());
    for (tool, outcome) in [
        ("board_create", "ok"),
        ("board_create", "read_only"),
        ("run", "ok"),
    ] {
        audit
            .record(&AuditEntry {
                subject: "bob",
                via: "hub",
                tool,
                broker: "acme/site",
                operation_id: "op-1",
                outcome,
            })
            .unwrap();
    }

    let (url, inbox) = receiver().await;
    let (other_url, other) = receiver().await;
    let metadata = Sink::new(
        &sink_config("watcher", &url, SinkFields::Metadata),
        test.store.clone(),
        audit.clone(),
        env,
    )
    .unwrap();
    let full = Sink::new(
        &sink_config("dataset", &other_url, SinkFields::Full),
        test.store.clone(),
        audit.clone(),
        env,
    )
    .unwrap();

    // Down: nothing moves, and it says so; the other sink is not held back.
    *inbox.up.lock().unwrap() = false;
    assert!(metadata.round().await.is_err());
    let health = metadata.health();
    assert_eq!(health["degraded"], true, "{health}");
    assert!(health["waiting"].as_u64().unwrap() > 0, "{health}");
    assert!(health["oldest_waiting_secs"].is_number(), "{health}");
    assert!(!health.to_string().contains("a-bearer-token"));
    let sent_full = full.round().await.unwrap();
    assert!(sent_full > 0);

    // Up: everything, once, in order within each scope; then nothing new.
    *inbox.up.lock().unwrap() = true;
    let sent = metadata.round().await.unwrap();
    assert_eq!(sent, sent_full, "the same signals as the other sink");
    assert_eq!(metadata.round().await.unwrap(), 0, "nothing new");
    assert_eq!(metadata.health()["degraded"], false);
    let lines = inbox.lines.lock().unwrap().clone();
    let ids: BTreeSet<&str> = lines.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), lines.len(), "each signal once");
    let kinds = |source: &str| -> Vec<String> {
        lines
            .iter()
            .filter(|l| l["source"] == source)
            .map(|l| l["kind"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        kinds("board")
            .iter()
            .filter(|k| *k == "ticket.created")
            .count(),
        3
    );
    assert!(kinds("tenancy").contains(&"org.member_set".to_string()));
    assert_eq!(
        kinds("audit"),
        vec!["tool.board_create", "tool.board_create", "tool.run"]
    );
    let refused = lines
        .iter()
        .find(|l| l["source"] == "audit" && l["outcome"] == "read_only")
        .unwrap();
    assert_eq!(refused["actor"], "bob");
    assert_eq!(refused["target"], "acme/site");
    let created = lines
        .iter()
        .find(|l| l["kind"] == "ticket.created")
        .unwrap();
    assert_eq!(created["project"], "site");
    assert_eq!(created["owner"], crate::orgs::owner_of(&org.id));
    assert_eq!(created["tenant"], org.id);
    for line in &lines {
        assert_eq!(line["schema"], SCHEMA);
        assert!(line.get("data").is_none(), "metadata only: {line}");
        let signal: Signal = serde_json::from_value(line.clone()).unwrap();
        assert!(signal.at_ms > 1_000_000_000_000, "{line}");
    }
    assert!(
        !serde_json::to_string(&lines)
            .unwrap()
            .contains("private words")
    );
    assert!(
        inbox.signed.lock().unwrap().iter().all(|ok| *ok),
        "every batch signed"
    );
    assert_eq!(
        inbox.tokens.lock().unwrap().iter().collect::<Vec<_>>(),
        vec!["Bearer a-bearer-token"]
    );

    // The full sink got the data, the private words included: it takes them.
    let full_lines = other.lines.lock().unwrap().clone();
    assert!(
        serde_json::to_string(&full_lines)
            .unwrap()
            .contains("private words")
    );

    // More happens: only that goes.
    board
        .create(
            &Actor::agent("codex"),
            &NewTicket {
                title: "later".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let sent = metadata.round().await.unwrap();
    assert!(sent >= 1);
    let lines = inbox.lines.lock().unwrap().clone();
    let ids: BTreeSet<&str> = lines.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), lines.len(), "still each signal once");
}

#[test]
fn a_sinks_url_never_carries_signals_in_the_clear_across_a_network() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = dir.path().join("tokens.json");
    let config = |url: &str| {
        crate::config::HubConfig::parse(
            &serde_json::json!({
                "listen": "127.0.0.1:1", "public_url": "http://127.0.0.1:1",
                "auth": { "mode": "dev", "tokens_file": tokens },
                "brokers": [], "grants": [],
                "signals": { "sinks": [{ "name": "watcher", "url": url }] }
            })
            .to_string(),
        )
    };
    for good in [
        "https://signals.example.com/in",
        "http://127.0.0.1:9000/in",
        "http://localhost/in",
        "http://[::1]:9/in",
        "http://rrsink:8080/in",
    ] {
        assert!(config(good).is_ok(), "{good}: {:?}", config(good).err());
    }
    for bad in [
        "http://signals.example.com/in",
        "http://10.0.0.5/in",
        "ftp://x/in",
        "signals.example.com",
    ] {
        assert!(config(bad).is_err(), "{bad}");
    }
}
