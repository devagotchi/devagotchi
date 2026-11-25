//! Sign in with Shippin: the issuer is a setting, the switch defaults to on, and turning
//! it off keeps the install-time client secret.

use std::sync::Arc;

use serde_json::json;

use crate::identity::web::{
    provider_apply::overlay, providers::Provider, sign_in_cards::SHIPPIN_NOTICE,
};
use crate::tenancy::web::tests::operator::{hub_on, operator_cookie};
use crate::{
    config::ProviderConfig,
    identity::{Sealer, accounts::sign_in_settings::StoredSignIn},
    tenancy::tests::tenants::Hosted,
};

fn nomen(client_id: &str, secret: Option<&str>) -> Arc<Provider> {
    let config: ProviderConfig = serde_json::from_value(json!({
        "kind": "oidc",
        "name": "nomen",
        "label": "Shippin",
        "issuer": "https://id.example.com",
        "client_id": client_id,
    }))
    .unwrap();
    Provider::from_parts(config, client_id.to_string(), secret.map(str::to_string))
}

fn row(client_id: &str, sealed_secret: &str, enabled: bool) -> StoredSignIn {
    StoredSignIn {
        name: "nomen".into(),
        kind: "oidc".into(),
        label: "Shippin".into(),
        issuer: "https://id.example.com".into(),
        client_id: client_id.into(),
        sealed_secret: sealed_secret.into(),
        enabled,
        updated_at: 0,
        updated_by: "owner".into(),
    }
}

#[test]
fn the_switch_hides_shippin_and_keeps_the_install_secret() {
    let file = nomen("hub-client", Some("file-secret"));
    let sealer = Sealer::ephemeral();

    let offered = overlay(std::slice::from_ref(&file), &[], &sealer);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].secret(), Some("file-secret"));

    let off = overlay(
        std::slice::from_ref(&file),
        &[row("hub-client", "", false)],
        &sealer,
    );
    assert!(off.is_empty(), "the switch off takes the button away");

    let on = overlay(
        std::slice::from_ref(&file),
        &[row("hub-client", "", true)],
        &sealer,
    );
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].client_id(), "hub-client");
    assert_eq!(
        on[0].secret(),
        Some("file-secret"),
        "turning it back on keeps the secret from the config file"
    );

    let sealed = sealer.seal("nomen", "page-secret");
    let replaced = overlay(
        std::slice::from_ref(&file),
        &[row("hub-client", &sealed, true)],
        &sealer,
    );
    assert_eq!(replaced[0].secret(), Some("page-secret"));

    let public = overlay(
        std::slice::from_ref(&file),
        &[row("other-client", "", true)],
        &sealer,
    );
    assert_eq!(public[0].client_id(), "other-client");
    assert_eq!(public[0].secret(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_hub_shows_shippin_off_until_an_issuer_is_saved() {
    let hub = or_skip!(hub_on(None, None));
    let owner = operator_cookie(&hub).await;
    let (_, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert!(page.contains("id=\"sign-in-nomen\""), "{page}");
    let nomen_card = page
        .split("id=\"sign-in-nomen\"")
        .nth(1)
        .unwrap()
        .split("id=\"sign-in-oidc\"")
        .next()
        .unwrap();
    assert!(nomen_card.contains(SHIPPIN_NOTICE), "{nomen_card}");
    assert!(
        nomen_card.contains("Offer Shippin on the sign-in page"),
        "{nomen_card}"
    );
    assert!(
        nomen_card.contains("value=\"on\" checked"),
        "the switch defaults to on: {nomen_card}"
    );
    assert!(
        !nomen_card.contains("value=\"https://"),
        "the issuer is empty: {nomen_card}"
    );
    assert!(
        !page.contains("id.shippin.cloud") && !page.contains("app.devagotchi.com"),
        "{page}"
    );
    // The issuer field is empty. The placeholder is the only host-shaped text.
    assert!(
        page.contains("name=\"issuer\"") && page.contains("placeholder=\"https://id.example.com\""),
        "{page}"
    );
    let (_, login) = Hosted::text(hub.get("/auth/login", None).await).await;
    assert!(
        !login.contains("/auth/provider/nomen"),
        "no issuer, no button: {login}"
    );

    let saved = hub
        .http
        .post(format!("{}/admin/sign-in/nomen", hub.url))
        .header(axum::http::header::COOKIE, &owner)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(&[
            ("issuer", "https://id.example.com"),
            ("label", "Shippin"),
            ("client_id", "hub-client"),
            ("enabled", "on"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), 303);
    let (_, login) = Hosted::text(hub.get("/auth/login", None).await).await;
    assert!(
        login.contains("/auth/provider/nomen") && login.contains(SHIPPIN_NOTICE),
        "{login}"
    );
    assert!(login.contains("Continue with Shippin"), "{login}");

    let off = hub
        .http
        .post(format!("{}/admin/sign-in/nomen", hub.url))
        .header(axum::http::header::COOKIE, &owner)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(&[
            ("issuer", "https://id.example.com"),
            ("label", "Shippin"),
            ("client_id", "hub-client"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(off.status(), 303);
    let (_, login) = Hosted::text(hub.get("/auth/login", None).await).await;
    assert!(
        !login.contains("/auth/provider/nomen"),
        "the switch removes the button: {login}"
    );
    let (_, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert!(
        page.contains("value=\"https://id.example.com\""),
        "the issuer stays: {page}"
    );
}
