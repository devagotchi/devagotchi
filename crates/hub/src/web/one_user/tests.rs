//! A single-environment hub (`tenancy.mode single`) has owner-managed local users,
//! no Organisations card on You, no organisation link on Boards,
//! and `/orgs*`, `/invite` and `/assets/invite.js` answer as an unknown path does. A hosted
//! hub (`tenancy.mode multi`) keeps all of them. The whole router as it serves, on the test
//! Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI sets it).
//! Organisation APIs follow that boundary too; shared project permissions remain local.

use serde_json::json;

use crate::{
    tenancy::tests::tenants::{Hosted, hosted_with},
    web::operator_tests_support::operator_cookie,
};

/// A hub in `mode` whose owner is `github:42`, and that owner's browser cookie.
async fn hub(mode: &str) -> Option<(Hosted, String)> {
    let hub = hosted_with(None, |_, _| {
        json!({
            "tenancy": { "mode": mode },
            "operators": ["github:42"]
        })
    })
    .await?;
    let owner = operator_cookie(&hub).await;
    Some((hub, owner))
}

/// What the hub answers for a path it has never had.
async fn unknown(hub: &Hosted, cookie: Option<&str>) -> (u16, String) {
    Hosted::text(hub.get("/no-such-page", cookie).await).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_single_environment_has_local_users_without_organization_surfaces() {
    let (hub, owner) = or_skip!(hub("single"));
    assert!(!hub.config.multi_tenant());

    // `/admin`: local users, without hosted sign-up limits or organizations.
    let (status, admin) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{admin}");
    assert!(admin.contains("id=\"sign-in\""), "{admin}");
    assert!(admin.contains("id=\"access\""), "{admin}");
    assert!(admin.contains("id=\"people\""), "{admin}");
    assert!(admin.contains("href=\"/admin/users\""), "{admin}");
    assert!(admin.contains("href=\"#people\""), "{admin}");
    assert!(!admin.contains("name=\"tenancy."), "{admin}");
    assert!(!admin.contains("href=\"/orgs\""), "{admin}");
    // The count of AI clients is the catalog's, not a number written into the page.
    let clients = format!("{} clients", devagotchi_connectors::all().len());
    assert!(admin.contains(&clients), "{admin}");

    // The organisation pages and the invite page answer as a path the hub never had, for
    // the owner and for anyone else, read or posted.
    let (gone, nothing) = unknown(&hub, Some(&owner)).await;
    assert_eq!(gone, 404);
    for path in ["/orgs", "/orgs/acme", "/invite", "/assets/invite.js"] {
        for cookie in [Some(owner.as_str()), None] {
            let (status, body) = Hosted::text(hub.get(path, cookie).await).await;
            assert_eq!((status, body.as_str()), (gone, nothing.as_str()), "{path}");
        }
    }
    for path in ["/orgs", "/orgs/acme/invites", "/invite"] {
        let response = hub
            .form(path, Some(&owner), &[("org", "acme"), ("name", "Acme")])
            .await;
        let (status, body) = Hosted::text(response).await;
        assert_eq!((status, body.as_str()), (gone, nothing.as_str()), "{path}");
    }

    // You: no Organisations card. Boards: no link to them.
    let (status, me) = Hosted::text(hub.get("/app/me", Some(&owner)).await).await;
    assert_eq!(status, 200, "{me}");
    assert!(me.contains("id=\"who\""), "{me}");
    assert!(!me.contains("id=\"orgs\""), "{me}");
    assert!(!me.contains("href=\"/orgs"), "{me}");
    let (status, boards) = Hosted::text(hub.get("/app/boards", Some(&owner)).await).await;
    assert_eq!(status, 200, "{boards}");
    assert!(!boards.contains("id=\"orgs-link\""), "{boards}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hosted_hub_keeps_the_multi_user_surfaces() {
    let (hub, owner) = or_skip!(hub("multi"));
    assert_eq!(hub.get("/admin/users", Some(&owner)).await.status(), 404);
    assert!(hub.config.multi_tenant());

    let (status, admin) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{admin}");
    assert!(admin.contains("id=\"people\""), "{admin}");
    assert!(admin.contains("href=\"#people\""), "{admin}");
    assert!(admin.contains("name=\"tenancy."), "{admin}");
    assert!(admin.contains("href=\"/orgs\""), "{admin}");

    let (status, orgs) = Hosted::text(hub.get("/orgs", Some(&owner)).await).await;
    assert_eq!(status, 200, "{orgs}");
    let (status, invite) = Hosted::text(hub.get("/invite", Some(&owner)).await).await;
    assert_eq!(status, 200, "{invite}");
    let (status, _) = Hosted::text(hub.get("/assets/invite.js", None).await).await;
    assert_eq!(status, 200);

    let (status, me) = Hosted::text(hub.get("/app/me", Some(&owner)).await).await;
    assert_eq!(status, 200, "{me}");
    assert!(me.contains("id=\"orgs\""), "{me}");
    let (status, boards) = Hosted::text(hub.get("/app/boards", Some(&owner)).await).await;
    assert_eq!(status, 200, "{boards}");
    assert!(boards.contains("id=\"orgs-link\""), "{boards}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_single_environment_hides_organisation_apis_but_keeps_project_permissions() {
    let (hub, owner) = or_skip!(hub("single"));
    let missing = unknown(&hub, Some(&owner)).await;
    assert_eq!(missing.0, 404);
    let paths = [
        ("GET", "/api/v1/orgs"),
        ("POST", "/api/v1/orgs"),
        ("GET", "/api/v1/orgs/acme"),
        ("PUT", "/api/v1/orgs/acme/members/person"),
        ("DELETE", "/api/v1/orgs/acme/members/person"),
        ("POST", "/api/v1/orgs/acme/teams"),
        ("DELETE", "/api/v1/orgs/acme/teams/builders"),
        ("PUT", "/api/v1/orgs/acme/teams/builders/members/person"),
        ("DELETE", "/api/v1/orgs/acme/teams/builders/members/person"),
        ("POST", "/api/v1/orgs/acme/projects"),
        ("PUT", "/api/v1/orgs/acme/projects/demo"),
        (
            "PUT",
            "/api/v1/orgs/acme/projects/demo/members/subject/person",
        ),
        (
            "DELETE",
            "/api/v1/orgs/acme/projects/demo/members/subject/person",
        ),
        ("GET", "/api/v1/orgs/acme/invites"),
        ("POST", "/api/v1/orgs/acme/invites"),
        ("DELETE", "/api/v1/orgs/acme/invites/invite"),
        ("POST", "/api/v1/invites/accept"),
    ];
    for (method, path) in paths {
        for cookie in [Some(owner.as_str()), None] {
            let mut request = hub
                .http
                .request(
                    method.parse().expect("HTTP method"),
                    format!("{}{path}", hub.url),
                )
                .header("Origin", &hub.url)
                .json(&json!({}));
            if let Some(cookie) = cookie {
                request = request.header("Cookie", cookie);
            }
            let response = request.send().await.expect("organisation API response");
            assert_eq!(Hosted::text(response).await, missing, "{method} {path}");
        }
    }

    // These routes must still reach their authentication guard, rather than disappear
    // with the organisation router: local users need shared-project permissions.
    for (method, path) in [
        ("GET", "/api/v1/projects/demo/members"),
        ("PUT", "/api/v1/projects/demo/members/subject/person"),
        ("DELETE", "/api/v1/projects/demo/members/subject/person"),
    ] {
        let response = hub
            .http
            .request(
                method.parse().expect("HTTP method"),
                format!("{}{path}", hub.url),
            )
            .json(&json!({}))
            .send()
            .await
            .expect("project membership response");
        assert_eq!(response.status(), 401, "{method} {path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hosted_environment_keeps_its_organisation_api() {
    let (hub, owner) = or_skip!(hub("multi"));
    assert_eq!(hub.get("/admin/users", Some(&owner)).await.status(), 404);
    let response = hub.get("/api/v1/orgs", Some(&owner)).await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("organisation list");
    assert!(body["orgs"].is_array(), "{body}");
    assert_eq!(hub.get("/api/v1/orgs", None).await.status(), 401);
}
