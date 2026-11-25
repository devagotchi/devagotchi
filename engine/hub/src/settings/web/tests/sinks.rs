//! `/admin` → Signal sinks over HTTP (`settings/web/sinks.rs`): a sink saved on the page runs at
//! once with its token, which is sealed; removed, it stops; a config file's sink is shown and its
//! name cannot be taken. On the test Postgres, with a stand-in collector on loopback.

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::tenancy::tests::tenants::{Hosted, hosted_with};
use crate::tenancy::web::tests::operator::{hub_on, operator_cookie};

/// A collector: keeps each batch's bearer token and body.
#[derive(Clone, Default)]
struct Collector {
    got: Arc<Mutex<Vec<(String, String)>>>,
}

impl Collector {
    async fn start() -> (String, Self) {
        use axum::{extract::State, http::HeaderMap, routing::post};
        async fn take(
            State(c): State<Collector>,
            headers: HeaderMap,
            body: String,
        ) -> &'static str {
            let auth = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            c.got.lock().unwrap().push((auth, body));
            ""
        }
        let collector = Self::default();
        let app = axum::Router::new()
            .route("/ingest", post(take))
            .with_state(collector.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://127.0.0.1:{}/ingest",
            listener.local_addr().unwrap().port()
        );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, collector)
    }

    async fn batches(&self, at_least: usize) -> Vec<(String, String)> {
        for _ in 0..150 {
            if self.got.lock().unwrap().len() >= at_least {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        self.got.lock().unwrap().clone()
    }
}

fn sink(
    hub: &Hosted,
    cookie: &str,
    path: &str,
    fields: &[(&str, &str)],
) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}/admin/sinks/{path}", hub.url))
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

async fn running(hub: &Hosted) -> Vec<String> {
    let (_, health) = Hosted::text(hub.get("/healthz", None).await).await;
    let health: serde_json::Value = serde_json::from_str(&health).unwrap();
    health["signals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect()
}

const ALL: [(&str, &str); 3] = [
    ("source.board", "on"),
    ("source.tenancy", "on"),
    ("source.audit", "on"),
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sink_saved_on_the_page_runs_at_once_with_its_sealed_token_and_stops_when_removed() {
    let store = or_skip!(crate::store::testing::empty_postgres());
    let hub = or_skip!(hub_on(Some(&store), None));
    let owner = operator_cookie(&hub).await;
    let (url, collector) = Collector::start().await;

    let (status, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{page}");
    assert!(
        page.contains("id=\"sinks\"") && page.contains("Add a sink"),
        "{page}"
    );
    assert!(running(&hub).await.is_empty());

    // Refused, nothing saved: http to another machine, no kind of signal, a bad name.
    let mut bad = vec![
        ("name", "elsewhere"),
        ("url", "http://logs.example.com/in"),
        ("enabled", "on"),
    ];
    bad.extend(ALL);
    let refused = sink(&hub, &owner, "new", &bad).send().await.unwrap();
    assert_eq!(refused.status(), 400);
    let none = [("name", "quiet"), ("url", url.as_str()), ("enabled", "on")];
    let refused = sink(&hub, &owner, "new", &none).send().await.unwrap();
    assert_eq!(refused.status(), 400);
    let mut named = vec![
        ("name", "Bad Name"),
        ("url", url.as_str()),
        ("enabled", "on"),
    ];
    named.extend(ALL);
    let refused = sink(&hub, &owner, "new", &named).send().await.unwrap();
    assert_eq!(refused.status(), 400);
    assert!(hub.accounts.stored_sinks().await.unwrap().is_empty());

    // Saved: running at once, its token on every batch, sealed at rest, audited without it.
    let mut fields = vec![
        ("name", "logs"),
        ("url", url.as_str()),
        ("token", "sink-token"),
        ("fields", "metadata"),
        ("enabled", "on"),
    ];
    fields.extend(ALL);
    let saved = sink(&hub, &owner, "new", &fields).send().await.unwrap();
    assert_eq!(saved.status(), 303);
    assert_eq!(running(&hub).await, ["logs"]);
    let batches = collector.batches(1).await;
    assert!(!batches.is_empty(), "the sink sent its first batch");
    assert!(
        batches.iter().all(|(auth, _)| auth == "Bearer sink-token"),
        "{batches:?}"
    );
    assert!(
        batches[0].1.contains("devagotchi.signal/v1"),
        "{}",
        batches[0].1
    );
    let stored = hub.accounts.stored_sinks().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert!(!stored[0].sealed_token.is_empty());
    assert!(!stored[0].sealed_token.contains("sink-token"));
    assert!(stored[0].sources.is_empty(), "all three is all");
    let home = hub.accounts.home_tenant().await.unwrap();
    let events = hub.accounts.events(&home, 50).await.unwrap();
    let audited: Vec<&str> = events
        .iter()
        .filter(|e| e.event == "signals.sink_saved")
        .map(|e| e.detail.as_str())
        .collect();
    assert_eq!(audited.len(), 1, "{audited:?}");
    assert!(
        audited[0].starts_with("logs http://127.0.0.1:"),
        "{}",
        audited[0]
    );
    assert!(!audited[0].contains("sink-token"));

    // A second "new" under the name is a conflict; its own card with no token keeps the token.
    let again = sink(&hub, &owner, "new", &fields).send().await.unwrap();
    assert_eq!(again.status(), 409);
    fields[2].1 = "";
    let kept = sink(&hub, &owner, "logs", &fields).send().await.unwrap();
    assert_eq!(kept.status(), 303);
    assert_eq!(
        hub.accounts.stored_sinks().await.unwrap()[0].sealed_token,
        stored[0].sealed_token
    );
    assert_eq!(running(&hub).await, ["logs"], "restarted, not doubled");

    // Not the owner: nothing. Removed: it stops, and removing again finds nothing.
    let other = hub.first_connect("Claude").await;
    let denied = sink(&hub, &other.cookie, "logs/remove", &[])
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 404);
    let removed = sink(&hub, &owner, "logs/remove", &[]).send().await.unwrap();
    assert_eq!(removed.status(), 303);
    assert!(running(&hub).await.is_empty());
    assert!(hub.accounts.stored_sinks().await.unwrap().is_empty());
    let gone = sink(&hub, &owner, "logs/remove", &[]).send().await.unwrap();
    assert_eq!(gone.status(), 404);

    // A config file's sink is shown and its name cannot be taken here.
    let fresh = crate::store::testing::empty_postgres().await.unwrap();
    let configured_url = url.clone();
    let pinned = or_skip!(hosted_with(Some(&fresh), move |_, _| {
        json!({
            "operators": ["github:42"],
            "signals": { "sinks": [{ "name": "ops", "url": configured_url }] }
        })
    }));
    // The harness starts no sinks; as serve does, the config's runs first.
    let config_sink = crate::notify::signals::Sink::new(
        &pinned.config.signals.sinks[0],
        pinned.accounts.store().clone(),
        pinned.hub.audit.clone(),
        |_| None,
    )
    .unwrap();
    pinned
        .hub
        .boards
        .start_sink("ops", Arc::new(config_sink), true);
    let owner = operator_cookie(&pinned).await;
    let (_, page) = Hosted::text(pinned.get("/admin", Some(&owner)).await).await;
    assert!(page.contains("signals.sinks"), "{page}");
    let mut taken = vec![("name", "ops"), ("url", url.as_str()), ("enabled", "on")];
    taken.extend(ALL);
    let conflict = sink(&pinned, &owner, "new", &taken).send().await.unwrap();
    assert_eq!(conflict.status(), 409);
    assert_eq!(running(&pinned).await, ["ops"]);
}
