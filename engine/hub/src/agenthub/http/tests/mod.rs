//! Real HTTP and MCP share PostgreSQL state and live authority, not separate stores.
mod access_admin;
mod access_browser;
mod access_control;
mod access_decisions;
mod access_execution;
mod access_files;
mod access_heartbeat;
mod access_mcp;
mod access_reads;
mod access_requests;
mod access_run;
mod access_selection;
mod aggregate;
mod catalog;
mod catalog_access;
mod catalog_defaults;
mod configuration;
mod events;
mod machine_access;
mod principals;
mod project_races;
mod projects;
mod protocol;
mod selection;
mod stream;
mod workflow;
use crate::agenthub::tests::serving::{hub_with, tool};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

struct World {
    url: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

async fn world() -> Option<World> {
    let dir = tempfile::tempdir().unwrap();
    let url = hub_with(
        dir.path(),
        json!({}),
        &[("tok-other", "300000000000000003")],
    )
    .await?;
    let bind = tool(&url, "tok-member", "board_bind", json!({"project":"demo"})).await;
    assert_ne!(bind["isError"], true, "{bind}");
    Some(World {
        url,
        http: reqwest::Client::new(),
        _dir: dir,
    })
}

async fn response(request: reqwest::RequestBuilder) -> (StatusCode, Value, bool) {
    let response = request.send().await.unwrap();
    let status = response.status();
    let replayed = response
        .headers()
        .get("idempotent-replayed")
        .is_some_and(|value| value == "true");
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("{status}: {text}")),
        replayed,
    )
}

