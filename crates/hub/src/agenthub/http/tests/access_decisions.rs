use super::*;
use super::{
    access_reads::{requested, token},
    principals::Admin,
};
use crate::identity::accounts::api_keys::Scope;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_http_narrows_and_replays_the_same_grant_without_widening() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let path = format!("/api/v1/access-requests/{}/approve", group.request_ids[0]);
    assert_eq!(
        response(
            admin
                .request(Method::POST, &path)
                .header("idempotency-key", "wider")
                .json(&json!({"version":1,"duration":"8h"}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let body = json!({"version":1,"duration":"once","uses_max":1,"note":"Only the requested inspection.",
        "item":{"capability":"exec","sandbox_level":"read-only","selector":{"paths":["/work"],"argv_prefix":[["git","status"]]}}});
    let send = || {
        admin
            .request(Method::POST, &path)
            .header("idempotency-key", "approval")
            .json(&body)
    };
    let (status, first, replayed) = response(send()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert!(!replayed);
    assert_eq!(first["result"]["status"], "narrowed");
    assert_eq!(first["result"]["granted"]["duration"], "once");
    let (status, again, replayed) = response(send()).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert!(replayed);
    assert_eq!(first["result"]["grant_id"], again["result"]["grant_id"]);
    assert_eq!(
        response(
            admin
                .request(Method::POST, &path)
                .header("idempotency-key", "stale")
                .json(&body)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, lookup, _) = response(admin.request(
        Method::GET,
        &format!("/api/v1/access-requests/{}", group.group_id),
    ))
    .await;
    assert_eq!(lookup["items"][0]["grant_id"], first["result"]["grant_id"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_http_refuses_agents_cross_origin_seats_and_unknown_fields() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let (_, token) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    for action in ["approve", "deny"] {
        let path = format!("/api/v1/access-requests/{}/{action}", group.request_ids[0]);
        assert_eq!(
            response(
                admin
                    .hub
                    .http
                    .post(format!("{}{path}", admin.hub.url))
                    .bearer_auth(&token)
                    .header("idempotency-key", "agent")
                    .json(&json!({"version":1,"note":"No"}))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            admin
                .hub
                .http
                .post(format!("{}{path}", admin.hub.url))
                .header("cookie", &admin.cookie)
                .header("origin", "https://other.invalid")
                .header("idempotency-key", "origin")
                .json(&json!({"version":1,"note":"No"}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            response(
                admin
                    .request(Method::POST, &path)
                    .header(crate::agenthub::mcp::SEAT_HEADER, "worker")
                    .header("idempotency-key", "seat")
                    .json(&json!({"version":1,"note":"No"}))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            response(
                admin
                    .request(Method::POST, &path)
                    .header("idempotency-key", "unknown")
                    .json(&json!({"version":1,"note":"No","force":true}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let (_, lookup, _) = response(admin.request(
        Method::GET,
        &format!("/api/v1/access-requests/{}", group.group_id),
    ))
    .await;
    assert_eq!(lookup["status"], "pending");
    // Make a human request possible explicitly; default access.request permits agents.
    // That policy must still never let the requester decide their own request.
    let (status, policy, _) = response(admin.request(Method::POST, "/api/v1/policies").json(
        &json!({
            "id":"human-request", "name":"Human request fixture", "subject":"human",
            "action":"access.request", "selector":{}, "effect":"allow",
            "priority":40, "enabled":true, "version":0
        }),
    ))
    .await;
    assert_eq!(status, StatusCode::OK, "{policy}");
    let access = crate::machines::access::Access::new(admin.hub.accounts.clone());
    let machine = access
        .machine_administration(&admin.owner, "lab", None)
        .await
        .unwrap()
        .binding;
    let actor = devagotchi_agenthub::Actor {
        id: &admin.owner,
        human: true,
        wip_limit: None,
        policy: None,
    };
    let input=serde_json::from_value(json!({"machine":"lab","selection":{"kind":"items","items":[{"capability":"exec","sandbox_level":"read-only","selector":{}}]},"duration":"1h","reason":"Inspect this repository for the requested change."})).unwrap();
    let own = access
        .request(
            &admin.owner,
            &actor,
            None,
            &machine,
            &crate::catalog::access::MachineFacts {
                os: "linux",
                display_server: "x11",
                kind: "native",
            },
            &input,
            "own-request",
        )
        .await
        .unwrap();
    for action in ["approve", "deny"] {
        let path = format!(
            "/api/v1/access-requests/{}/{action}",
            own.result.request_ids[0]
        );
        let (status, body, _) = response(
            admin
                .request(Method::POST, &path)
                .header("idempotency-key", format!("own-{action}"))
                .json(&json!({"version":1,"note":"No"})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["error"]["code"], "own_request");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_http_concurrent_approval_and_denial_commit_one_decision() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let base = format!("/api/v1/access-requests/{}", group.request_ids[0]);
    let a = admin
        .request(Method::POST, &format!("{base}/approve"))
        .header("idempotency-key", "approve")
        .json(&json!({"version":1}));
    let b = admin
        .request(Method::POST, &format!("{base}/deny"))
        .header("idempotency-key", "deny")
        .json(&json!({"version":1,"note":"Not needed."}));
    let (a, b) = tokio::join!(response(a), response(b));
    assert_eq!(
        [a.0, b.0]
            .into_iter()
            .filter(|s| *s == StatusCode::OK)
            .count(),
        1,
        "{a:?} {b:?}"
    );
    assert_eq!(
        [a.0, b.0]
            .into_iter()
            .filter(|s| *s == StatusCode::CONFLICT)
            .count(),
        1,
        "{a:?} {b:?}"
    );
    let (_, lookup, _) = response(admin.request(Method::GET, &base)).await;
    let status = lookup["status"].as_str().unwrap();
    assert!(["approved", "denied"].contains(&status), "{lookup}");
}
