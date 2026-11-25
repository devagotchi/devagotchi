use crate::tenancy::tests::tenants::{Hosted, hosted_with};
use crate::tenancy::web::tests::operator::operator_cookie;
use serde_json::json;

async fn setup() -> Option<(Hosted, String)> {
    let hub = hosted_with(
        None,
        |_, _| json!({"tenancy":{"mode":"single"},"operators":["github:42"]}),
    )
    .await?;
    let cookie = operator_cookie(&hub).await;
    Some((hub, cookie))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_creates_edits_disables_and_resets_a_local_user_over_http() {
    let (hub, owner) = or_skip!(setup());
    let owner_account = hub.accounts.owner().await.unwrap().unwrap();
    let owner_path = format!("/admin/users/{}", owner_account.id);
    let (status, owner_page) = Hosted::text(hub.get(&owner_path, Some(&owner)).await).await;
    assert_eq!(status, 200);
    assert!(owner_page.contains("checked disabled"));
    assert_eq!(
        hub.form(
            &owner_path,
            Some(&owner),
            &[("name", "Owner"), ("version", "1")]
        )
        .await
        .status(),
        400
    );
    let response = hub
        .form(
            "/admin/users",
            Some(&owner),
            &[
                ("name", "<Alice>"),
                ("email", "alice@example.test"),
                ("password", "password-one"),
            ],
        )
        .await;
    assert_eq!(response.status(), 303);
    let path = response.headers()["location"]
        .to_str()
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_owned();
    let account = hub
        .accounts
        .sign_in_password("alice@example.test", "password-one")
        .await
        .unwrap();
    assert_eq!(path, format!("/admin/users/{}", account.id));
    let (_, html) = Hosted::text(hub.get(&path, Some(&owner)).await).await;
    assert!(html.contains("&lt;Alice&gt;"));
    assert!(!html.contains("password-one"));
    assert!(!html.contains("$argon2"));
    assert!(html.contains("name=\"version\" value=\"1\""));
    let session = hub.accounts.open_session(&account.id, None).await.unwrap();
    let cookie = format!("dg_session={session}");
    for cookie in [None, Some(cookie.as_str())] {
        for path in ["/admin/users", "/admin/users/new", path.as_str()] {
            assert_eq!(hub.get(path, cookie).await.status(), 404);
        }
        assert_eq!(
            hub.form(
                &path,
                cookie,
                &[("name", "Intruder"), ("version", "1"), ("enabled", "true")]
            )
            .await
            .status(),
            404
        );
    }
    for (origin, fetch) in [
        (None, None),
        (Some("https://elsewhere.test"), Some("same-origin")),
        (Some(hub.url.as_str()), Some("cross-site")),
    ] {
        for (path, form) in [
            (
                "/admin/users".to_owned(),
                vec![
                    ("name", "Intruder"),
                    ("email", "intruder@example.test"),
                    ("password", "password-test"),
                ],
            ),
            (
                path.clone(),
                vec![("name", "Intruder"), ("version", "1"), ("enabled", "true")],
            ),
            (
                format!("{path}/password"),
                vec![("password", "password-test"), ("version", "1")],
            ),
        ] {
            let mut request = hub
                .http
                .post(format!("{}{path}", hub.url))
                .header("Cookie", &owner)
                .form(&form);
            if let Some(origin) = origin {
                request = request.header("Origin", origin);
            }
            if let Some(fetch) = fetch {
                request = request.header("Sec-Fetch-Site", fetch);
            }
            assert_eq!(request.send().await.unwrap().status(), 403, "{path}");
        }
    }
    assert_eq!(
        hub.form(
            &path,
            Some(&owner),
            &[("name", "Alice renamed"), ("version", "1")]
        )
        .await
        .status(),
        303
    );
    assert!(hub.accounts.session(&session).await.unwrap().is_none());
    assert!(
        hub.accounts
            .sign_in_password("alice@example.test", "password-one")
            .await
            .is_err()
    );
    assert_eq!(
        hub.form(
            &path,
            Some(&owner),
            &[("name", "Stale"), ("version", "1"), ("enabled", "true")]
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        hub.form(
            &path,
            Some(&owner),
            &[
                ("name", "Alice renamed"),
                ("version", "2"),
                ("enabled", "true")
            ]
        )
        .await
        .status(),
        303
    );
    assert_eq!(
        hub.form(
            &format!("{path}/password"),
            Some(&owner),
            &[("password", "replacement-two"), ("version", "3")]
        )
        .await
        .status(),
        303
    );
    assert!(
        hub.accounts
            .sign_in_password("alice@example.test", "password-one")
            .await
            .is_err()
    );
    assert_eq!(
        hub.accounts
            .sign_in_password("alice@example.test", "replacement-two")
            .await
            .unwrap()
            .id,
        account.id
    );
    let response = hub
        .form(
            "/admin/users",
            Some(&owner),
            &[
                ("name", "Keep <name>"),
                ("email", "alice@example.test"),
                ("password", "never-echo-this"),
            ],
        )
        .await;
    let (status, html) = Hosted::text(response).await;
    assert_eq!(status, 400);
    assert!(html.contains("Keep &lt;name&gt;"));
    assert!(!html.contains("never-echo-this"));
    let (_, history) = Hosted::text(
        hub.get("/admin/audit?action=admin.user.", Some(&owner))
            .await,
    )
    .await;
    for action in [
        "admin.user.create",
        "admin.user.update",
        "admin.user.password_reset",
    ] {
        assert!(history.contains(action), "{history}");
    }
    assert!(!history.contains("replacement-two"));
    hub.accounts
        .set_password_for_email(&owner_account.id, "owner@example.test", "owner-before")
        .await
        .unwrap();
    let link = hub
        .accounts
        .issue_link(
            crate::identity::accounts::LinkPurpose::Owner,
            &owner_account.id,
        )
        .await
        .unwrap();
    let response = hub
        .form(
            &format!("{owner_path}/password"),
            Some(&owner),
            &[("version", "1"), ("password", "owner-after")],
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(
        response.headers()["location"],
        "/auth/login?return_to=%2Fadmin%2Fusers"
    );
    assert_eq!(hub.get("/admin/users", Some(&owner)).await.status(), 404);
    assert!(
        hub.accounts
            .check_link(crate::identity::accounts::LinkPurpose::Owner, &link)
            .await
            .is_err()
    );
    assert_eq!(
        hub.accounts
            .sign_in_password("owner@example.test", "owner-after")
            .await
            .unwrap()
            .id,
        owner_account.id
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_user_pages_paginate_and_storage_failure_is_not_an_empty_list() {
    let (hub, owner) = or_skip!(setup());
    let admin = hub.accounts.store().admin();
    let tenant: String =
        sqlx::query_scalar("SELECT tenant_id FROM devagotchi.auth_accounts WHERE owner=1")
            .fetch_one(admin)
            .await
            .unwrap();
    for n in 0..102 {
        hub.accounts
            .create(&format!("person-{n:03}"), &format!("Person {n}"), false)
            .await
            .unwrap();
    }
    let (status, html) = Hosted::text(hub.get("/admin/users", Some(&owner)).await).await;
    assert_eq!(status, 200);
    assert_eq!(html.matches("<li><a href=\"/admin/users/").count(), 100);
    let next = html
        .split("id=\"users-more\" href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let (_, rest) = Hosted::text(hub.get(next, Some(&owner)).await).await;
    assert_eq!(rest.matches("<li><a href=\"/admin/users/").count(), 3);
    assert!(!rest.contains("id=\"users-more\""));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT tenant_id) FROM devagotchi.auth_accounts"
        )
        .fetch_one(admin)
        .await
        .unwrap(),
        1,
        "{tenant}"
    );
    sqlx::query("ALTER TABLE devagotchi.auth_passwords RENAME TO broken_passwords")
        .execute(admin)
        .await
        .unwrap();
    assert_eq!(hub.get("/admin/users", Some(&owner)).await.status(), 503);
}
