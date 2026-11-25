//! A successful issuance must still show its one-time secret if the list read fails.

use crate::tenancy::tests::tenants::Hosted;
use crate::web::app::tests::{hosted, page};

fn secret(html: &str) -> &str {
    html.split_once("id=\"key-secret\">")
        .unwrap()
        .1
        .split_once("</code>")
        .unwrap()
        .0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoke_form_replays_without_secret_storage_and_rechecks_live_account() {
    use crate::identity::accounts::api_keys::{NewKey, Scope};
    let hub = or_skip!(hosted());
    let person = hub.first_connect("revocation test").await;
    let subject = hub
        .accounts
        .verify_access(&person.token)
        .await
        .unwrap()
        .unwrap();
    let account = subject.account;
    let input = NewKey {
        name: "Revocation".into(),
        scopes: vec![Scope::BoardsRead],
        hours: 24,
        ..Default::default()
    };
    let (key, secret) = hub
        .accounts
        .create_key(&account.tenant, &account.id, &input)
        .await
        .unwrap();
    let (_, html) = page(&hub, "/app/keys", &person).await;
    let request = html
        .split_once("name=\"request_id\" value=\"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0;
    let path = format!("/app/keys/{}/revoke", key.id);
    for _ in 0..2 {
        assert_eq!(
            hub.form(&path, Some(&person.cookie), &[("request_id", request)])
                .await
                .status(),
            303
        );
    }
    let replay = hub
        .form(&path, Some(&person.cookie), &[("request_id", request)])
        .await;
    assert_eq!(replay.headers()["idempotent-replayed"], "true");
    assert!(hub.accounts.verify_key(&secret).await.unwrap().is_none());
    let responses: Vec<String> = sqlx::query_scalar(
        "SELECT response FROM store_request_receipts WHERE tenant_id=$1 AND operation='revoke_key'",
    )
    .bind(&account.tenant)
    .fetch_all(hub.accounts.store().admin())
    .await
    .unwrap();
    assert_eq!(responses, ["null"]);
    let (other, other_secret) = hub
        .accounts
        .create_key(&account.tenant, &account.id, &input)
        .await
        .unwrap();
    assert_eq!(
        hub.accounts
            .revoke_key_requested(&account.tenant, &account.id, &other.id, Some(request))
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    assert!(
        hub.accounts
            .verify_key(&other_secret)
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("UPDATE auth_accounts SET disabled_at=1 WHERE id=$1")
        .bind(&account.id)
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(
        hub.accounts
            .revoke_key_requested(&account.tenant, &account.id, &key.id, Some(request))
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
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
