//! API keys end to end over HTTP, the whole router as it serves: a person makes one on the
//! page and sees it once; it works on the MCP endpoint and nowhere else; it does only what it
//! was given; it can never approve; and ending it stops it at once. On SQLite, and on Postgres
//! when the test server is set (CI).

use serde_json::{Value, json};

use crate::{
    accounts::api_keys,
    tenant_tests::{Hosted, Person, hosted_with, refused},
};

fn form(
    hub: &Hosted,
    path: &str,
    person: &Person,
    fields: &[(&str, &str)],
) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}{path}", hub.url))
        .header(axum::http::header::COOKIE, &person.cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

fn secret_in(html: &str) -> String {
    let start = html.find("dgk_").expect("the key is on the page");
    html[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .collect()
}

fn as_key(secret: &str) -> Person {
    Person {
        cookie: String::new(),
        token: secret.to_string(),
        session: None,
    }
}

fn why(answer: &Value) -> String {
    answer["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

async fn run(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let mut person = hub.first_connect("Claude").await;
    for project in ["site", "other"] {
        hub.call(&mut person, "board_bind", json!({ "project": project }))
            .await;
    }

    // Made on the page: limits chosen, the key shown this once.
    let made = form(
        &hub,
        "/app/keys",
        &person,
        &[
            ("name", "hermes"),
            ("hours", "24"),
            ("scope", "boards.read"),
            ("scope", "approvals.request"),
            ("project", "site"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(made.status(), 200);
    assert_eq!(made.headers()["cache-control"], "no-store");
    let html = made.text().await.unwrap();
    assert!(html.contains("Copy this key now"), "{html}");
    let secret = secret_in(&html);
    assert_eq!(secret.len(), 47, "{secret}");

    // Listed afterwards without the key.
    let (status, page) = Hosted::text(hub.get("/app/keys", Some(&person.cookie)).await).await;
    assert_eq!(status, 200);
    assert!(page.contains("hermes"), "{page}");
    assert!(page.contains("boards.read"), "{page}");
    assert!(!page.contains(&secret), "shown once: {page}");

    // It works, and only where and how it was told.
    let mut key = as_key(&secret);
    let allowed = hub
        .call(&mut key, "board_list", json!({ "project": "site" }))
        .await;
    assert!(!refused(&allowed), "{allowed}");
    let other = hub
        .call(&mut key, "board_list", json!({ "project": "other" }))
        .await;
    assert!(refused(&other), "{other}");
    assert!(why(&other).contains("not allowed on project"), "{other}");
    let write = hub
        .call(
            &mut key,
            "board_create",
            json!({ "project": "site", "title": "Should not exist" }),
        )
        .await;
    assert!(refused(&write), "{write}");
    assert!(why(&write).contains("boards.write"), "{write}");
    let run = hub
        .call(&mut key, "run", json!({ "machine": "x", "command": "id" }))
        .await;
    assert!(refused(&run), "{run}");

    // It asks for an approval as an AI, and the person decides; it can never be the person.
    let asked = hub
        .call(
            &mut key,
            "approval_request",
            json!({ "project": "site", "title": "Restart", "what": "restart", "why": "b", "undo": "c" }),
        )
        .await;
    assert_eq!(asked["structuredContent"]["status"], "pending", "{asked}");
    assert_eq!(asked["structuredContent"]["requested_by"], "ai", "{asked}");

    // The MCP endpoint is all it reaches.
    let me = hub
        .http
        .get(format!("{}/api/v1/me", hub.url))
        .bearer_auth(&secret)
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 401, "a key is not a sign-in to the API");
    let pages = hub
        .http
        .get(format!("{}/app/keys", hub.url))
        .bearer_auth(&secret)
        .send()
        .await
        .unwrap();
    assert_eq!(pages.status(), 303, "a key cannot manage keys");

    // Another site's form changes nothing.
    let cross = hub
        .http
        .post(format!("{}/app/keys", hub.url))
        .header(axum::http::header::COOKIE, &person.cookie)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[("name", "x"), ("hours", "24"), ("scope", "boards.read")])
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);

    // Bad choices are refused and say why.
    for fields in [
        vec![("name", "x"), ("hours", "24")],
        vec![("name", "x"), ("hours", "5"), ("scope", "boards.read")],
        vec![("name", "x"), ("hours", "24"), ("scope", "everything")],
    ] {
        let answer = form(&hub, "/app/keys", &person, &fields)
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 400, "{fields:?}");
    }

    // Revoked from the page: the very next call is refused.
    let id = {
        let start = page.find("id=\"key_").unwrap() + 4;
        page[start..].split('"').next().unwrap().to_string()
    };
    let ended = form(&hub, &format!("/app/keys/{id}/revoke"), &person, &[])
        .send()
        .await
        .unwrap();
    assert_eq!(ended.status(), 303);
    let gone = hub
        .http
        .post(format!("{}/mcp", hub.url))
        .bearer_auth(&secret)
        .header(
            axum::http::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_key_does_what_it_was_made_to_do_and_stops_when_it_is_ended() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}

/// A tool nobody assigned a scope is closed to keys by default; this makes that a decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_tool_is_either_scoped_or_deliberately_closed_to_keys() {
    let hub = hosted_with(None, |_, _| json!({})).await;
    let mut person = hub.first_connect("Claude").await;
    let listed = hub.mcp(&mut person, "tools/list", json!({})).await;
    let tools = listed["tools"].as_array().expect("tools");
    assert!(tools.len() > 30, "{listed}");
    const CLOSED: [&str; 2] = ["offers", "offers_accept"];
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            api_keys::scope_for(name).is_some()
                || api_keys::ALWAYS.contains(&name)
                || CLOSED.contains(&name),
            "{name} has no API key scope"
        );
    }
}
