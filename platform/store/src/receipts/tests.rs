use super::*;
use crate::{Store, testing, transaction};
use serde_json::json;

const TENANT: &str = "t-00000000-0000-4000-8000-000000000001";
const OTHER: &str = "t-00000000-0000-4000-8000-000000000002";

fn request(key: &str, op: &str, body: Value) -> Request {
    Request::new(TENANT, "principal", key, op, &body).unwrap()
}

async fn apply(store: &Store, request: &Request, allowed: bool, fail: bool) -> BridgeResult<Value> {
    transaction!(store, |connection| {
        store.scope(connection, &request.tenant).await?;
        // Real adapters must perform their current account/project/policy checks here.
        if !allowed {
            return Err(BridgeError::new("not_granted", "authority was revoked").into());
        }
        match begin_in(connection, request).await? {
            Outcome::Replay(value) => Ok(value),
            Outcome::Fresh(reserved) => {
                let n: i64 = sqlx::query_scalar("UPDATE store_test_effects SET n=n+1 RETURNING n")
                    .fetch_one(&mut *connection)
                    .await?;
                if fail {
                    return Err(BridgeError::new("test", "failed before commit").into());
                }
                let value = json!({"result":n});
                reserved.complete_in(connection, &value).await?;
                Ok(value)
            }
        }
    })
}

