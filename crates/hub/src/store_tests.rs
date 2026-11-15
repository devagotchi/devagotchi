use sqlx::Row;

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::store::{testing::*, *};

#[test]
fn a_configured_url_is_postgres_and_never_carries_a_password() {
    for good in [
        "postgres://hub@db.internal:5432/devagotchi".to_string(),
        "postgresql://hub@10.40.0.30/devagotchi?sslmode=require".to_string(),
        "postgres:///devagotchi?host=/run/postgresql".to_string(),
    ] {
        assert_eq!(check_configured_url(&good), Ok(()), "{good}");
    }
    for bad in [
        "postgres://hub:hunter2@db.internal/devagotchi",
        "postgresql://hub:x@10.40.0.30/devagotchi",
        "sqlite:relative/accounts.sqlite3",
        "sqlite:/var/lib/devagotchi/accounts.sqlite3",
        "mysql://hub@db/devagotchi",
        "",
    ] {
        let refused = check_configured_url(bad).unwrap_err();
        assert!(!refused.contains("hunter2"), "{refused}");
    }
}

#[test]
fn without_a_url_there_is_no_store_and_the_configs_is_used_as_written() {
    if std::env::var(STORAGE_URL_ENV).is_ok() {
        return; // the environment names the store; nothing else is consulted
    }
    assert_eq!(url_for(None), None);
    assert_eq!(
        url_for(Some("postgres://hub@db/devagotchi")).as_deref(),
        Some("postgres://hub@db/devagotchi")
    );
}

#[tokio::test]
async fn a_store_never_prints_its_url() {
    for test in stores().await {
        let shown = format!("{:?}", test.store);
        assert!(!shown.contains("://"), "{shown}");
    }
}

#[tokio::test]
async fn writers_racing_on_one_row_each_count_once() {
    // Read, add one, write back: eight at once. Postgres runs them side by side, refuses the
    // ones that collided, and they run again: the count is eight.
    for test in stores().await {
        let store = test.store.clone();
        sqlx::raw_sql(
            "CREATE TABLE counter (id BIGINT PRIMARY KEY, n BIGINT NOT NULL);
             INSERT INTO counter (id, n) VALUES (1, 0);",
        )
        .execute(store.admin())
        .await
        .unwrap();
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                transaction!(&store, |connection| {
                    let n: i64 = sqlx::query_scalar("SELECT n FROM counter WHERE id = 1")
                        .fetch_one(&mut *connection)
                        .await?;
                    tokio::task::yield_now().await;
                    sqlx::query("UPDATE counter SET n = $1 WHERE id = 1")
                        .bind(n + 1)
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            }));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        let n: i64 = sqlx::query_scalar("SELECT n FROM counter WHERE id = 1")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(n, 8);
    }
}

#[tokio::test]
async fn a_refusal_rolls_back_what_the_transaction_did() {
    for test in stores().await {
        let store = test.store;
        sqlx::raw_sql("CREATE TABLE notes (body TEXT NOT NULL)")
            .execute(store.admin())
            .await
            .unwrap();
        let refused: BridgeResult<()> = transaction!(&store, |connection| {
            sqlx::query("INSERT INTO notes (body) VALUES ('written')")
                .execute(&mut *connection)
                .await?;
            Err(BridgeError::new("refused", "not this one").into())
        });
        assert_eq!(refused.unwrap_err().code, "refused");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notes")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn schema_names_need_no_quoting_and_are_never_postgress_own() {
    for good in ["devagotchi_hub", "hub", "t_0123"] {
        assert!(valid_schema(good), "{good}");
    }
    for bad in [
        "",
        "Hub",
        "pg_catalog",
        "9lives",
        "hub; drop table accounts",
        "hub\"",
        &"a".repeat(64),
    ] {
        assert!(!valid_schema(bad), "{bad:?}");
    }
}

#[tokio::test]
async fn on_postgres_the_hubs_tables_live_in_their_own_schema() {
    // A database something else also uses (a Supabase project): the hub's tables go in the
    // `devagotchi` schema under their prefixed names, its migration record in the schema
    // configured (made if missing); `public` stays as it was.
    let url = or_skip!(empty_postgres());
    let store = Store::open_in(&url, Some("devagotchi_hub")).await.unwrap();
    let mut placed = std::collections::BTreeMap::<String, Vec<String>>::new();
    let rows = sqlx::query(
        "SELECT table_schema::text, table_name::text FROM information_schema.tables
         WHERE table_schema IN ('public', 'devagotchi_hub', 'devagotchi') ORDER BY 1, 2",
    )
    .fetch_all(store.admin())
    .await
    .unwrap();
    for row in rows {
        placed
            .entry(row.get::<String, _>(0))
            .or_default()
            .push(row.get::<String, _>(1));
    }
    assert!(!placed.contains_key("public"), "{placed:?}");
    assert_eq!(placed["devagotchi_hub"], ["_sqlx_migrations"], "{placed:?}");
    let hub = &placed["devagotchi"];
    for table in [
        "auth_accounts",
        "auth_passkeys",
        "auth_sessions",
        "auth_identities",
        "oauth_tokens",
        "tenancy_tenants",
        "tenancy_events",
        "hub_secrets",
    ] {
        assert!(hub.iter().any(|t| t == table), "{table} in {hub:?}");
    }
    // Opened again, as a second hub instance would: the schema and its tables are found.
    let again = Store::open_in(&url, Some("devagotchi_hub")).await.unwrap();
    let accounts = crate::accounts::Accounts::open(again).await.unwrap();
    accounts
        .create("acct_aaaaaaaaaaaaaaaaaaaaaaaaaa", "Jesse", true)
        .await
        .unwrap();
    assert!(accounts.owner().await.unwrap().is_some());
    // A name that is not a plain schema name is refused before anything runs.
    assert_eq!(
        Store::open_in(&url, Some("hub; drop schema public"))
            .await
            .unwrap_err()
            .code,
        "account_store"
    );
}

#[tokio::test]
async fn a_store_a_newer_hub_migrated_still_opens() {
    // The rollback case: a newer image applied a migration this build does not have, and
    // was then taken back out. Migrations only add, so this build runs on that schema.
    for store in testing::stores().await {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(9_999_i64)
        .bind("a newer hub's")
        .bind(true)
        .bind(vec![0_u8; 48])
        .bind(0_i64)
        .execute(store.store.admin())
        .await
        .unwrap();
        Store::open(&store.url)
            .await
            .expect("an older hub opens a newer store");
    }
}
