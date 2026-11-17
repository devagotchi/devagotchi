//! A hosted hub's accounts end to end: it offers no claim, a terminal signs in to a new
//! account with one approve, and an account adds machines only up to its quota.

use axum::http::header;
use serde_json::{Value, json};

use super::tenants::{Hosted, Person, hosted, refused, set_cookie};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_hub_offers_no_claim_and_says_so_everywhere() {
    let hub = or_skip!(hosted());
    let (status, page) = Hosted::text(hub.get("/", None).await).await;
    assert_eq!(status, 200);
    assert!(
        page.contains("Get started") && !page.contains("Claim this hub"),
        "{page}"
    );
    let (status, page) = Hosted::text(hub.get("/claim", None).await).await;
    assert_eq!(status, 404);
    assert!(page.contains("Nothing to claim here"), "{page}");
    let refused = hub
        .http
        .post(format!("{}/claim", hub.url))
        .header(header::ORIGIN, &hub.url)
        .json(&json!({ "code": "0000-0000-0000-0000-0000-0000" }))
        .send()
        .await
        .unwrap();
    assert_ne!(refused.status(), 200);
    // Signed in to a new account: every page says it is not secured, and the account page
    // offers the passkey without being in the way.
    let mut person = hub.first_connect("Claude").await;
    let (_, page) = Hosted::text(hub.get("/approvals", Some(&person.cookie)).await).await;
    assert!(page.contains("class=\"nudge\""), "{page}");
    let (status, page) = Hosted::text(hub.get("/account", Some(&person.cookie)).await).await;
    assert_eq!(status, 200);
    assert!(
        page.contains("id=\"secure\"") && page.contains("id=\"passkey-add\""),
        "{page}"
    );
    assert!(page.contains("/assets/passkey.js"), "{page}");
    assert!(
        page.contains("account.created") && page.contains("client.connected"),
        "{page}"
    );
    // whoami says the same to the AI.
    let who = hub.call(&mut person, "whoami", json!({})).await["structuredContent"].clone();
    assert!(
        who["secure_url"]
            .as_str()
            .unwrap()
            .ends_with("/account#secure")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_terminal_signs_in_to_a_new_account_with_one_approve() {
    let hub = or_skip!(hosted());
    let started: Value = hub
        .http
        .post(format!("{}/oauth/device", hub.url))
        .form(&[("client_id", "devagotchi-cli")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let user_code = started["user_code"].as_str().unwrap();
    let (status, page) =
        Hosted::text(hub.get(&format!("/device?code={user_code}"), None).await).await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("your new Devagotchi account"), "{page}");
    let answer = hub
        .form(
            "/device",
            None,
            &[("code", user_code), ("decision", "approve")],
        )
        .await;
    let cookie = set_cookie(&answer, "dg_session").expect("the new account's session");
    let (status, page) = Hosted::text(answer).await;
    assert_eq!(status, 200);
    assert!(page.contains("Your terminal is signed in"), "{page}");
    assert!(
        page.contains("Secure my account"),
        "the offer, not in the way: {page}"
    );
    let tokens: Value = hub
        .http
        .post(format!("{}/oauth/token", hub.url))
        .form(&[
            ("grant_type", crate::web::oauth::DEVICE_GRANT),
            ("device_code", started["device_code"].as_str().unwrap()),
            ("client_id", "devagotchi-cli"),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut person = Person {
        cookie,
        token: tokens["access_token"].as_str().expect("tokens").to_string(),
        session: None,
    };
    let who = hub.call(&mut person, "whoami", json!({})).await["structuredContent"].clone();
    assert!(who["tenant"].as_str().unwrap().starts_with("t-"));
    // The browser that approved is that account's.
    let (_, page) = Hosted::text(hub.get("/account", Some(&person.cookie)).await).await;
    assert!(page.contains(who["subject"].as_str().unwrap()), "{page}");
    assert!(page.contains("terminal.connected"), "{page}");
}

#[tokio::test]
async fn an_unsecured_account_adds_machines_only_up_to_its_quota() {
    let hub = or_skip!(hosted());
    let mut person = hub.first_connect("Claude").await;
    for name in ["one", "two"] {
        let added = hub
            .call(&mut person, "machine_add", json!({ "name": name }))
            .await;
        assert!(!refused(&added), "{added}");
    }
    let third = hub
        .call(&mut person, "machine_add", json!({ "name": "three" }))
        .await;
    assert!(
        refused(&third) && third.to_string().contains("machine_limit"),
        "{third}"
    );
}
