//! `/admin`'s cards for the settings the hub reads while it runs (`settings/web/live.rs`,
//! `config/live.rs`), over HTTP as the router serves them, on the test Postgres
//! (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI sets it).

use serde_json::json;

use crate::tenancy::tests::tenants::Hosted;
use crate::tenancy::web::tests::operator::{hub_on, operator_cookie};

fn settings(
    hub: &Hosted,
    cookie: &str,
    group: &str,
    fields: &[(&str, &str)],
) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}/admin/settings/{group}", hub.url))
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

/// The settings the hub reads while it runs (`config/live.rs`): only the owner, only from the
/// hub's own page, all of a card or nothing; a save is in force at once for every part of the
/// hub, audited, and a fresh start reads it back; what the config file set stays as it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn settings_saved_on_the_page_apply_at_once_are_audited_and_leave_what_the_file_set() {
    let store = or_skip!(crate::store::testing::empty_postgres());
    let hub = or_skip!(hub_on(Some(&store), None));
    let owner = operator_cookie(&hub).await;

    // Every card, with the values in force; the file's tenancy.starts_per_hour is shown, not
    // offered (the test hub's config sets it), and the keys set at deploy are listed.
    let (status, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{page}");
    for words in [
        "Shortest password",
        "New accounts per address, per hour",
        "Machines update themselves",
        "A workflow link works for",
        "Keep each visit for",
        "Set when the hub is deployed",
        "tenancy.mode",
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    let starts = page
        .split("name=\"tenancy.starts_per_hour\"")
        .nth(1)
        .unwrap();
    assert!(
        starts[..starts.find('>').unwrap()].contains("disabled"),
        "pinned: {starts}"
    );

    // Nobody else, and not from another site.
    let other = hub.first_connect("Claude").await;
    let denied = settings(
        &hub,
        &other.cookie,
        "passwords",
        &[("passwords.min_length", "16")],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(denied.status(), 404);
    let foreign = hub
        .http
        .post(format!("{}/admin/settings/passwords", hub.url))
        .header(axum::http::header::COOKIE, &owner)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[("passwords.min_length", "16")])
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), 403);
    let unknown = settings(&hub, &owner, "no-such-card", &[])
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);

    // Out of range: nothing saved, every reason given.
    let short = settings(&hub, &owner, "passwords", &[("passwords.min_length", "7")])
        .send()
        .await
        .unwrap();
    assert_eq!(short.status(), 400);
    let (_, why) = Hosted::text(short).await;
    assert!(why.contains("8 to 128"), "{why}");
    assert!(hub.accounts.hub_settings().await.unwrap().is_empty());
    assert_eq!(hub.config.live().password_min, 8);

    // Saved: in force at once, for sign-up and the words it shows.
    let saved = settings(&hub, &owner, "passwords", &[("passwords.min_length", "16")])
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), 303);
    assert_eq!(hub.config.live().password_min, 16);
    let refused = hub
        .accounts
        .sign_up_password("fifteen@example.test", "fifteen-chars!!")
        .await
        .unwrap_err();
    assert_eq!(refused.code, "password_short");
    assert!(refused.message.contains("16"), "{}", refused.message);
    hub.accounts
        .sign_up_password("sixteen@example.test", "sixteen-chars!!!")
        .await
        .unwrap();

    // A switch left unchecked is off; a pinned value sent anyway changes nothing.
    assert!(hub.config.live().auto_update);
    let off = settings(&hub, &owner, "machines", &[])
        .send()
        .await
        .unwrap();
    assert_eq!(off.status(), 303);
    assert!(!hub.config.live().auto_update);
    let limits = settings(
        &hub,
        &owner,
        "sign-up",
        &[
            ("tenancy.starts_per_hour", "1"),
            ("tenancy.max_unsecured", "50"),
            ("tenancy.unsecured.machines", "1"),
            ("tenancy.unsecured.clients", "2"),
            ("tenancy.unsecured.idle_days", "3"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(limits.status(), 303);
    let live = hub.config.live();
    assert_eq!(live.starts_per_hour, 100, "the file's value stays");
    assert_eq!(live.max_unsecured, 50);
    assert_eq!(live.unsecured.machines, 1);
    assert_eq!(live.unsecured.clients, 2);
    assert_eq!(live.unsecured.idle_days, 3);
    assert!(live.anonymous_connect, "pinned by the file too");

    // Audited on the hub's own chain, by the owner, with the value and never anything else.
    let home = hub.accounts.home_tenant().await.unwrap();
    let events = hub.accounts.events(&home, 50).await.unwrap();
    let changed: Vec<&str> = events
        .iter()
        .filter(|event| event.event == "settings.changed")
        .map(|event| event.detail.as_str())
        .collect();
    for detail in [
        "passwords.min_length = 16",
        "machines.auto_update = off",
        "tenancy.max_unsecured = 50",
    ] {
        assert!(changed.contains(&detail), "{detail}: {changed:?}");
    }
    assert!(
        !changed
            .iter()
            .any(|detail| detail.contains("starts_per_hour")),
        "{changed:?}"
    );

    // A hub starting on this store reads them back.
    let fresh = crate::config::HubConfig::parse(
        &json!({
            "listen": "127.0.0.1:1", "public_url": "http://127.0.0.1:1",
            "auth": { "mode": "hub" }, "brokers": [], "grants": []
        })
        .to_string(),
    )
    .unwrap();
    let applied = crate::settings::store::apply_saved(&fresh, &hub.accounts)
        .await
        .unwrap();
    assert_eq!(applied, fresh.live());
    assert_eq!(applied.password_min, 16);
    assert!(!applied.auto_update);
    assert_eq!(applied.max_unsecured, 50);
}