impl World {
    fn request(&self, method: Method, path: &str, token: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.url))
            .bearer_auth(token)
    }
    async fn write(&self, path: &str, body: Value) -> Value {
        let (status, value, _) =
            response(self.request(Method::POST, path, "tok-member").json(&body)).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        value
    }
    async fn get(&self, path: &str) -> Value {
        let (status, value, _) = response(self.request(Method::GET, path, "tok-member")).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        value
    }
    async fn entries(&self) -> u64 {
        let result = tool(
            &self.url,
            "tok-member",
            "board_verify",
            json!({"project":"demo"}),
        )
        .await;
        result["structuredContent"]["entries"].as_u64().unwrap()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_create_replays_across_transports_and_scopes_reads() {
    let world = or_skip!(world());
    let baseline = world.entries().await;
    let body = json!({"project":"demo","title":"receiptquartz","request_id":"http-one"});
    let create = || {
        world
            .request(Method::POST, "/api/v1/tasks", "tok-member")
            .json(&body)
    };
    let (left, right) = tokio::join!(response(create()), response(create()));
    assert_eq!(left.0, StatusCode::OK, "{}", left.1);
    assert_eq!(right.0, StatusCode::OK, "{}", right.1);
    assert_ne!(left.2, right.2);
    assert_eq!(left.1["id"], right.1["id"]);
    assert_eq!(world.entries().await, baseline + 1);
    let mcp = tool(&world.url, "tok-member", "board_create", body.clone()).await;
    assert_eq!(mcp["structuredContent"]["replayed"], true, "{mcp}");
    assert_eq!(mcp["structuredContent"]["id"], left.1["id"]);
    let id = left.1["id"].as_str().unwrap();
    let detail = world.get(&format!("/api/v1/tasks/{id}?project=demo")).await;
    assert_eq!(detail["task"]["id"], id);
    assert_eq!(
        world
            .get("/api/v1/tasks?project=demo&q=receiptquartz")
            .await["tasks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for body in [
        json!({"project":"demo","title":"Different","request_id":"http-one"}),
        json!({"project":"demo","title":"One","request_id":"conflicting-body-key"}),
    ] {
        let (status, error, _) = response(
            world
                .request(Method::POST, "/api/v1/tasks", "tok-member")
                .header("idempotency-key", "http-one")
                .json(&body),
        )
        .await;
        assert!(
            matches!(status, StatusCode::CONFLICT | StatusCode::BAD_REQUEST),
            "{error}"
        );
    }
    let (status, error, _) = response(
        world
            .request(
                Method::POST,
                &format!("/api/v1/tasks/{id}/claim"),
                "tok-member",
            )
            .json(&json!({"project":"demo","request_id":"http-one"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    assert_eq!(error["error"]["code"], "request_conflict");
    for method in [Method::GET, Method::PATCH] {
        let mut request = world.request(
            method.clone(),
            &format!("/api/v1/tasks/{id}?project=demo"),
            "tok-other",
        );
        if method == Method::PATCH {
            request = request.json(&json!({"version":1,"title":"stolen"}));
        }
        let (status, _, _) = response(request).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    assert_eq!(world.entries().await, baseline + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_lease_replays_preserve_newer_claim_and_distinguish_progress_null() {
    let world = or_skip!(world());
    let made = world
        .write("/api/v1/tasks", json!({"project":"demo","title":"Lease"}))
        .await;
    let id = made["id"].as_str().unwrap();
    let claim_path = format!("/api/v1/tasks/{id}/claim?project=demo");
    let release_path = format!("/api/v1/tasks/{id}/release?project=demo");
    let first = world
        .write(
            &claim_path,
            json!({"ttl_seconds":120,"request_id":"claim-old"}),
        )
        .await;
    let release = json!({"fence":first["fence"],"reason":"pause","request_id":"release-old"});
    world.write(&release_path, release.clone()).await;
    let newer = world
        .write(&claim_path, json!({"request_id":"claim-new"}))
        .await;
    assert!(newer["fence"].as_i64().unwrap() > first["fence"].as_i64().unwrap());
    let before = world.entries().await;
    assert_eq!(world.write(&release_path, release).await["replayed"], true);
    let old = world
        .write(
            &claim_path,
            json!({"ttl_ms":120000,"request_id":"claim-old"}),
        )
        .await;
    assert_eq!(old["fence"], first["fence"]);
    assert_eq!(old["lease_expires_at"], first["lease_expires_at"]);
    assert_eq!(old["replayed"], true);
    assert_eq!(world.entries().await, before);
    assert_eq!(
        world.get(&format!("/api/v1/tasks/{id}?project=demo")).await["task"],
        newer["ticket"]
    );
    let beat_path = format!("/api/v1/tasks/{id}/heartbeat?project=demo");
    world
        .write(
            &beat_path,
            json!({"fence":newer["fence"],"progress":{"step":4}}),
        )
        .await;
    let omitted = json!({"fence":newer["fence"],"request_id":"keep-progress"});
    assert_eq!(
        world.write(&beat_path, omitted.clone()).await["ticket"]["progress"]["step"],
        4
    );
    world
        .write(&beat_path, json!({"fence":newer["fence"],"progress":null}))
        .await;
    assert_eq!(world.write(&beat_path, omitted).await["replayed"], true);
    assert!(
        world.get(&format!("/api/v1/tasks/{id}?project=demo")).await["task"]["progress"].is_null()
    );
    let (status, error, _) = response(
        world
            .request(Method::POST, &beat_path, "tok-member")
            .json(&json!({"fence":newer["fence"],"progress":null,"request_id":"keep-progress"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_rejects_malformed_oversized_and_conflicting_preconditions_without_mutating() {
    let world = or_skip!(world());
    let made = world
        .write("/api/v1/tasks", json!({"project":"demo","title":"Guarded"}))
        .await;
    let path = format!(
        "/api/v1/tasks/{}?project=demo",
        made["id"].as_str().unwrap()
    );
    let before = world.entries().await;
    for (body, tag, expected) in [
        (json!({"title":"bad"}), "W/\"1\"", StatusCode::BAD_REQUEST),
        (
            json!({"title":"bad","version":2}),
            "\"1\"",
            StatusCode::PRECONDITION_FAILED,
        ),
        (
            json!({"title":"bad","unknown":true}),
            "\"1\"",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, error, _) = response(
            world
                .request(Method::PATCH, &path, "tok-member")
                .header("if-match", tag)
                .json(&body),
        )
        .await;
        assert_eq!(status, expected, "{error}");
    }
    let (status, _, _) = response(
        world
            .request(Method::POST, "/api/v1/tasks", "tok-member")
            .header("content-type", "application/json")
            .body("{bad"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = response(
        world
            .request(Method::POST, "/api/v1/tasks", "tok-member")
            .json(&json!({"project":"demo","title":"large","description":"x".repeat(1024*1024)})),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(world.entries().await, before);
    let (status, changed, _) = response(
        world
            .request(Method::PATCH, &path, "tok-member")
            .header("if-match", "\"1\"")
            .header("idempotency-key", "edit-once")
            .json(&json!({"title":"Changed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    let (status, replay, header) = response(
        world
            .request(Method::PATCH, &path, "tok-member")
            .header("if-match", "\"1\"")
            .header("idempotency-key", "edit-once")
            .json(&json!({"title":"Changed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert!(header);
    assert_eq!(replay["version"], changed["version"]);
    assert_eq!(world.entries().await, before + 1);
    let (status, error, _) = response(
        world
            .request(Method::PATCH, &path, "tok-member")
            .header("if-match", "\"1\"")
            .json(&json!({"title":"stale"})),
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{error}");
    assert_eq!(world.entries().await, before + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_current_key_limits_revocation_and_browser_origin_hold_on_replay() {
    use crate::identity::accounts::api_keys::{NewKey, Scope};
    let (url, accounts) = or_skip!(crate::agenthub::tests::cli_retries::local_accounts_hub());
    let owner = accounts
        .ensure_owner(None, "Owner", [53; 16])
        .await
        .unwrap();
    let key = |scopes, projects| NewKey {
        name: "HTTP test".into(),
        hours: 24,
        scopes,
        projects,
        ..Default::default()
    };
    let (writer, secret) = accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &key(vec![Scope::BoardsRead, Scope::BoardsWrite], vec![]),
        )
        .await
        .unwrap();
    let (_, reader) = accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &key(vec![Scope::BoardsRead], vec!["demo".into()]),
        )
        .await
        .unwrap();
    let (_, restricted) = accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &key(
                vec![Scope::BoardsRead, Scope::BoardsWrite],
                vec!["another".into()],
            ),
        )
        .await
        .unwrap();
    let bound = tool(&url, &secret, "board_bind", json!({"project":"demo"})).await;
    assert_ne!(bound["isError"], true, "{bound}");
    let http = reqwest::Client::new();
    let body = json!({"project":"demo","title":"Authorized","request_id":"private-result"});
    let write = |token: &str| {
        http.post(format!("{url}/api/v1/tasks"))
            .bearer_auth(token)
            .json(&body)
    };
    let (status, made, _) = response(write(&secret)).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert!(response(write(&secret)).await.2);
    for token in [&reader, &restricted] {
        let (status, error, _) = response(write(token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{error}");
        assert_eq!(error["error"]["code"], "key_denied");
        assert!(error.get("id").is_none(), "no saved result is disclosed");
    }
    let (status, _, _) = response(
        http.get(format!("{url}/api/v1/tasks?project=demo"))
            .bearer_auth(&restricted),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, listed, _) = response(
        http.get(format!("{url}/api/v1/tasks?project=demo"))
            .bearer_auth(&reader),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|task| task["id"] == made["id"])
            .count(),
        1
    );
    accounts
        .revoke_key(&owner.tenant, &owner.id, &writer.id)
        .await
        .unwrap();
    assert_eq!(
        write(&secret).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let session = accounts.open_session(&owner.id, None).await.unwrap();
    let browser_body = json!({"project":"demo","title":"Browser","request_id":"browser-once"});
    let browser = || {
        http.post(format!("{url}/api/v1/tasks"))
            .header("cookie", format!("dg_session={session}"))
            .json(&browser_body)
    };
    assert_eq!(
        browser()
            .header("origin", "https://foreign.test")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        browser().send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let (status, made, _) = response(browser().header("origin", &url)).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert!(response(browser().header("origin", &url)).await.2);
    assert_eq!(
        browser()
            .header("origin", "https://foreign.test")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    accounts.end_session(&session).await.unwrap();
    assert_eq!(
        browser()
            .header("origin", &url)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
