use devagotchi_store::{or_skip, testing};

use super::MIGRATIONS;

#[test]
fn every_migration_is_embedded_with_the_repositorys_line_endings() {
    testing::assert_line_endings(&MIGRATIONS);
}

/// On a database of its own, with no host: every table AgentHub makes is a board table, and
/// its history is its own.
#[tokio::test]
async fn on_its_own_every_table_is_a_board_table_with_its_own_history() {
    let test = or_skip!(testing::postgres_with(&[&MIGRATIONS]));
    // As the login role: the hub's role, where the server has one, may not read the history.
    let mut connection = test.store.admin().acquire().await.unwrap();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables
         WHERE table_schema = 'devagotchi' ORDER BY 1",
    )
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    assert!(tables.len() >= 13, "{tables:?}");
    assert!(tables.iter().all(|t| t.starts_with("board_")), "{tables:?}");
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _agenthub_migrations ORDER BY version")
            .fetch_all(&mut *connection)
            .await
            .unwrap();
    assert_eq!(versions, vec![1]);
}

/// Opened again, nothing changes: every statement finds what it would make.
#[tokio::test]
async fn run_again_the_migrations_find_everything_made() {
    let test = or_skip!(testing::postgres_with(&[&MIGRATIONS]));
    drop(test.store);
    let mut connection = <sqlx::AnyConnection as sqlx::Connection>::connect(&test.url)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        MIGRATIONS.iter().next().unwrap().sql.as_ref().to_string(),
    ))
    .execute(&mut connection)
    .await
    .unwrap();
}
