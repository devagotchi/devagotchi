use super::*;
use std::time::Duration;

struct Stream {
    response: reqwest::Response,
    buffered: Vec<u8>,
}

impl Stream {
    async fn open(request: reqwest::RequestBuilder) -> Self {
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-accel-buffering"], "no");
        let mut stream = Self {
            response,
            buffered: Vec::new(),
        };
        assert!(stream.frame().await.unwrap().contains("retry: 3000"));
        stream
    }

    async fn frame(&mut self) -> Option<String> {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let Some(end) = self.buffered.windows(2).position(|w| w == b"\n\n") {
                    let frame: Vec<_> = self.buffered.drain(..end + 2).collect();
                    return Some(String::from_utf8(frame).unwrap());
                }
                let Some(bytes) = self.response.chunk().await.unwrap() else {
                    assert!(self.buffered.is_empty(), "truncated SSE frame");
                    return None;
                };
                self.buffered.extend(bytes);
                assert!(self.buffered.len() < 2 * 1024 * 1024);
            }
        })
        .await
        .expect("stream did not answer within its recheck window")
    }

    async fn event(&mut self) -> Value {
        loop {
            let frame = self.frame().await.expect("stream ended before its event");
            if frame.starts_with(':') {
                continue;
            }
            assert!(frame.lines().any(|line| line == "event: hub"), "{frame}");
            let id = frame
                .lines()
                .find_map(|line| line.strip_prefix("id: "))
                .unwrap()
                .parse::<i64>()
                .unwrap();
            let data = frame
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            let event: Value = serde_json::from_str(data).unwrap();
            assert_eq!(event["seq"], id);
            return event;
        }
    }

    async fn ended(&mut self) {
        assert!(
            self.frame().await.is_none(),
            "revoked stream disclosed another frame"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_catches_up_resumes_and_observes_live_history_without_replay_events() {
    let world = or_skip!(world());
    let before = world.get("/api/v1/events?project=demo").await["head_seq"]
        .as_i64()
        .unwrap();
    // Cross the server's 100-event page boundary, rather than only testing one page.
    for index in 0..102 {
        world
            .write(
                "/api/v1/tasks",
                json!({"project":"demo","title":format!("Stream {index}"),
            "request_id":format!("stream-{index}")}),
            )
            .await;
    }
    let expected = world
        .get(&format!(
            "/api/v1/events?project=demo&after_seq={before}&limit=200"
        ))
        .await;
    let mut stream = Stream::open(world.request(
        Method::GET,
        &format!("/api/v1/stream?project=demo&after_seq={before}"),
        "tok-member",
    ))
    .await;
    for event in expected["events"].as_array().unwrap() {
        assert_eq!(&stream.event().await, event);
    }
    let cursor = expected["head_seq"].as_i64().unwrap();
    drop(stream);
    let mut resumed = Stream::open(
        world
            .request(Method::GET, "/api/v1/stream?project=demo", "tok-member")
            .header("Last-Event-ID", cursor),
    )
    .await;
    let mut live = Stream::open(world.request(
        Method::GET,
        "/api/v1/stream?project=demo&from=now",
        "tok-member",
    ))
    .await;
    let original = json!({"project":"demo","title":"Stream 0","request_id":"stream-0"});
    assert!(
        world.write("/api/v1/tasks", original).await["replayed"]
            .as_bool()
            .unwrap()
    );
    let made = world
        .write(
            "/api/v1/tasks",
            json!({"project":"demo","title":"Live after replay"}),
        )
        .await;
    let (a, b) = tokio::join!(resumed.event(), live.event());
    assert_eq!(a, b);
    assert_eq!(a["ticket"], made["id"]);
    assert!(a["seq"].as_i64().unwrap() > cursor);
    drop((resumed, live));
    // Dropping open bodies must not hold the board or prevent later reads/writes.
    assert!(
        world.get("/api/v1/events/verify?project=demo").await["ok"]
            .as_bool()
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_rejects_ambiguous_cursors_and_foreign_boards_before_opening() {
    let world = or_skip!(world());
    for path in [
        "/api/v1/stream",
        "/api/v1/stream?project=demo&after_seq=-1",
        "/api/v1/stream?project=demo&from=later",
        "/api/v1/stream?project=demo&from=now&after_seq=0",
        "/api/v1/stream?project=demo&after_seq=9223372036854775807",
        "/api/v1/stream?project=demo&unknown=1",
    ] {
        let (status, _, _) = response(world.request(Method::GET, path, "tok-member")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
    for cursor in ["invalid", "-1", "", "1,2"] {
        let (status, _, _) = response(
            world
                .request(Method::GET, "/api/v1/stream?project=demo", "tok-member")
                .header("Last-Event-ID", cursor),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    for request in [
        world
            .request(
                Method::GET,
                "/api/v1/stream?project=demo&after_seq=0",
                "tok-member",
            )
            .header("Last-Event-ID", "1"),
        world
            .request(Method::GET, "/api/v1/stream?project=demo", "tok-member")
            .header("Last-Event-ID", "0")
            .header("Last-Event-ID", "0"),
        world
            .request(Method::GET, "/api/v1/stream?project=demo", "tok-member")
            .header("Idempotency-Key", "read-is-not-write"),
    ] {
        assert_eq!(response(request).await.0, StatusCode::BAD_REQUEST);
    }
    let (status, _, _) =
        response(world.request(Method::GET, "/api/v1/stream?project=demo", "tok-other")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_streams_recheck_api_key_and_browser_session_revocation() {
    use crate::identity::accounts::api_keys::{NewKey, Scope};
    let (url, accounts) = or_skip!(crate::agenthub::tests::cli_retries::local_accounts_hub());
    let owner = accounts
        .ensure_owner(None, "Owner", [54; 16])
        .await
        .unwrap();
    let new = NewKey {
        name: "Stream".into(),
        hours: 24,
        scopes: vec![Scope::BoardsRead, Scope::BoardsWrite],
        ..Default::default()
    };
    let (key, secret) = accounts
        .create_key(&owner.tenant, &owner.id, &new)
        .await
        .unwrap();
    let bind = tool(&url, &secret, "board_bind", json!({"project":"demo"})).await;
    assert_ne!(bind["isError"], true, "{bind}");
    let http = reqwest::Client::new();
    let endpoint = format!("{url}/api/v1/stream?project=demo&from=now");
    for limited in [
        NewKey {
            projects: vec!["elsewhere".into()],
            ..new.clone()
        },
        NewKey {
            scopes: vec![Scope::MachinesRead],
            ..new.clone()
        },
    ] {
        let (_, token) = accounts
            .create_key(&owner.tenant, &owner.id, &limited)
            .await
            .unwrap();
        assert_eq!(
            response(http.get(&endpoint).bearer_auth(token)).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let session = accounts.open_session(&owner.id, None).await.unwrap();
    let browser = || {
        http.get(&endpoint)
            .header("cookie", format!("dg_session={session}"))
    };
    assert_eq!(
        response(browser().header("origin", "https://foreign.test"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let mut keyed = Stream::open(http.get(&endpoint).bearer_auth(&secret)).await;
    let mut browser = Stream::open(browser().header("origin", &url)).await;
    accounts
        .revoke_key(&owner.tenant, &owner.id, &key.id)
        .await
        .unwrap();
    accounts.end_session(&session).await.unwrap();
    tokio::join!(keyed.ended(), browser.ended());
    assert_eq!(
        http.get(&endpoint)
            .bearer_auth(secret)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_stops_when_its_registered_agent_is_disabled() {
    use crate::identity::accounts::{
        agents::AgentEdit,
        api_keys::{NewKey, Scope},
    };
    use crate::tenancy::tests::tenants::{Person, hosted_with, refused};
    let hub = or_skip!(hosted_with(
        None,
        |_, _| json!({"tenancy":{"mode":"single"}})
    ));
    let owner = hub
        .accounts
        .ensure_owner(None, "Owner", [78; 16])
        .await
        .unwrap();
    hub.config.learn_owner(&owner.id).unwrap();
    let mut edit = AgentEdit {
        id: "stream-worker".into(),
        name: "Stream worker".into(),
        kind: "codex".into(),
        roles: vec!["worker".into()],
        enabled: true,
        wip_limit: None,
        version: 0,
    };
    let agent = hub
        .accounts
        .put_agent(&owner.id, &owner.id, &edit)
        .await
        .unwrap();
    let new = NewKey {
        name: "Stream agent".into(),
        scopes: Scope::ALL.to_vec(),
        hours: 24,
        ..Default::default()
    };
    let (_, secret) = hub
        .accounts
        .create_key(&owner.tenant, &owner.id, &new)
        .await
        .unwrap();
    let mut setup = Person {
        cookie: String::new(),
        token: secret,
        session: None,
    };
    assert!(!refused(
        &hub.call(&mut setup, "board_bind", json!({"project":"demo"}))
            .await
    ));
    let (_, token) = hub
        .accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &NewKey {
                agent_id: Some(agent.id),
                ..new
            },
        )
        .await
        .unwrap();
    let mut stream = Stream::open(
        reqwest::Client::new()
            .get(format!("{}/api/v1/stream?project=demo&from=now", hub.url))
            .bearer_auth(token),
    )
    .await;
    edit.version = agent.version;
    edit.enabled = false;
    hub.accounts
        .put_agent(&owner.id, &owner.id, &edit)
        .await
        .unwrap();
    stream.ended().await;
}
