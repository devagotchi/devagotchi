use super::*;
use super::{
    access_reads::{requested, token},
    principals::Admin,
};
use crate::{
    identity::accounts::api_keys::Scope,
    machines::access::{Access, AccessPageQuery},
};
use devagotchi_agenthub::Actor;

async fn html(request: reqwest::RequestBuilder) -> (StatusCode, String) {
    let response = request.send().await.unwrap();
    let status = response.status();
    (status, response.text().await.unwrap())
}
fn form(key: &str) -> Vec<(&str, &str)> {
    vec![
        ("request_id", key),
        ("version", "1"),
        ("action", "narrow"),
        ("level", "read-only"),
        ("duration", "once"),
        ("paths_mode", "restrict"),
        ("paths", "/work"),
        ("note", "Only this inspection <script>unsafe</script>"),
    ]
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_browser_review_narrow_replay_revoke_and_history_use_one_domain() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let path = format!("/app/access/requests/{}", group.request_ids[0]);
    for entry in ["/app", "/approvals"] {
        let (status, body) = html(admin.request(Method::GET, entry)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            body.contains("1 machine access requests need your review"),
            "{entry}"
        );
    }
    let attention = admin
        .request(Method::GET, "/app/attention")
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(attention["approvals"], 1);
    let (status, body) = html(admin.request(Method::GET, &path)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("Worker"));
    assert!(body.contains("Inspect the source repository"));
    assert!(body.contains("name=\"request_id\""));
    let send = || {
        admin
            .request(Method::POST, &path)
            .form(&form("browser-decision"))
    };
    let first = send().send().await.unwrap();
    assert_eq!(first.status(), StatusCode::SEE_OTHER);
    let again = send().send().await.unwrap();
    assert_eq!(again.status(), StatusCode::SEE_OTHER);
    assert_eq!(again.headers()["idempotent-replayed"], "true");
    let (status, lookup, _) = response(admin.request(
        Method::GET,
        &format!("/api/v1/access-requests/{}", group.group_id),
    ))
    .await;
    assert_eq!(status, StatusCode::OK, "{lookup}");
    assert_eq!(
        lookup["items"][0]["granted"]["item"]["selector"]["paths"],
        json!(["/work"])
    );
    assert_eq!(lookup["items"][0]["granted"]["duration"], "once");
    let (_, history) = html(admin.request(Method::GET, &path)).await;
    assert!(history.contains("Approved access"));
    assert!(history.contains("/work"));
    assert!(history.contains("&lt;script&gt;unsafe&lt;/script&gt;"));
    assert!(!history.contains("<script>unsafe</script>"));
    let grant = lookup["items"][0]["grant_id"].as_str().unwrap();
    let (_, live) = html(admin.request(Method::GET, "/app/access/grants")).await;
    assert!(live.contains("Worker"));
    assert!(live.contains("Run commands"));
    assert!(live.contains(grant));
    let (_, ended) = html(admin.request(Method::GET, "/app/access/grants?ended=true")).await;
    assert!(!ended.contains(grant));
    let revoke = format!("/app/access/grants/{grant}/revoke");
    let fields = [
        ("request_id", "browser-revoke"),
        ("version", "1"),
        ("action", "revoke"),
    ];
    for replay in [false, true] {
        let response = admin
            .request(Method::POST, &revoke)
            .form(&fields)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get("idempotent-replayed").is_some(),
            replay
        );
    }
    let (_, ended) = html(admin.request(Method::GET, "/app/access/grants?ended=true")).await;
    assert!(ended.contains("Revoked"));
    assert!(!ended.contains("value=\"revoke\""));
    let query = AccessPageQuery {
        ended: true,
        limit: Some(1),
        ..Default::default()
    };
    let rows = Access::new(admin.hub.accounts.clone())
        .access_grants(&admin.owner, &Actor::human(&admin.owner), &query)
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0]["id"], grant);
    assert!(rows.next.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_browser_refusal_retains_scope_note_key_and_version_without_a_grant() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let path = format!("/app/access/requests/{}", group.request_ids[0]);
    let mut fields = form("browser-invalid");
    fields.iter_mut().find(|(k, _)| *k == "duration").unwrap().1 = "8h";
    let (status, body) = html(admin.request(Method::POST, &path).form(&fields)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    for value in [
        "value=\"browser-invalid\"",
        "value=\"8h\" selected",
        "value=\"1\"",
        "/work",
        "Only this inspection &lt;script&gt;unsafe&lt;/script&gt;",
    ] {
        assert!(body.contains(value), "{value}");
    }
    let mut stale = form("browser-stale");
    stale.iter_mut().find(|(k, _)| *k == "version").unwrap().1 = "9";
    assert_eq!(
        html(admin.request(Method::POST, &path).form(&stale))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut forged = form("browser-forged");
    forged.push(("principal", "owner"));
    assert_eq!(
        html(admin.request(Method::POST, &path).form(&forged))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, lookup, _) = response(admin.request(
        Method::GET,
        &format!("/api/v1/access-requests/{}", group.group_id),
    ))
    .await;
    assert_eq!(status, StatusCode::OK, "{lookup}");
    assert_eq!(lookup["status"], "pending");
    assert!(lookup["items"][0]["grant_id"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_browser_refuses_other_origins_agents_seats_users_and_ended_sessions() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let path = format!("/app/access/requests/{}", group.request_ids[0]);
    assert_eq!(
        html(
            admin
                .hub
                .http
                .post(format!("{}{path}", admin.hub.url))
                .header("cookie", &admin.cookie)
                .header("origin", "https://foreign.test")
                .form(&form("origin"))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        html(
            admin
                .request(Method::POST, &path)
                .header(crate::agenthub::mcp::SEAT_HEADER, "worker")
                .form(&form("seat"))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, token) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let response = admin
        .hub
        .http
        .post(format!("{}{path}", admin.hub.url))
        .bearer_auth(token)
        .form(&form("agent"))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    assert!(response.headers().get("idempotent-replayed").is_none());
    let other = admin
        .hub
        .accounts
        .create_local_user(
            &admin.owner,
            "Other",
            "browser-other@example.test",
            "a good password",
        )
        .await
        .unwrap()
        .account;
    let session = admin
        .hub
        .accounts
        .open_session(&other.id, None)
        .await
        .unwrap();
    assert_eq!(
        html(
            admin
                .hub
                .http
                .get(format!("{}{path}", admin.hub.url))
                .header("cookie", format!("dg_session={session}"))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    admin.hub.accounts.end_sessions(&admin.owner).await.unwrap();
    let response = admin
        .request(Method::POST, &path)
        .form(&form("expired"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("/auth/login")
    );
}
