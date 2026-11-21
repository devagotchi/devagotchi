use super::*;
use crate::{
    config::{TenancyConfig, TenancyMode},
    store,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_rls_scopes_all_eight_types_and_seed_marks_in_postgres() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(
        Accounts::open(test.store.clone())
            .await
            .unwrap()
            .with_tenancy(
                TenancyConfig {
                    mode: TenancyMode::Multi,
                    ..Default::default()
                },
                Vec::new(),
            ),
    );
    let a = accounts
        .start_unsecured("198.51.100.31".parse().unwrap(), "First")
        .await
        .unwrap()
        .0;
    let b = accounts
        .start_unsecured("198.51.100.32".parse().unwrap(), "Second")
        .await
        .unwrap()
        .0;
    let catalog = Catalog::new(accounts.clone());
    catalog.list(&a.id, Kind::Capabilities, None).await.unwrap();
    catalog.list(&b.id, Kind::Capabilities, None).await.unwrap();
    assert_ne!(a.tenant, b.tenant);
    assert!(accounts.store().enforced());
    let mut c = accounts.store().pool().begin().await.unwrap();
    let role: (String, bool, bool) = sqlx::query_as(
        "SELECT current_user::text,rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *c)
    .await
    .unwrap();
    assert_eq!(role, ("devagotchi_hub".into(), false, false));
    let tables = Kind::ALL
        .iter()
        .map(|k| k.table())
        .chain(["catalog_seed_marks"]);
    for table in tables.clone() {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    accounts.store().scope(&mut c, &a.tenant).await.unwrap();
    for table in tables {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert!(count > 0, "{table}");
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE tenant_id=$1"
        )))
        .bind(&b.tenant)
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(count, 0, "{table}");
        let changed = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE tenant_id=$1"
        )))
        .bind(&b.tenant)
        .execute(&mut *c)
        .await
        .unwrap();
        assert_eq!(changed.rows_affected(), 0);
        let mut savepoint = sqlx::Connection::begin(&mut *c).await.unwrap();
        let moved = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET tenant_id=$1"
        )))
        .bind(&b.tenant)
        .execute(&mut *savepoint)
        .await
        .unwrap_err();
        assert_eq!(
            moved.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501"),
            "{table}: {moved}"
        );
        savepoint.rollback().await.unwrap();
    }
    c.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_audit_receipt_and_seed_changes_commit_together_and_owner_is_live() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(Accounts::open(test.store.clone()).await.unwrap());
    let owner = accounts
        .ensure_owner(None, "Owner", [74; 16])
        .await
        .unwrap();
    let catalog = Catalog::new(accounts.clone());
    let fields = json!({"label":"Changed seed"});
    let made = catalog
        .put(
            &owner.id,
            Kind::Capabilities,
            "fs.read",
            1,
            &fields,
            Some("seed-edit"),
        )
        .await
        .unwrap();
    assert_eq!(made.result.version, 2);
    let count = || {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event LIKE 'catalog.%'",
        )
        .bind(&owner.tenant)
        .fetch_one(accounts.store().admin())
    };
    let events = count().await.unwrap();
    assert_eq!(events, 1);
    assert!(
        catalog
            .put(
                &owner.id,
                Kind::Capabilities,
                "fs.read",
                1,
                &fields,
                Some("seed-edit")
            )
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(count().await.unwrap(), events);
    assert!(
        catalog
            .put(
                &owner.id,
                Kind::Capabilities,
                "fs.read",
                2,
                &json!({"label":3}),
                Some("correctable")
            )
            .await
            .is_err()
    );
    assert_eq!(count().await.unwrap(), events);
    assert_eq!(
        catalog
            .put(
                &owner.id,
                Kind::Capabilities,
                "fs.read",
                2,
                &json!({"label":"Corrected"}),
                Some("correctable")
            )
            .await
            .unwrap()
            .result
            .version,
        3
    );
    // Re-run only the grant seed after an operator-selected default: preserve it.
    catalog
        .choose_default(&owner.id, "read-only-look", 1, None)
        .await
        .unwrap();
    catalog
        .remove(&owner.id, Kind::GrantPresets, "task-scoped", 2, None)
        .await
        .unwrap();
    sqlx::query("DELETE FROM catalog_seed_marks WHERE tenant_id=$1 AND name='grant-presets'")
        .bind(&owner.tenant)
        .execute(accounts.store().admin())
        .await
        .unwrap();
    let rows = catalog
        .list(&owner.id, Kind::GrantPresets, None)
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .filter(|r| r.is_default == Some(true))
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["read-only-look"]
    );
    // A new catalog/Accounts instance must read persisted edits, not restore defaults.
    let reopened = Catalog::new(Arc::new(
        Accounts::open(store::open(&test.url).await.unwrap())
            .await
            .unwrap(),
    ));
    assert_eq!(
        reopened
            .get(&owner.id, Kind::Capabilities, "fs.read")
            .await
            .unwrap()
            .data["label"],
        "Corrected"
    );
    sqlx::query("UPDATE auth_accounts SET owner=0 WHERE id=$1")
        .bind(&owner.id)
        .execute(accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(
        catalog
            .put(
                &owner.id,
                Kind::Capabilities,
                "fs.read",
                1,
                &fields,
                Some("seed-edit")
            )
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
}
