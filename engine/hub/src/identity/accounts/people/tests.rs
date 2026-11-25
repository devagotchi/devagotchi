use super::*;
use crate::identity::accounts::api_keys::{NewKey, Scope};

async fn setup() -> Option<(Accounts, Account)> {
    let test = crate::store::testing::postgres().await?;
    let accounts = Accounts::open(test.store).await.unwrap();
    let owner = accounts
        .ensure_owner(None, "Owner", [63; 16])
        .await
        .unwrap();
    Some((accounts, owner))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_users_are_atomic_scoped_owner_managed_and_versioned() {
    let (accounts, owner) = or_skip!(setup());
    assert!(
        accounts
            .create_local_user(&owner.id, "Bad", "not an address", "password-one")
            .await
            .is_err()
    );
    assert!(
        accounts
            .create_local_user(&owner.id, "Bad", "bad@example.test", "short")
            .await
            .is_err()
    );
    assert_eq!(
        accounts.local_users(&owner.id, None).await.unwrap().len(),
        1
    );
    let (a, b) = tokio::join!(
        accounts.create_local_user(&owner.id, "Alice", "alice@example.test", "password-one"),
        accounts.create_local_user(&owner.id, "Alice", "alice@example.test", "password-one")
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let user = a.or(b).unwrap();
    assert_eq!(user.account.tenant, owner.tenant);
    assert!(!user.account.owner);
    assert_eq!(
        accounts
            .sign_in_password("alice@example.test", "password-one")
            .await
            .unwrap()
            .id,
        user.account.id
    );
    assert_eq!(
        accounts.local_users(&owner.id, None).await.unwrap().len(),
        2
    );
    assert_eq!(
        accounts
            .local_users(&user.account.id, None)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    assert!(
        accounts
            .create_local_user(&user.account.id, "Eve", "eve@example.test", "password-one")
            .await
            .is_err()
    );
    let (a, b) = tokio::join!(
        accounts.update_local_user(&owner.id, &user.account.id, 1, "Alice A", true),
        accounts.update_local_user(&owner.id, &user.account.id, 1, "Alice B", true)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let (changed, conflict) = if a.is_ok() {
        (a.unwrap(), b.unwrap_err())
    } else {
        (b.unwrap(), a.unwrap_err())
    };
    assert_eq!(conflict.code, "version_conflict");
    assert_eq!(changed.version, 2);
    assert_eq!(changed.account.id, user.account.id);
    assert_eq!(
        accounts
            .update_local_user(&owner.id, &owner.id, 1, "Owner", false)
            .await
            .unwrap_err()
            .code,
        "owner_protected"
    );
    assert!(
        accounts
            .get(&owner.id)
            .await
            .unwrap()
            .unwrap()
            .disabled_at
            .is_none()
    );
    let hosted = Accounts::open(accounts.store.clone())
        .await
        .unwrap()
        .with_tenancy(
            crate::config::TenancyConfig {
                mode: TenancyMode::Multi,
                ..Default::default()
            },
            Vec::new(),
        );
    let elsewhere = hosted
        .sign_up_password("elsewhere@example.test", "password-one")
        .await
        .unwrap();
    assert_ne!(elsewhere.tenant, owner.tenant);
    assert_eq!(
        accounts
            .local_user(&owner.id, &elsewhere.id)
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        hosted.local_users(&owner.id, None).await.unwrap_err().code,
        "not_available"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabling_and_resetting_end_access_without_losing_the_user_or_revealing_passwords() {
    let (accounts, owner) = or_skip!(setup());
    let user = accounts
        .create_local_user(&owner.id, "Alice", "alice@example.test", "password-one")
        .await
        .unwrap();
    let id = &user.account.id;
    let session = accounts.open_session(id, None).await.unwrap();
    let (_, key) = accounts
        .create_key(
            &owner.tenant,
            id,
            &NewKey {
                name: "Agent".into(),
                scopes: vec![Scope::BoardsRead],
                hours: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let redirect = "http://127.0.0.1/callback";
    let resource = "http://127.0.0.1/mcp";
    let verifier = "a".repeat(43);
    let client = accounts
        .register_client("Agent", &[redirect.into()])
        .await
        .unwrap();
    let code = accounts
        .issue_code(
            &client.id,
            id,
            redirect,
            &oauth::s256(&verifier),
            resource,
            "mcp",
        )
        .await
        .unwrap();
    let tokens = accounts
        .exchange_code(&code, &client.id, redirect, &verifier, Some(resource))
        .await
        .unwrap();
    let pending = accounts
        .issue_code(
            &client.id,
            id,
            redirect,
            &oauth::s256(&verifier),
            resource,
            "mcp",
        )
        .await
        .unwrap();
    assert!(
        accounts
            .verify_access(&tokens.access)
            .await
            .unwrap()
            .is_some()
    );
    assert!(accounts.verify_key(&key).await.unwrap().is_some());
    let device = accounts
        .start_device(&client.id, "mcp", resource)
        .await
        .unwrap();
    accounts
        .decide_device(&device.user_code, id, true)
        .await
        .unwrap();

    let disabled = accounts
        .update_local_user(&owner.id, id, 1, "Alice", false)
        .await
        .unwrap();
    assert!(disabled.account.disabled_at.is_some());
    assert!(accounts.session(&session).await.unwrap().is_none());
    assert!(
        accounts
            .verify_access(&tokens.access)
            .await
            .unwrap()
            .is_none()
    );
    assert!(accounts.verify_key(&key).await.unwrap().is_none());
    assert!(
        accounts
            .sign_in_password("alice@example.test", "password-one")
            .await
            .is_err()
    );
    let enabled = accounts
        .update_local_user(&owner.id, id, 2, "Alice", true)
        .await
        .unwrap();
    assert!(enabled.account.disabled_at.is_none());
    assert!(
        accounts
            .verify_access(&tokens.access)
            .await
            .unwrap()
            .is_none()
    );
    assert!(accounts.verify_key(&key).await.unwrap().is_none());
    assert!(
        accounts
            .exchange_code(&pending, &client.id, redirect, &verifier, Some(resource))
            .await
            .is_err()
    );
    assert_eq!(
        accounts
            .poll_device(&device.device_code, &client.id)
            .await
            .unwrap(),
        oauth::DevicePoll::Waiting("invalid_grant")
    );
    let fresh = accounts
        .sign_in_password("alice@example.test", "password-one")
        .await
        .unwrap();
    assert_eq!(&fresh.id, id);
    let session = accounts.open_session(id, None).await.unwrap();
    let (_, independent_key) = accounts
        .create_key(
            &owner.tenant,
            id,
            &NewKey {
                name: "Independent".into(),
                scopes: vec![Scope::BoardsRead],
                hours: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let code = accounts
        .issue_code(
            &client.id,
            id,
            redirect,
            &oauth::s256(&verifier),
            resource,
            "mcp",
        )
        .await
        .unwrap();
    let before_reset = accounts
        .exchange_code(&code, &client.id, redirect, &verifier, Some(resource))
        .await
        .unwrap();
    let reset = accounts
        .reset_local_password(&owner.id, id, 3, "password-two")
        .await
        .unwrap();
    assert_eq!(reset.version, 4);
    assert!(
        accounts
            .verify_key(&independent_key)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        accounts
            .verify_access(&before_reset.access)
            .await
            .unwrap()
            .is_none()
    );

    assert!(accounts.session(&session).await.unwrap().is_none());
    assert!(
        accounts
            .sign_in_password("alice@example.test", "password-one")
            .await
            .is_err()
    );
    assert_eq!(
        &accounts
            .sign_in_password("alice@example.test", "password-two")
            .await
            .unwrap()
            .id,
        id
    );
    let details: Vec<String> = sqlx::query_scalar("SELECT detail FROM devagotchi.tenancy_events")
        .fetch_all(accounts.store.admin())
        .await
        .unwrap();
    assert!(details.iter().all(|s| !s.contains("password-one")
        && !s.contains("password-two")
        && !s.contains("argon2")));
}
