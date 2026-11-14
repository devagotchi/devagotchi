//! The hub's settings page over HTTP, the whole router as it serves: only the owner sees it, a
//! provider saved there is stored sealed, offered at once with no restart, and sends people to
//! the provider with the id that was saved; a ClickHouse saved there is checked first, then
//! AgentHub's events go to it at once, and again after a restart. On SQLite, and on Postgres
//! when the test server is set (CI).

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

/// A stand-in ClickHouse: answers every query while up, refuses while down, and keeps the
/// passwords it was sent.
#[derive(Clone, Default)]
struct StandIn {
    down: std::sync::Arc<std::sync::atomic::AtomicBool>,
    keys: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl StandIn {
    async fn start() -> (String, Self) {
        use axum::{extract::State, http::HeaderMap, routing::post};
        async fn answer(
            State(fake): State<StandIn>,
            headers: HeaderMap,
        ) -> (axum::http::StatusCode, &'static str) {
            if let Some(key) = headers
                .get("x-clickhouse-key")
                .and_then(|v| v.to_str().ok())
            {
                fake.keys.lock().unwrap().push(key.to_string());
            }
            if fake.down.load(std::sync::atomic::Ordering::SeqCst) {
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "Code: 210. DB::NetException: down",
                )
            } else {
                (axum::http::StatusCode::OK, "")
            }
        }
        let fake = Self::default();
        let app = axum::Router::new()
            .route("/", post(answer))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, fake)
    }

    fn set_down(&self, down: bool) {
        self.down.store(down, std::sync::atomic::Ordering::SeqCst);
    }
}