#[test]
fn identity_is_bounded_and_json_objects_are_canonical_but_arrays_are_not_reordered() {
    let a = request("key", "edit", json!({"b": [1,2], "a": {"z":1,"x":2}}));
    let b = request(
        "key",
        "edit",
        serde_json::from_str(r#"{"a":{"x":2,"z":1},"b":[1,2]}"#).unwrap(),
    );
    assert_eq!(a.digest, b.digest);
    assert_ne!(
        a.digest,
        request("key", "edit", json!({"b":[2,1],"a":{"z":1,"x":2}})).digest
    );
    assert_ne!(
        a.digest,
        request("key", "claim", json!({"b":[1,2],"a":{"z":1,"x":2}})).digest
    );
    for key in [
        "".into(),
        "a".repeat(201),
        "line\nbreak".into(),
        "é".repeat(101),
    ] {
        assert!(Request::new(TENANT, "principal", &key, "edit", &json!({})).is_err());
    }
    assert!(Request::new("foreign", "principal", "key", "edit", &json!({})).is_err());
    assert!(Request::new(TENANT, "principal", "key", "edit", &"x".repeat(1024 * 1024)).is_err());
}

#[tokio::test]
async fn duplicates_race_once_survive_reopen_and_never_replay_after_revocation_or_retirement() {
    let Some(test) = testing::postgres_with(&[]).await else {
        return;
    };
    let store = &test.store;
    sqlx::raw_sql("CREATE TABLE devagotchi.store_test_effects (n BIGINT NOT NULL); INSERT INTO devagotchi.store_test_effects VALUES (0)")
        .execute(store.admin()).await.unwrap();
    let req = request("same-key", "create", json!({"project":"a","title":"Work"}));
    let second = Store::open(&test.url, &[]).await.unwrap();
    let (one, two) = tokio::join!(
        apply(store, &req, true, false),
        apply(&second, &req, true, false)
    );
    assert_eq!(one.unwrap(), json!({"result":1}));
    assert_eq!(two.unwrap(), json!({"result":1}));
    assert_eq!(
        apply(&second, &req, false, false).await.unwrap_err().code,
        "not_granted"
    );
    assert_eq!(
        apply(
            store,
            &request("same-key", "delete", json!({})),
            true,
            false
        )
        .await
        .unwrap_err()
        .code,
        "request_conflict"
    );
    assert_eq!(
        apply(
            store,
            &request("same-key", "create", json!({"project":"b"})),
            true,
            false
        )
        .await
        .unwrap_err()
        .code,
        "request_conflict"
    );
    let failed = request("rolled-back", "create", json!({}));
    assert_eq!(
        apply(store, &failed, true, true).await.unwrap_err().code,
        "test"
    );
    assert_eq!(
        apply(store, &failed, true, false).await.unwrap(),
        json!({"result":2})
    );
    transaction!(store, |connection| {
        store.scope(connection, TENANT).await?;
        let n = retire_before_in(connection, TENANT, i64::MAX, 1).await?;
        assert_eq!(n, 1);
        assert_eq!(retire_before_in(connection, TENANT, i64::MAX, 1).await?, 1);
        assert_eq!(retire_before_in(connection, TENANT, i64::MAX, 1).await?, 0);
        Ok(())
    })
    .unwrap();
    let reopened = Store::open(&test.url, &[]).await.unwrap();
    assert_eq!(
        apply(&reopened, &req, true, false).await.unwrap_err().code,
        "request_retired"
    );
    let count: i64 = sqlx::query_scalar("SELECT n FROM store_test_effects")
        .fetch_one(store.admin())
        .await
        .unwrap();
    assert_eq!(count, 2);
    // Even a mistaken privileged writer cannot delete or recycle a tombstone.
    assert!(
        sqlx::query("DELETE FROM store_request_receipts WHERE request_key='same-key'")
            .execute(store.admin())
            .await
            .is_err()
    );
    assert!(sqlx::query("UPDATE store_request_receipts SET retired_at=NULL,response='{}' WHERE request_key='same-key'").execute(store.admin()).await.is_err());
}

#[tokio::test]
async fn unfinished_or_oversized_receipts_rollback_and_scopes_and_principals_do_not_collide() {
    let Some(test) = testing::postgres_with(&[]).await else {
        return;
    };
    let store = &test.store;
    let req = request("unfinished", "edit", json!({}));
    sqlx::raw_sql("CREATE TABLE devagotchi.store_test_effects (n BIGINT NOT NULL); INSERT INTO devagotchi.store_test_effects VALUES (0)")
        .execute(store.admin()).await.unwrap();
    let incomplete: BridgeResult<()> = transaction!(store, |connection| {
        store.scope(connection, TENANT).await?;
        let _ = begin_in(connection, &req).await?;
        sqlx::query("UPDATE store_test_effects SET n=n+1")
            .execute(connection)
            .await?;
        Ok(())
    });
    assert!(
        incomplete.is_err(),
        "a pending reservation must never commit"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM store_request_receipts")
        .fetch_one(store.admin())
        .await
        .unwrap();
    assert_eq!(count, 0);
    let effects: i64 = sqlx::query_scalar("SELECT n FROM store_test_effects")
        .fetch_one(store.admin())
        .await
        .unwrap();
    assert_eq!(
        effects, 0,
        "deferred receipt failure also rolls back the domain mutation"
    );
    let oversized: BridgeResult<()> = transaction!(store, |connection| {
        store.scope(connection, TENANT).await?;
        let Outcome::Fresh(reserved) = begin_in(connection, &req).await? else {
            panic!("rolled-back receipt must be fresh")
        };
        reserved
            .complete_in(connection, &"x".repeat(MAX_RESPONSE))
            .await
    });
    assert_eq!(oversized.unwrap_err().code, "invalid");
    for (tenant, principal) in [(TENANT, "a"), (TENANT, "b"), (OTHER, "a")] {
        let req = Request::new(tenant, principal, "same", "edit", &json!({})).unwrap();
        transaction!(store, |connection| {
            store.scope(connection, tenant).await?;
            let Outcome::Fresh(reserved) = begin_in(connection, &req).await? else {
                panic!("distinct scope")
            };
            reserved
                .complete_in(connection, &json!({"principal":principal}))
                .await
        })
        .unwrap();
    }
    let denied: BridgeResult<()> = transaction!(store, |connection| {
        store.scope(connection, OTHER).await?;
        begin_in(connection, &req).await?;
        Ok(())
    });
    assert_eq!(denied.unwrap_err().code, "not_granted");
}

#[test]
fn receipt_migration_bytes_match_the_repository() {
    testing::assert_line_endings(&MIGRATIONS);
}

#[tokio::test]
async fn restricted_role_cannot_read_or_write_another_environment_receipts() {
    let Some(test) = testing::postgres_with(&[]).await else {
        return;
    };
    let store = &test.store;
    sqlx::raw_sql("DO $$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='devagotchi_hub') THEN CREATE ROLE devagotchi_hub NOLOGIN NOSUPERUSER NOBYPASSRLS; END IF; END $$; GRANT USAGE ON SCHEMA devagotchi TO devagotchi_hub; GRANT SELECT,INSERT,UPDATE ON devagotchi.store_request_receipts TO devagotchi_hub")
        .execute(store.admin()).await.unwrap();
    for tenant in [TENANT, OTHER] {
        let req = Request::new(tenant, "principal", "key", "edit", &json!({})).unwrap();
        transaction!(store, |connection| {
            store.scope(connection, tenant).await?;
            let Outcome::Fresh(reserved) = begin_in(connection, &req).await? else {
                panic!("fresh")
            };
            reserved
                .complete_in(connection, &json!({"environment":tenant}))
                .await
        })
        .unwrap();
    }
    let restricted = Store::open(&test.url, &[]).await.unwrap();
    assert!(restricted.enforced());
    transaction!(&restricted, |connection| {
        restricted.scope(connection, TENANT).await?;
        let tenants: Vec<String> =
            sqlx::query_scalar("SELECT tenant_id FROM store_request_receipts")
                .fetch_all(&mut *connection)
                .await?;
        assert_eq!(tenants, [TENANT]);
        Ok(())
    })
    .unwrap();
    let wrong_scope: BridgeResult<()> = transaction!(&restricted, |connection| {
        restricted.scope(connection, TENANT).await?;
        sqlx::query("INSERT INTO store_request_receipts (tenant_id,principal,request_key,operation,request_hash,response,completed_at) VALUES ($1,'principal','foreign','edit','hash','{}',1)")
            .bind(OTHER).execute(connection).await?;
        Ok(())
    });
    assert!(wrong_scope.is_err());
    // Deferred completion must inspect the original environment even when a caller
    // changes its RLS scope before COMMIT. The trigger owner is the migration role.
    let hidden_pending: BridgeResult<()> = transaction!(&restricted, |connection| {
        restricted.scope(connection, TENANT).await?;
        begin_in(connection, &request("hidden-pending", "edit", json!({}))).await?;
        restricted.scope(connection, OTHER).await?;
        Ok(())
    });
    assert!(
        hidden_pending.is_err(),
        "changing scope cannot commit an incomplete receipt"
    );
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM store_request_receipts WHERE request_key='hidden-pending'",
    )
    .fetch_one(store.admin())
    .await
    .unwrap();
    assert_eq!(pending, 0);
}
