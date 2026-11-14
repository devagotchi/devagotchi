//! Connections over HTTP and MCP on a hosted hub, against a stand-in service: a person adds
//! one on `/app/connections` (checked first, sealed, never shown again), an agent lists it and
//! calls it with `connection_call`, the key reaches the service and never comes back, a call
//! cannot leave the service, another account cannot see or use it, and removing it ends it.
//! On SQLite, and on Postgres when the test server is set (CI).

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
};
use serde_json::{Value, json};

use crate::tenant_tests::{Hosted, hosted_with, refused};

const GOOD: &str = "re_good_key_123456";

/// What the stand-in service was asked: method, path and query, and the key it was sent.
#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<(String, String, String)>>>);

async fn service(
    State(seen): State<Seen>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: String,
) -> (StatusCode, [(&'static str, &'static str); 1], String) {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    seen.0
        .lock()
        .unwrap()
        .push((method.to_string(), uri.to_string(), auth.clone()));
    let json = [("content-type", "application/json")];
    if auth != format!("Bearer {GOOD}") {
        return (
            StatusCode::UNAUTHORIZED,
            json,
            json!({ "message": "API key is invalid" }).to_string(),
        );
    }
    // It echoes what it was sent, the key included, as a careless service might.
    (
        StatusCode::OK,
        json,
        json!({ "id": "email_1", "path": uri.to_string(), "auth": auth, "body": body }).to_string(),
    )
}

async fn stand_in() -> (String, Seen) {
    let seen = Seen::default();
    let app = Router::new().fallback(service).with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, seen)
}

