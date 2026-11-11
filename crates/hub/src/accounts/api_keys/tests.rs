//! API keys against the store and the limits they carry.

use serde_json::json;

use super::*;
use crate::store::testing;

fn new(name: &str, scopes: &[Scope]) -> NewKey {
    NewKey {
        name: name.into(),
        scopes: scopes.to_vec(),
        hours: 24,
        ..NewKey::default()
    }
}

fn args(value: serde_json::Value) -> serde_json::Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn a_key_does_only_what_it_was_given() {
    let limits = KeyLimits {
        id: "k".into(),
        scopes: vec![Scope::BoardsRead, Scope::MachinesRun],
        projects: vec!["alpha".into()],
        machines: vec!["dabo".into()],
    };
    assert_eq!(limits.check("whoami", None), None);
    assert_eq!(
        limits.check("board_list", Some(&args(json!({"project":"alpha"})))),
        None
    );
    assert!(
        limits
            .check("board_list", Some(&args(json!({"project":"beta"}))))
            .unwrap()
            .contains("beta")
    );
    assert!(
        limits
            .check("board_note", Some(&args(json!({"project":"alpha"}))))
            .unwrap()
            .contains("boards.write")
    );
    assert_eq!(
        limits.check("run", Some(&args(json!({"machine":"dabo"})))),
        None
    );
    assert!(
        limits
            .check("run", Some(&args(json!({"machine":"prod"}))))
            .unwrap()
            .contains("prod")
    );
    assert!(
        limits
            .check("operation_start", Some(&args(json!({"broker":"prod"}))))
            .unwrap()
            .contains("prod")
    );
    assert!(
        limits
            .check("write_file", Some(&args(json!({"machine":"dabo"}))))
            .unwrap()
            .contains("files.write")
    );
    assert!(
        limits
            .check("a_tool_from_the_future", None)
            .unwrap()
            .contains("cannot call")
    );
    assert!(
        limits.check("offers_accept", None).is_some(),
        "never for a key"
    );
}

#[test]
fn what_a_person_asks_for_is_checked() {
    let ok = new("hermes", &[Scope::BoardsRead]);
    assert!(ok.check().is_ok());
    for bad in [
        NewKey {
            name: "  ".into(),
            ..ok.clone()
        },
        NewKey {
            name: "x".repeat(61),
            ..ok.clone()
        },
        NewKey {
            scopes: vec![],
            ..ok.clone()
        },
        NewKey {
            hours: 2,
            ..ok.clone()
        },
        NewKey {
            hours: 87_600,
            ..ok.clone()
        },
        NewKey {
            projects: vec!["a\nb".into()],
            ..ok.clone()
        },
    ] {
        assert_eq!(bad.check().unwrap_err().code, "invalid", "{bad:?}");
    }
}

async fn holds(store: crate::store::Store) {
    let accounts = Accounts::open(store.clone()).await.unwrap().with_tenancy(
        crate::config::TenancyConfig {
            mode: crate::config::TenancyMode::Multi,
            ..Default::default()
        },
        Vec::new(),
    );
    let (account, _) = accounts
        .sign_up_password("key@example.com", "a good password")
        .await
        .map(|account| (account, ()))
        .unwrap();
    let tenant = account.tenant.clone();
    let other = accounts
        .sign_up_password("other@example.com", "a good password")
        .await
        .unwrap();

    let (key, secret) = accounts
        .create_key(&tenant, &account.id, &new("hermes", &[Scope::BoardsRead]))
        .await
        .unwrap();
    assert!(secret.starts_with("dgk_"));
    assert_eq!(key.hint, secret[secret.len() - 4..]);

    // The hub keeps a hash, never the key.
    {
        let mut connection = store.admin().acquire().await.unwrap();
        let stored: String = sqlx::query_scalar("SELECT hash FROM api_keys")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_ne!(stored, secret);
        assert_eq!(stored, hash(&secret));
    }

    // It acts for the account, as its AI, named for what the person called it.
    let (subject, limits) = accounts.verify_key(&secret).await.unwrap().unwrap();
    assert_eq!(subject.id, account.id);
    assert_eq!(subject.via, "key");
    assert_eq!(subject.client.as_deref(), Some("hermes"));
    assert_eq!(limits.scopes, [Scope::BoardsRead]);
    let listed = accounts.keys(&tenant, &account.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].last_used_at.is_some(), "last used is recorded");

    // Unknown keys and other prefixes are nothing.
    assert!(accounts.verify_key("dgk_nope").await.unwrap().is_none());
    assert!(accounts.verify_key("dgh_nope").await.unwrap().is_none());

    // Another account's person cannot see or end it.
    assert!(
        accounts
            .keys(&other.tenant, &other.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        accounts
            .revoke_key(&other.tenant, &other.id, &key.id)
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        accounts
            .revoke_key(&tenant, &other.id, &key.id)
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    assert!(accounts.verify_key(&secret).await.unwrap().is_some());

    // Replaced: the old one stops, the new one has the same limits and length.
    let (replaced, fresh) = accounts
        .rotate_key(&tenant, &account.id, &key.id)
        .await
        .unwrap();
    assert_ne!(fresh, secret);
    assert!(accounts.verify_key(&secret).await.unwrap().is_none());
    assert_eq!(replaced.scopes, key.scopes);
    assert_eq!(replaced.expires_at - replaced.created_at, 24 * 3600);

    // Revoked: it stops at once, and cannot be ended twice.
    accounts
        .revoke_key(&tenant, &account.id, &replaced.id)
        .await
        .unwrap();
    assert!(accounts.verify_key(&fresh).await.unwrap().is_none());
    assert_eq!(
        accounts
            .revoke_key(&tenant, &account.id, &replaced.id)
            .await
            .unwrap_err()
            .code,
        "not_found"
    );

    // Past its time it stops.
    let (old, stale) = accounts
        .create_key(&tenant, &account.id, &new("old", &[Scope::BoardsRead]))
        .await
        .unwrap();
    {
        let mut connection = store.admin().acquire().await.unwrap();
        sqlx::query("UPDATE api_keys SET expires_at = 1 WHERE id = $1")
            .bind(&old.id)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    assert!(accounts.verify_key(&stale).await.unwrap().is_none());

    // The account disabled: its keys stop with it.
    let (_, live) = accounts
        .create_key(&tenant, &account.id, &new("live", &[Scope::BoardsRead]))
        .await
        .unwrap();
    assert!(accounts.verify_key(&live).await.unwrap().is_some());
    {
        let mut connection = store.admin().acquire().await.unwrap();
        sqlx::query("UPDATE auth_accounts SET disabled_at = 1 WHERE id = $1")
            .bind(&account.id)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    assert!(accounts.verify_key(&live).await.unwrap().is_none());

    // Every step is on the tenant's chain, and no chain entry holds a key.
    let events = accounts.events(&tenant, 50).await.unwrap();
    let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert!(names.contains(&"api_key.created"), "{names:?}");
    assert!(names.contains(&"api_key.revoked"), "{names:?}");
    assert!(events.iter().all(|e| !e.detail.contains("dgk_")));
}

#[tokio::test]
async fn keys_hold_on_sqlite() {
    let test = testing::sqlite().await;
    holds(test.store.clone()).await;
}

#[tokio::test]
async fn keys_hold_on_postgres() {
    if let Some(test) = testing::postgres().await {
        holds(test.store.clone()).await;
    }
}
