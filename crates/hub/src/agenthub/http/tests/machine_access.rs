use super::principals::Admin;
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn machine_access_settings_http_replays_and_refuses_origin_scope_and_stale_versions() {
    let admin = or_skip!(Admin::new());
    let path = "/api/v1/access-settings";
    let (status, mut value, _) = response(admin.request(Method::GET, path)).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["request_expiry_minutes"], 30);
    value["request_expiry_minutes"] = 60.into();
    let send = || {
        admin
            .request(Method::PUT, path)
            .header("idempotency-key", "settings-http")
            .json(&value)
    };
    let (status, saved, replayed) = response(send()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert!(!replayed);
    let (_, repeated, replayed) = response(send()).await;
    assert!(replayed);
    assert_eq!(repeated["result"], saved["result"]);
    assert_eq!(
        response(
            admin
                .request(Method::PUT, path)
                .header("idempotency-key", "stale-settings")
                .json(&value)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        response(admin.request(Method::PUT, path).json(&value))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(send().header("origin", "https://elsewhere.example.test"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        admin
            .hub
            .http
            .put(format!("{}{path}", admin.hub.url))
            .header("cookie", &admin.cookie)
            .header("origin", "https://elsewhere.example.test")
            .header("idempotency-key", "foreign-origin")
            .json(&value)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(
            admin
                .request(Method::GET, path)
                .header(crate::agenthub::mcp::SEAT_HEADER, "agent")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let user = admin
        .hub
        .accounts
        .create_local_user(
            &admin.owner,
            "Other",
            "other@example.test",
            "a good password",
        )
        .await
        .unwrap()
        .account;
    let session = admin
        .hub
        .accounts
        .open_session(&user.id, None)
        .await
        .unwrap();
    let (status, denied, _) = response(
        admin
            .hub
            .http
            .get(format!("{}{path}", admin.hub.url))
            .header("cookie", format!("dg_session={session}")),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
}
