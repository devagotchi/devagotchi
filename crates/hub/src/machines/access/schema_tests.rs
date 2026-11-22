//! The hosted role, not an admin connection, proves isolation for every access table.
use super::*;
use crate::{
    config::{TenancyConfig, TenancyMode},
    store,
};

const TABLES: &[&str] = &[
    "machine_access_settings",
    "machine_sandbox_policies",
    "machine_access_requests",
    "machine_grants",
    "machine_admissions",
    "machine_access_notifications",
    "machine_resume_alerts",
];
async fn seed(c: &mut AnyConnection, tenant: &str, account: &str) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO machine_access_settings(tenant_id,version,value) VALUES($1,1,$2::jsonb)",
    )
    .bind(tenant)
    .bind(encoded(&AccessSettings::default())?)
    .execute(&mut *c)
    .await?;
    sqlx::query("INSERT INTO machine_sandbox_policies(tenant_id,scope,allowed,acknowledged,enabled_by,enabled_at,version)
        VALUES($1,'environment',false,'[]',$2,1,1)").bind(tenant).bind(account).execute(&mut *c).await?;
    sqlx::query("INSERT INTO machine_access_requests(tenant_id,id,group_id,account,principal,machine,machine_binding,
        capability,prepared_item,duration,reason,status,created_at,expires_at,version)
        VALUES($1,'r','group',$2,$2,'lab','{}','fs.read','{}','once','Inspect the requested files safely','pending',1,2,1)")
        .bind(tenant).bind(account).execute(&mut *c).await?;
    sqlx::query("INSERT INTO machine_grants(tenant_id,id,account,principal,machine,machine_binding,request_id,capability,
        item,duration,full_access,risk_snapshot,approved_by,approved_at,version)
        VALUES($1,'g',$2,$2,'lab','{}','r','fs.read','{}','once',false,'[]',$2,1,1)")
        .bind(tenant).bind(account).execute(&mut *c).await?;
    sqlx::query("INSERT INTO machine_admissions(tenant_id,id,account,principal,machine,generation,operation_key,input_sha256,
        grant_id,grant_version,effective,state,admitted_at,authority_expires_at)
        VALUES($1,'a',$2,$2,'lab','generation','key',repeat('0',64),'g',1,'{}','admitted',1,2)")
        .bind(tenant).bind(account).execute(&mut *c).await?;
    sqlx::query("INSERT INTO machine_access_notifications(tenant_id,grant_id,version,machine,generation,reason,created_at,next_attempt_at)
        VALUES($1,'g',2,'lab','generation','revoked',1,1)").bind(tenant).execute(&mut *c).await?;
    sqlx::query("INSERT INTO machine_resume_alerts(tenant_id,id,grant_id,person,session_id,reason,shown_at,version)
        VALUES($1,'alert','g',$2,'session','resume',1,1)").bind(tenant).bind(account).execute(c).await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn machine_access_tables_enforce_rls_and_account_environment_foreign_keys() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Accounts::open(test.store.clone())
        .await
        .unwrap()
        .with_tenancy(
            TenancyConfig {
                mode: TenancyMode::Multi,
                ..Default::default()
            },
            Vec::new(),
        );
    let a = accounts
        .start_unsecured("198.51.100.41".parse().unwrap(), "First")
        .await
        .unwrap()
        .0;
    let b = accounts
        .start_unsecured("198.51.100.42".parse().unwrap(), "Second")
        .await
        .unwrap()
        .0;
    assert_ne!(a.tenant, b.tenant);
    assert!(accounts.store.enforced());
    for account in [&a, &b] {
        transaction!(&accounts.store, |c| {
            accounts.store.scope(c, &account.tenant).await?;
            seed(c, &account.tenant, &account.id).await
        })
        .unwrap();
    }
    let mut c = accounts.store.pool().begin().await.unwrap();
    let role: (String, bool, bool) = sqlx::query_as(
        "SELECT current_user::text,rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *c)
    .await
    .unwrap();
    assert_eq!(role, ("devagotchi_hub".into(), false, false));
    accounts.store.scope(&mut c, &a.tenant).await.unwrap();
    for table in TABLES {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(count, 1, "{table}");
        let changed = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET tenant_id=$1 WHERE tenant_id=$2"
        )))
        .bind(&a.tenant)
        .bind(&b.tenant)
        .execute(&mut *c)
        .await
        .unwrap();
        assert_eq!(changed.rows_affected(), 0, "{table}");
        let removed = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE tenant_id=$1"
        )))
        .bind(&b.tenant)
        .execute(&mut *c)
        .await
        .unwrap();
        assert_eq!(removed.rows_affected(), 0, "{table}");
        let mut savepoint = sqlx::Connection::begin(&mut *c).await.unwrap();
        let moved = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET tenant_id=$1 WHERE tenant_id=$2"
        )))
        .bind(&b.tenant)
        .bind(&a.tenant)
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
    let mut savepoint = sqlx::Connection::begin(&mut *c).await.unwrap();
    let foreign =
        sqlx::query("UPDATE machine_access_requests SET account=$1 WHERE tenant_id=$2 AND id='r'")
            .bind(&b.id)
            .bind(&a.tenant)
            .execute(&mut *savepoint)
            .await
            .unwrap_err();
    assert_eq!(
        foreign
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23503")
    );
    savepoint.rollback().await.unwrap();
    c.rollback().await.unwrap();
}
