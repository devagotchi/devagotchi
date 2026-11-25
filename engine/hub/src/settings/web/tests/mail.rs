//! `/admin` → Email over HTTP (`settings/web/mail.rs`): a mail server is checked before it is
//! saved, saved with its password sealed, in force at once, and a test reaches the inbox; one
//! the config file names is shown, not changed. On the test Postgres and the stub mail server.

use serde_json::json;

use crate::tenancy::web::tests::operator::{hub_on, operator_cookie};
use crate::{
    notify::mail::stub,
    tenancy::tests::tenants::{Hosted, hosted_with},
};

fn mail(hub: &Hosted, cookie: &str, fields: &[(&str, &str)]) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}/admin/mail", hub.url))
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

async fn mail_configured(hub: &Hosted) -> bool {
    let (_, health) = Hosted::text(hub.get("/healthz", None).await).await;
    let health: serde_json::Value = serde_json::from_str(&health).unwrap();
    health["mail"]["configured"] == true
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mail_server_saved_on_the_page_is_checked_sealed_and_sends_at_once() {
    let store = or_skip!(crate::store::testing::empty_postgres());
    let hub = or_skip!(hub_on(Some(&store), None));
    let owner = operator_cookie(&hub).await;
    let smtp = stub::start("hub").await;

    let (status, page) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("id=\"mail\""), "{page}");
    assert!(page.contains("smtp.resend.com"), "{page}");
    assert!(!mail_configured(&hub).await);

    // Refused, and nothing saved: a password in the address, a server that does not answer.
    let base = |url: &'static str| {
        vec![
            ("smtp_url", url),
            ("password", "mail-pass"),
            ("from", "Hub <hub@hub.test>"),
            ("per_address_per_hour", "5"),
            ("enabled", "on"),
        ]
    };
    let carried = mail(
        &hub,
        &owner,
        &base("smtp://hub:secret@127.0.0.1:25?tls=none"),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(carried.status(), 400);
    let closed = mail(&hub, &owner, &base("smtp://hub@127.0.0.1:9?tls=none"))
        .send()
        .await
        .unwrap();
    assert_eq!(closed.status(), 400);
    let (_, why) = Hosted::text(closed).await;
    assert!(why.contains("nothing was saved"), "{why}");
    assert!(hub.accounts.stored_mail().await.unwrap().is_none());

    // Checked, saved, in force at once; the test reaches the inbox.
    let mut fields = vec![
        ("smtp_url", smtp.url.as_str()),
        ("password", "mail-pass"),
        ("from", "Hub <hub@hub.test>"),
        ("per_address_per_hour", "5"),
        ("enabled", "on"),
        ("test_to", "me@example.test"),
    ];
    let saved = mail(&hub, &owner, &fields).send().await.unwrap();
    assert_eq!(saved.status(), 303);
    assert!(mail_configured(&hub).await);
    let received = smtp.received.lock().unwrap().clone();
    assert_eq!(received.len(), 1, "one test, sign-in sent nothing");
    assert_eq!(received[0].to, "me@example.test");
    assert!(
        received[0].data.contains("mail check"),
        "{}",
        received[0].data
    );

    // The password is sealed: nowhere in the rows or the audit as given.
    let stored = hub.accounts.stored_mail().await.unwrap().unwrap();
    assert!(!stored.sealed_password.is_empty());
    assert_ne!(stored.sealed_password, "mail-pass");
    for value in hub.accounts.hub_settings().await.unwrap().values() {
        assert!(!value.contains("mail-pass"), "{value}");
    }
    let home = hub.accounts.home_tenant().await.unwrap();
    let configured: Vec<String> = hub
        .accounts
        .events(&home, 50)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.event == "mail.configured")
        .map(|event| event.detail)
        .collect();
    assert_eq!(configured.len(), 1, "{configured:?}");
    assert!(!configured[0].contains("mail-pass"));
    assert!(!configured[0].contains(&stored.sealed_password));

    // Saved again with no password: the sealed one is kept. Turned off: no mail at all.
    fields.retain(|(name, _)| *name != "test_to");
    fields[1].1 = "";
    assert_eq!(
        mail(&hub, &owner, &fields).send().await.unwrap().status(),
        303
    );
    let kept = hub.accounts.stored_mail().await.unwrap().unwrap();
    assert_eq!(kept.sealed_password, stored.sealed_password);
    fields.retain(|(name, _)| *name != "enabled");
    assert_eq!(
        mail(&hub, &owner, &fields).send().await.unwrap().status(),
        303
    );
    assert!(!mail_configured(&hub).await);
    assert!(!hub.accounts.stored_mail().await.unwrap().unwrap().enabled);

    // Not the owner: nothing there.
    let other = hub.first_connect("Claude").await;
    let denied = mail(&hub, &other.cookie, &fields).send().await.unwrap();
    assert_eq!(denied.status(), 404);

    // A mail server the config file names is shown, not changed.
    let fresh = crate::store::testing::empty_postgres().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let password = dir.path().join("smtp-password");
    std::fs::write(&password, "file-pass\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&password, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let url = smtp.url.clone();
    let pinned = or_skip!(hosted_with(Some(&fresh), move |_, _| {
        json!({
            "operators": ["github:42"],
            "auth": { "email": { "smtp_url": url, "password_file": password.to_string_lossy(),
                                 "from": "Hub <hub@hub.test>" } }
        })
    }));
    let owner = operator_cookie(&pinned).await;
    let (_, page) = Hosted::text(pinned.get("/admin", Some(&owner)).await).await;
    assert!(page.contains("auth.email"), "{page}");
    let refused = mail(&pinned, &owner, &[("smtp_url", smtp.url.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 409);
}
