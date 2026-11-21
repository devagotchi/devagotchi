//! A successful issuance must still show its one-time secret if the list read fails.

use super::tests::{hosted, page};
use crate::tenancy::tests::tenants::Hosted;

fn secret(html: &str) -> &str {
    html.split_once("id=\"key-secret\">")
        .unwrap()
        .1
        .split_once("</code>")
        .unwrap()
        .0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn issued_keys_survive_metadata_failure_but_are_never_replayed() {
    let hub = or_skip!(hosted());
    let person = hub.first_connect("key test").await;
    let fields = [("name", "first"), ("scope", "boards.read"), ("hours", "24")];
    let (status, html) =
        Hosted::text(hub.form("/app/keys", Some(&person.cookie), &fields).await).await;
    assert_eq!(status, 200, "{html}");
    let first = secret(&html).to_string();
    let (subject, limits) = hub.accounts.verify_key(&first).await.unwrap().unwrap();
    let tenant = subject.tenant.as_deref().unwrap();
    sqlx::query("UPDATE auth_api_keys SET projects = 'malformed' WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(&limits.id)
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(page(&hub, "/app/keys", &person).await.0, 503);

    let response = hub.form("/app/keys", Some(&person.cookie), &fields).await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let (status, html) = Hosted::text(response).await;
    assert_eq!(status, 200, "{html}");
    assert!(html.contains("key list could not be loaded"));
    assert!(!html.contains("No keys yet."));
    let second = secret(&html).to_string();
    let (_, second_limits) = hub.accounts.verify_key(&second).await.unwrap().unwrap();

    let path = format!("/app/keys/{}/rotate", second_limits.id);
    let response = hub.form(&path, Some(&person.cookie), &[]).await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let (status, html) = Hosted::text(response).await;
    assert_eq!(status, 200, "{html}");
    assert!(html.contains("key list could not be loaded"));
    let third = secret(&html).to_string();
    assert!(hub.accounts.verify_key(&second).await.unwrap().is_none());
    assert!(hub.accounts.verify_key(&third).await.unwrap().is_some());
    let (status, html) = page(&hub, "/app/keys", &person).await;
    assert_eq!(status, 503);
    assert!(!html.contains(&third));

    sqlx::query("UPDATE auth_api_keys SET projects = '[]' WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(&limits.id)
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    let (status, html) = page(&hub, "/app/keys", &person).await;
    assert_eq!(status, 200, "{html}");
    for key in [&first, &second, &third] {
        assert!(!html.contains(key), "GET must never show a secret");
    }
    assert_eq!(
        hub.accounts.keys(tenant, &subject.id).await.unwrap().len(),
        3
    );
    assert!(
        hub.accounts
            .events(tenant, 100)
            .await
            .unwrap()
            .iter()
            .all(|event| !event.detail.contains("dgk_"))
    );
}