async fn run(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let (url, seen) = stand_in().await;
    hub.hub
        .connections
        .as_ref()
        .expect("a hub with accounts holds connections")
        .send_everything_to(&url);
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Claude").await;

    // The page: every service in the catalog, nothing connected yet; the You page links it.
    let (status, page) = Hosted::text(hub.get("/app/connections", Some(&alice.cookie)).await).await;
    assert_eq!(status, 200, "{page}");
    for service in ["resend", "stripe", "github", "slack", "anthropic"] {
        assert!(
            page.contains(&format!("<option value=\"{service}\">")),
            "{service}"
        );
    }
    assert!(page.contains("Nothing connected yet"), "{page}");
    let (_, me) = Hosted::text(hub.get("/app/me", Some(&alice.cookie)).await).await;
    assert!(me.contains("href=\"/app/connections\""), "{me}");

    // A key the service refuses is not kept.
    let bad = hub
        .form(
            "/app/connections",
            Some(&alice.cookie),
            &[("service", "resend"), ("key", "re_wrong_key_000")],
        )
        .await;
    assert_eq!(bad.status(), 400);
    let (_, body) = Hosted::text(bad).await;
    assert!(
        body.contains("Resend (email) refused this key (401)"),
        "{body}"
    );
    assert!(!body.contains("re_wrong_key_000"));
    assert!(
        hub.accounts
            .connections(&tenant(&hub, &mut alice).await)
            .await
            .unwrap()
            .is_empty()
    );
    // Nor from another site, nor a service that is not in the catalog, nor a bad name.
    let cross = hub
        .http
        .post(format!("{}/app/connections", hub.url))
        .header(axum::http::header::COOKIE, &alice.cookie)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[("service", "resend"), ("key", GOOD)])
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);
    for form in [
        vec![("service", "evil"), ("key", GOOD)],
        vec![("service", "resend"), ("name", "Not A Name"), ("key", GOOD)],
    ] {
        let response = hub
            .form("/app/connections", Some(&alice.cookie), &form)
            .await;
        assert_eq!(response.status(), 400, "{form:?}");
    }

    // A good key: checked against the service, kept sealed, shown only as its last four.
    let added = hub
        .form(
            "/app/connections",
            Some(&alice.cookie),
            &[("service", "resend"), ("key", GOOD)],
        )
        .await;
    assert_eq!(added.status(), 303);
    let checked = seen.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        (checked.0.as_str(), checked.1.as_str()),
        ("GET", "/domains")
    );
    let stored = hub
        .accounts
        .connections(&tenant(&hub, &mut alice).await)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "resend");
    assert_eq!(stored[0].hint, "…3456");
    assert!(!stored[0].sealed_key.contains(GOOD));
    let (_, page) = Hosted::text(hub.get("/app/connections", Some(&alice.cookie)).await).await;
    assert!(
        page.contains("id=\"connection-resend\"") && page.contains("…3456"),
        "{page}"
    );
    assert!(!page.contains(GOOD), "the key is never shown again");

    // The agent sees the connection by name, never the key.
    let listed = hub.call(&mut alice, "connections", json!({})).await;
    assert!(!refused(&listed), "{listed}");
    assert_eq!(
        listed["structuredContent"]["connections"][0]["name"],
        "resend"
    );
    assert!(!listed.to_string().contains(GOOD));

    // And calls it: the key reaches the service, and is taken out of what comes back.
    let called = hub
        .call(
            &mut alice,
            "connection_call",
            json!({ "connection": "resend", "method": "post", "path": "/emails",
                    "query": { "dry run": "yes" },
                    "body": { "to": "jesse@example.com", "subject": "Hi" } }),
        )
        .await;
    assert!(!refused(&called), "{called}");
    let answer = &called["structuredContent"];
    assert_eq!(answer["status"], 200, "{answer}");
    assert_eq!(answer["body"]["auth"], "Bearer [redacted]");
    assert_eq!(answer["body"]["path"], "/emails?dry+run=yes");
    assert!(
        answer["body"]["body"]
            .as_str()
            .unwrap()
            .contains("jesse@example.com")
    );
    assert!(!called.to_string().contains(GOOD), "{called}");
    let sent = seen.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        sent,
        (
            "POST".into(),
            "/emails?dry+run=yes".into(),
            format!("Bearer {GOOD}")
        )
    );
    assert!(
        hub.accounts
            .connections(&tenant(&hub, &mut alice).await)
            .await
            .unwrap()[0]
            .last_used_at
            .is_some()
    );

    // A call cannot leave the service, nor use a method it does not have.
    let before = seen.0.lock().unwrap().len();
    for (method, path) in [
        ("GET", "https://evil.example/steal"),
        ("GET", "//evil.example/steal"),
        ("GET", "/../steal"),
        ("GET", "/x?key=1"),
        ("CONNECT", "/emails"),
    ] {
        let out = hub
            .call(
                &mut alice,
                "connection_call",
                json!({ "connection": "resend", "method": method, "path": path }),
            )
            .await;
        assert!(refused(&out), "{method} {path}: {out}");
    }
    assert_eq!(
        seen.0.lock().unwrap().len(),
        before,
        "none of those reached anything"
    );

    // Another account sees none of it and cannot use it.
    let theirs = hub.call(&mut bob, "connections", json!({})).await;
    assert_eq!(theirs["structuredContent"]["connections"], json!([]));
    let use_it = hub
        .call(
            &mut bob,
            "connection_call",
            json!({ "connection": "resend", "method": "GET", "path": "/domains" }),
        )
        .await;
    assert!(refused(&use_it), "{use_it}");

    // Removed: gone from the page and from the agent.
    let removed = hub
        .form("/app/connections/resend/remove", Some(&alice.cookie), &[])
        .await;
    assert_eq!(removed.status(), 303);
    let gone = hub
        .call(
            &mut alice,
            "connection_call",
            json!({ "connection": "resend", "method": "GET", "path": "/domains" }),
        )
        .await;
    assert!(refused(&gone), "{gone}");
    assert!(
        hub.accounts
            .connections(&tenant(&hub, &mut alice).await)
            .await
            .unwrap()
            .is_empty()
    );
}

/// The tenant a person's account is in.
async fn tenant(hub: &Hosted, person: &mut crate::tenant_tests::Person) -> String {
    let who: Value = hub.call(person, "whoami", json!({})).await;
    let subject = who["structuredContent"]["subject"]
        .as_str()
        .unwrap()
        .to_string();
    let store = hub.accounts.store();
    let mut connection = store.read().await.unwrap();
    store
        .tenant_of(&mut connection, crate::store::Lookup::Account(&subject))
        .await
        .unwrap()
        .expect("an account has a tenant")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_connection_is_called_through_the_hub_and_its_key_never_leaves_it() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}
