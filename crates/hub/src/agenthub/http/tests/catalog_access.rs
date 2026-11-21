use super::principals::Admin;
use super::*;
use crate::identity::accounts::api_keys::{NewKey, Scope};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_live_owner_and_key_scopes_are_enforced_before_receipt_replay() {
    let admin = or_skip!(Admin::new());
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let mut tokens = Vec::new();
    for scopes in [vec![Scope::BoardsRead], vec![Scope::CatalogRead]] {
        tokens.push(
            admin
                .hub
                .accounts
                .create_key(
                    &owner.tenant,
                    &owner.id,
                    &NewKey {
                        name: "Catalog reader".into(),
                        scopes,
                        hours: 24,
                        ..Default::default()
                    },
                )
                .await
                .unwrap()
                .1,
        );
    }
    let key_request = |method, path: &str, n: usize| {
        admin
            .hub
            .http
            .request(method, format!("{}{path}", admin.hub.url))
            .bearer_auth(&tokens[n])
    };
    assert_eq!(
        response(key_request(Method::GET, "/api/v1/capabilities", 0))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(key_request(Method::GET, "/api/v1/capabilities", 1))
            .await
            .0,
        StatusCode::OK
    );
    let body = json!({"id":"private","version":0,"label":"Owner","sandbox_levels":["read-only"]});
    let (_, made, _) = response(
        admin
            .request(Method::POST, "/api/v1/capabilities")
            .header("idempotency-key", "owner-only")
            .json(&body),
    )
    .await;
    assert_eq!(made["result"]["version"], 1, "{made}");
    assert_eq!(
        response(
            key_request(Method::POST, "/api/v1/capabilities", 1)
                .header("idempotency-key", "owner-only")
                .json(&body)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let user = admin
        .hub
        .accounts
        .create_local_user(
            &owner.id,
            "Member",
            "catalog-member@example.test",
            "A good local password",
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
    let user_request = |method, path: &str| {
        admin
            .hub
            .http
            .request(method, format!("{}{path}", admin.hub.url))
            .header("cookie", format!("dg_session={session}"))
            .header("origin", &admin.hub.url)
    };
    let (status, read, _) =
        response(user_request(Method::GET, "/api/v1/capabilities/private")).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["label"], "Owner");
    assert_eq!(
        response(user_request(Method::POST, "/api/v1/capabilities").json(&body))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        user_request(Method::GET, "/app/catalog")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(
            admin
                .request(Method::GET, "/api/v1/capabilities")
                .header(crate::agenthub::mcp::SEAT_HEADER, "seat")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        admin
            .hub
            .http
            .post(format!("{}/api/v1/capabilities", admin.hub.url))
            .header("cookie", &admin.cookie)
            .header("origin", "https://foreign.example")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_browser_retains_escaped_drafts_and_round_trips_structured_lists() {
    let admin = or_skip!(Admin::new());
    let me = admin
        .request(Method::GET, "/app/me")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(me.contains("id=\"catalog-link\""));
    let fields = [
        ("id", "custom.shell"),
        ("version", "0"),
        ("request_id", "browser-shell"),
        ("action", "save"),
        ("os", "linux"),
        ("label", "<Shell>"),
        ("argv_prefix", "[\"sh\",\"-c\"]"),
        ("env_allow", "[\"NAME,WITH,COMMAS\",\"OTHER\"]"),
        ("cwd_default", "/tmp"),
    ];
    let post = || {
        admin
            .request(Method::POST, "/app/catalog/shell-profiles/new")
            .form(&fields)
    };
    let made = post().send().await.unwrap();
    assert_eq!(made.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        post()
            .send()
            .await
            .unwrap()
            .headers()
            .get("idempotent-replayed")
            .unwrap(),
        "true"
    );
    let page = admin
        .request(
            Method::GET,
            "/app/catalog/shell-profiles/records/custom.shell",
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("&lt;Shell&gt;"));
    assert!(page.contains("NAME,WITH,COMMAS"));
    let (_, row, _) =
        response(admin.request(Method::GET, "/api/v1/shell-profiles/custom.shell")).await;
    assert_eq!(row["env_allow"], json!(["NAME,WITH,COMMAS", "OTHER"]));
    let mut broken = fields.to_vec();
    broken[1].1 = "1";
    broken[2].1 = "browser-shell-edit";
    broken[6].1 = "[broken<script>";
    let denied = admin
        .request(
            Method::POST,
            "/app/catalog/shell-profiles/records/custom.shell",
        )
        .form(&broken)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let text = denied.text().await.unwrap();
    assert!(text.contains("[broken&lt;script&gt;"));
    assert!(text.contains("value=\"browser-shell-edit\""));
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/shell-profiles/custom.shell"))
            .await
            .1,
        row
    );
    broken[6].1 = "[]";
    broken.push(("label", "Duplicate"));
    assert_eq!(
        admin
            .request(
                Method::POST,
                "/app/catalog/shell-profiles/records/custom.shell"
            )
            .form(&broken)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        admin
            .hub
            .http
            .post(format!("{}/app/catalog/shell-profiles/new", admin.hub.url))
            .header("cookie", &admin.cookie)
            .header("origin", "https://foreign.example")
            .form(&fields)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}