fn analytics(hub: &Hosted, cookie: &str, fields: &[(&str, &str)]) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}/admin/analytics", hub.url))
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_clickhouse_saved_on_the_settings_page_is_checked_sealed_and_exported_to_at_once() {
    // On SQLite: the page says where the boards are and that statistics need Postgres.
    let hub = hub_on(None, None).await;
    let owner = operator_cookie(&hub).await;
    let (status, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Where AgentHub keeps its boards"), "{page}");
    assert!(page.contains("SQLite file"), "{page}");
    assert!(
        page.contains("Statistics need this hub on Postgres"),
        "{page}"
    );
    let (url, _fake) = StandIn::start().await;
    let refused = analytics(&hub, &owner, &[("url", &url), ("enabled", "on")])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 409);

    let Some(store) = crate::store::testing::empty_postgres().await else {
        return;
    };
    let hub = hub_on(Some(&store), None).await;
    let owner = operator_cookie(&hub).await;
    let (url, fake) = StandIn::start().await;

    // The page: which Postgres and schema, and how to move to one's own; nothing exported yet.
    let (status, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Where AgentHub keeps its boards"), "{page}");
    assert!(page.contains("DEVAGOTCHI_HUB_STORAGE_URL"), "{page}");
    assert!(
        page.contains("Tenants kept apart by the database"),
        "{page}"
    );
    assert!(page.contains("Statistics: ClickHouse"), "{page}");
    assert!(page.contains("Not set up"), "{page}");
    assert!(hub.hub.boards.warehouse().is_none());

    // Only the owner, only from the hub's own page, and only what can be sent to ClickHouse.
    let mut other = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut other, "board_bind", json!({ "project": "x" }))
        .await;
    assert!(bound.get("error").is_none(), "{bound}");
    let not_owner = analytics(&hub, &other.cookie, &[("url", &url), ("enabled", "on")])
        .send()
        .await
        .unwrap();
    assert_eq!(not_owner.status(), 404);
    let cross = hub
        .http
        .post(format!("{}/admin/analytics", hub.url))
        .header(axum::http::header::COOKIE, &owner)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[("url", url.as_str()), ("enabled", "on")])
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);
    for bad in [
        vec![("url", "ftp://clickhouse.example"), ("enabled", "on")],
        vec![
            ("url", "https://user:pw@clickhouse.example"),
            ("enabled", "on"),
        ],
        vec![
            ("url", url.as_str()),
            ("database", "x; DROP TABLE y"),
            ("enabled", "on"),
        ],
        vec![
            ("url", url.as_str()),
            ("database", "9lives"),
            ("enabled", "on"),
        ],
    ] {
        let response = analytics(&hub, &owner, &bad).send().await.unwrap();
        assert_eq!(response.status(), 400, "{bad:?}");
    }

    // ClickHouse down: checked first, so nothing is saved and nothing runs.
    fake.set_down(true);
    let down = analytics(
        &hub,
        &owner,
        &[
            ("url", &url),
            ("user", "devagotchi"),
            ("password", "s3cret"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(down.status(), 400);
    let body = down.text().await.unwrap();
    assert!(body.contains("ClickHouse did not take it"), "{body}");
    assert!(hub.accounts.clickhouse_setting().await.unwrap().is_none());
    assert!(hub.hub.boards.warehouse().is_none());

    // Up: saved with the password sealed, and the export runs now, with that password.
    fake.set_down(false);
    let saved = analytics(
        &hub,
        &owner,
        &[
            ("url", &format!("{url}/")),
            ("database", "agenthub_stats"),
            ("user", "devagotchi"),
            ("password", "s3cret"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(saved.status(), 303);
    let stored = hub.accounts.clickhouse_setting().await.unwrap().unwrap();
    assert_eq!(stored.url, url, "kept without its trailing slash");
    assert_eq!(stored.database, "agenthub_stats");
    assert_eq!(stored.user, "devagotchi");
    assert!(stored.enabled);
    assert!(!stored.sealed_password.is_empty());
    assert!(!stored.sealed_password.contains("s3cret"));
    let running = hub.hub.boards.warehouse().expect("exporting now");
    assert_eq!(running.url(), url);
    assert_eq!(running.database(), "agenthub_stats");
    assert!(fake.keys.lock().unwrap().iter().any(|key| key == "s3cret"));
    let (_, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert!(page.contains("Events waiting in Postgres"), "{page}");
    assert!(page.contains("Saved. Leave empty to keep it."), "{page}");
    assert!(!page.contains("s3cret"), "the password is never shown");

    // Saved again with no password: the sealed one is kept, and a new export replaces the old.
    fake.keys.lock().unwrap().clear();
    let again = analytics(
        &hub,
        &owner,
        &[
            ("url", &url),
            ("database", "agenthub_stats"),
            ("user", "devagotchi"),
            ("enabled", "on"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(again.status(), 303);
    assert_eq!(
        hub.accounts
            .clickhouse_setting()
            .await
            .unwrap()
            .unwrap()
            .sealed_password,
        stored.sealed_password
    );
    assert!(fake.keys.lock().unwrap().iter().any(|key| key == "s3cret"));
    assert!(!std::sync::Arc::ptr_eq(
        &running,
        &hub.hub.boards.warehouse().unwrap()
    ));

    // Turned off: kept, not run.
    let off = analytics(
        &hub,
        &owner,
        &[
            ("url", &url),
            ("database", "agenthub_stats"),
            ("user", "devagotchi"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(off.status(), 303);
    assert!(hub.hub.boards.warehouse().is_none());
    assert!(
        !hub.accounts
            .clickhouse_setting()
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    // After a restart: what was saved starts again, unsealed with the hub's own key.
    let sealer = crate::sealed::Sealer::ephemeral();
    let mut on = hub.accounts.clickhouse_setting().await.unwrap().unwrap();
    on.enabled = true;
    on.sealed_password = sealer.seal(crate::warehouse::SEALED_AS, "after-restart");
    hub.accounts.save_clickhouse("test", &on).await.unwrap();
    fake.keys.lock().unwrap().clear();
    let started = crate::warehouse::start_stored(
        &hub.hub.boards,
        &hub.accounts,
        &sealer,
        hub.accounts.store().clone(),
    )
    .await
    .unwrap();
    assert!(started);
    let restarted = hub
        .hub
        .boards
        .warehouse()
        .expect("exporting after a restart");
    assert_eq!(restarted.url(), url);
    for _ in 0..50 {
        if fake
            .keys
            .lock()
            .unwrap()
            .iter()
            .any(|key| key == "after-restart")
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        fake.keys
            .lock()
            .unwrap()
            .iter()
            .any(|key| key == "after-restart")
    );
    hub.hub.boards.set_warehouse(None);

    // A ClickHouse the hub's configuration sets is shown, not changed, here.
    let fresh = crate::store::testing::empty_postgres().await.unwrap();
    let configured = crate::tenant_tests::hosted_with(Some(&fresh), |_, _| {
        json!({ "operators": ["github:42"], "clickhouse": { "url": url, "database": "from_config" } })
    })
    .await;
    let owner = operator_cookie(&configured).await;
    let (_, page) = Hosted::text(configured.get("/admin", Some(&owner)).await).await;
    assert!(
        page.contains("On, from this hub&#x27;s configuration")
            || page.contains("On, from this hub's configuration"),
        "{page}"
    );
    assert!(page.contains("from_config"), "{page}");
    let refused = analytics(&configured, &owner, &[("url", &url), ("enabled", "on")])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 409);
}
