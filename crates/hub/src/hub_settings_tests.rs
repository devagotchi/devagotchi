//! The hub's settings page over HTTP, the whole router as it serves: only the owner sees it, a
//! provider saved there is stored sealed, offered at once with no restart, and sends people to
//! the provider with the id that was saved. On SQLite, and on Postgres when the test server is
//! set (CI).

use serde_json::json;

use crate::{
    tenant_tests::Hosted,
    web::operator_tests_support::{hub_on, operator_cookie},
};

fn save(
    hub: &Hosted,
    cookie: &str,
    name: &str,
    fields: &[(&str, &str)],
) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}/admin/sign-in/{name}", hub.url))
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

async fn run(store: Option<&str>) {
    let hub = hub_on(store, None).await;
    let owner = operator_cookie(&hub).await;

    // The page: guided, with this hub's own callback address, and nothing set yet.
    let (status, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Authorized redirect URI"), "{page}");
    assert!(
        page.contains(&format!("{}/auth/provider/google/callback", hub.url)),
        "{page}"
    );
    assert!(page.contains("Not set up"), "{page}");

    // Nobody else knows it exists, signed in or not.
    let mut other = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut other, "board_bind", json!({ "project": "x" }))
        .await;
    assert!(bound.get("error").is_none(), "{bound}");
    for cookie in [None, Some(other.cookie.as_str())] {
        let (status, _) = Hosted::text(hub.get("/admin", cookie).await).await;
        assert_eq!(status, 404);
    }
    let denied = save(
        &hub,
        &other.cookie,
        "github",
        &[
            ("client_id", "x"),
            ("client_secret", "y"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(denied.status(), 404);

    // Another site's form changes nothing; a new provider needs its secret.
    let cross = hub
        .http
        .post(format!("{}/admin/sign-in/github", hub.url))
        .header(axum::http::header::COOKIE, &owner)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[
            ("client_id", "x"),
            ("client_secret", "y"),
            ("enabled", "on"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);
    let no_secret = save(
        &hub,
        &owner,
        "github",
        &[("client_id", "x"), ("enabled", "on")],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(no_secret.status(), 400);

    // Before: the sign-in is not offered.
    let (status, _) = Hosted::text(hub.get("/auth/provider/github", None).await).await;
    assert_eq!(status, 404);

    // Saved: stored sealed, offered at once, and it sends people to GitHub with this client id.
    let saved = save(
        &hub,
        &owner,
        "github",
        &[
            ("client_id", "the-client-id"),
            ("client_secret", "the-very-secret-value"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(saved.status(), 303);
    let go = hub.get("/auth/provider/github", None).await;
    assert_eq!(go.status(), 303);
    let to = go.headers()["location"].to_str().unwrap().to_string();
    assert!(
        to.starts_with("https://github.com/login/oauth/authorize"),
        "{to}"
    );
    assert!(to.contains("client_id=the-client-id"), "{to}");
    let (_, login) = Hosted::text(hub.get("/auth/login", None).await).await;
    assert!(login.contains("/auth/provider/github"), "{login}");

    let settings = hub.accounts.sign_in_settings().await.unwrap();
    assert_eq!(settings.len(), 1);
    assert!(
        !settings[0].sealed_secret.contains("very-secret"),
        "sealed at rest"
    );

    // The page never gives the secret back.
    let (_, page) = Hosted::text(hub.get("/admin?saved=github", Some(&owner)).await).await;
    assert!(page.contains("the-client-id"), "{page}");
    assert!(!page.contains("the-very-secret-value"), "{page}");
    assert!(page.contains("Saved. Leave empty to keep it."), "{page}");

    // Saved again without the secret keeps it; off takes the button away at once.
    let kept = save(&hub, &owner, "github", &[("client_id", "second-id")])
        .send()
        .await
        .unwrap();
    assert_eq!(kept.status(), 303);
    let after = hub.accounts.sign_in_settings().await.unwrap();
    assert_eq!(after[0].sealed_secret, settings[0].sealed_secret);
    assert!(!after[0].enabled);
    let (status, _) = Hosted::text(hub.get("/auth/provider/github", None).await).await;
    assert_eq!(status, 404, "turned off applies at once");

    // Sign in with Shippin: its card has Nomen's issuer filled in, and saving it offers it at once.
    let (_, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert!(page.contains("Shippin (Nomen)"), "{page}");
    assert!(
        page.contains("value=\"https://id.shippin.cloud\""),
        "{page}"
    );
    assert!(
        page.contains("Add another OpenID Connect provider"),
        "{page}"
    );
    let nomen = save(
        &hub,
        &owner,
        "nomen",
        &[
            ("issuer", "https://id.shippin.cloud"),
            ("label", "Shippin"),
            ("client_id", "devagotchi-hub"),
            ("client_secret", "nomen-secret-value"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(nomen.status(), 303);
    let (_, login) = Hosted::text(hub.get("/auth/login", None).await).await;
    assert!(
        login.contains("/auth/provider/nomen") && login.contains("Shippin"),
        "{login}"
    );
    let stored = hub.accounts.sign_in_settings().await.unwrap();
    let shippin = stored.iter().find(|s| s.name == "nomen").unwrap();
    assert_eq!(
        (shippin.kind.as_str(), shippin.issuer.as_str()),
        ("oidc", "https://id.shippin.cloud")
    );
    assert!(!shippin.sealed_secret.contains("nomen-secret-value"));

    // Another provider by name, with a public client; an issuer that is not https is refused,
    // and so is a name already taken.
    let plain = save(
        &hub,
        &owner,
        "new",
        &[
            ("name", "okta"),
            ("label", "Okta"),
            ("issuer", "http://idp.example.com"),
            ("client_id", "x"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(plain.status(), 400);
    let taken = save(
        &hub,
        &owner,
        "new",
        &[
            ("name", "nomen"),
            ("issuer", "https://idp.example.com"),
            ("client_id", "x"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(taken.status(), 409);
    let okta = save(
        &hub,
        &owner,
        "new",
        &[
            ("name", "okta"),
            ("label", "Okta"),
            ("issuer", "https://example.okta.com"),
            ("client_id", "public-client"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(okta.status(), 303);
    let (_, login) = Hosted::text(hub.get("/auth/login", None).await).await;
    assert!(
        login.contains("/auth/provider/okta") && login.contains("Okta"),
        "a public client is offered too: {login}"
    );
    let (_, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert!(page.contains("id=\"sign-in-okta\""), "its own card: {page}");
    assert!(
        page.contains(&format!("{}/auth/provider/okta/callback", hub.url)),
        "{page}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_provider_saved_on_the_settings_page_is_sealed_and_offered_at_once() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}
