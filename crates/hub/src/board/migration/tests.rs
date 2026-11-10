use super::*;
use crate::{
    board::{Actor, Anchors, Board, DepKind, NewTicket},
    store::{testing, transaction},
};
use sqlx::Row;

const TENANT: &str = "t-00000000-0000-4000-8000-000000000001";
const OTHER: &str = "t-00000000-0000-4000-8000-000000000002";

fn source() -> (tempfile::TempDir, Board) {
    let dir = tempfile::tempdir().unwrap();
    let board = Board::open(&dir.path().join("board.sqlite3"), "migration-test").unwrap();
    let human = Actor::human("owner");
    let epic = board
        .create(
            &human,
            NewTicket {
                title: "Roadmap".into(),
                kind: Some("epic".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let ticket = board
        .create(
            &human,
            NewTicket {
                title: "Keep all fields".into(),
                description: "Existing evidence".into(),
                parent: Some(epic.id.clone()),
                labels: vec!["poam".into()],
                anchors: Anchors {
                    repo: Some("https://github.com/acme/demo".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
    board
        .add_dep(&human, &ticket.id, &epic.id, DepKind::Related)
        .unwrap();
    let agent = Actor::agent("cli");
    let lease = board.claim(&agent, &ticket.id, None).unwrap();
    board
        .add_note(
            &agent,
            &ticket.id,
            "decision",
            "Preserve this advice",
            None,
            Some(lease.fence),
        )
        .unwrap();
    (dir, board)
}

async fn seed_tenant(store: &Store, tenant: &str, home: i64) {
    let mut connection = store.read().await.unwrap();
    sqlx::query("INSERT INTO tenancy_tenants (id, home, created_at) VALUES ($1, $2, 1)")
        .bind(tenant)
        .bind(home)
        .execute(&mut *connection)
        .await
        .unwrap();
}

#[test]
fn snapshot_is_stable_read_only_and_refuses_a_broken_chain() {
    let (dir, board) = source();
    let path = dir.path().join("board.sqlite3");
    let before = board.events(0, 1000).unwrap();
    let one = Snapshot::read(&path).unwrap().receipt(TENANT);
    let two = Snapshot::read(&path).unwrap().receipt(TENANT);
    assert_eq!(one, two);
    assert_eq!(one.counts["tickets"], 2);
    assert_eq!(one.counts["notes"], 1);
    assert_eq!(board.events(0, 1000).unwrap(), before);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER events_no_update; UPDATE events SET actor = 'tampered' WHERE seq = 1;",
        )
        .unwrap();
    assert_eq!(Snapshot::read(&path).unwrap_err().code, "board_damaged");
}

#[tokio::test]
async fn import_preserves_rows_fences_history_and_retries_without_duplicate_outbox() {
    let (dir, board) = source();
    let snapshot = Snapshot::read(&dir.path().join("board.sqlite3")).unwrap();
    for test in testing::stores().await {
        let store = &test.store;
        seed_tenant(store, TENANT, 1).await;
        let receipt = snapshot.import(store, TENANT).await.unwrap();
        assert_eq!(receipt, snapshot.import(store, TENANT).await.unwrap());
        transaction!(store, |connection| {
            store.scope(connection, TENANT).await?;
            for table in TABLES {
                let sql = format!(
                    "SELECT count(*) FROM {} WHERE tenant_id = $1 AND board_id = $2",
                    table.target
                );
                let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(TENANT)
                    .bind(&receipt.project)
                    .fetch_one(&mut *connection)
                    .await?;
                assert_eq!(count, receipt.counts[table.source]);
            }
            let row = sqlx::query(
                "SELECT holder, fence, lease_expires_at, parent FROM board_tickets
                WHERE tenant_id = $1 AND board_id = $2 AND holder IS NOT NULL",
            )
            .bind(TENANT)
            .bind(&receipt.project)
            .fetch_one(&mut *connection)
            .await?;
            assert_eq!(row.try_get::<String, _>("holder")?, "cli");
            assert_eq!(row.try_get::<i64, _>("fence")?, receipt.fence);
            assert!(row.try_get::<i64, _>("lease_expires_at")? > 0);
            assert!(!row.try_get::<String, _>("parent")?.contains('.'));
            let head: String = sqlx::query_scalar(
                "SELECT hash FROM board_events
                WHERE tenant_id = $1 AND board_id = $2 ORDER BY seq DESC LIMIT 1",
            )
            .bind(TENANT)
            .bind(&receipt.project)
            .fetch_one(&mut *connection)
            .await?;
            assert_eq!(head, receipt.chain_head);
            let pending: i64 =
                sqlx::query_scalar("SELECT count(*) FROM board_outbox WHERE tenant_id = $1")
                    .bind(TENANT)
                    .fetch_one(&mut *connection)
                    .await?;
            assert_eq!(pending, 1);
            Ok(())
        })
        .unwrap();
        board
            .add_note(
                &Actor::human("owner"),
                &board.list(&Default::default()).unwrap()[0].id,
                "note",
                "Source changed after snapshot",
                None,
                None,
            )
            .unwrap();
        let changed = Snapshot::read(&dir.path().join("board.sqlite3")).unwrap();
        assert_eq!(
            changed.import(store, TENANT).await.unwrap_err().code,
            "version_conflict"
        );
    }
}

#[tokio::test]
async fn unknown_tenant_import_rolls_back_without_creating_a_project() {
    let (dir, _board) = source();
    let snapshot = Snapshot::read(&dir.path().join("board.sqlite3")).unwrap();
    for test in testing::stores().await {
        assert_eq!(
            snapshot.import(&test.store, TENANT).await.unwrap_err().code,
            "not_found"
        );
        let store = &test.store;
        transaction!(store, |connection| {
            store.scope(connection, TENANT).await?;
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM board_projects")
                .fetch_one(&mut *connection)
                .await?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }
}

#[tokio::test]
async fn postgres_enforces_tenant_scope_and_history_is_append_only() {
    let (dir, _board) = source();
    let snapshot = Snapshot::read(&dir.path().join("board.sqlite3")).unwrap();
    for test in testing::stores().await {
        let store = &test.store;
        seed_tenant(store, TENANT, 1).await;
        seed_tenant(store, OTHER, 0).await;
        snapshot.import(store, TENANT).await.unwrap();
        for table in ["board_events", "board_notes", "board_imports"] {
            let error = transaction!(store, |connection| {
                store.scope(connection, TENANT).await?;
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "DELETE FROM {table} WHERE tenant_id = $1"
                )))
                .bind(TENANT)
                .execute(&mut *connection)
                .await?;
                Ok(())
            })
            .unwrap_err();
            assert!(error.message.contains("append-only"), "{}", error.message);
        }

        if store.backend() == Backend::Postgres {
            assert!(store.enforced());
            transaction!(store, |connection| {
                store.scope(connection, OTHER).await?;
                for table in [
                    "board_projects",
                    "board_metadata",
                    "board_settings",
                    "board_fence",
                    "board_tickets",
                    "board_deps",
                    "board_notes",
                    "board_events",
                    "board_imports",
                    "board_outbox",
                ] {
                    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                        "SELECT count(*) FROM {table}"
                    )))
                    .fetch_one(&mut *connection)
                    .await?;
                    assert_eq!(count, 0, "RLS on {table}");
                }
                Ok(())
            })
            .unwrap();
            let denied = transaction!(store, |connection| {
                store.scope(connection, OTHER).await?;
                sqlx::query(
                    "INSERT INTO board_projects (tenant_id, id, source_hash, imported_at)
                    VALUES ($1, 'forged', 'bad', 1)",
                )
                .bind(TENANT)
                .execute(&mut *connection)
                .await?;
                Ok(())
            });
            assert!(denied.is_err());
        }
    }
}

#[tokio::test]
async fn verification_preserves_binary_key_order_on_locale_aware_postgres() {
    let (dir, _board) = source();
    let path = dir.path().join("board.sqlite3");
    let connection = Connection::open(&path).unwrap();
    for key in ["a-b", "a_b", "ab", "A", "a"] {
        connection
            .execute("INSERT INTO meta (key,value) VALUES (?1,?1)", [key])
            .unwrap();
    }
    let snapshot = Snapshot::read(&path).unwrap();
    for test in testing::stores().await {
        seed_tenant(&test.store, TENANT, 1).await;
        if test.store.backend() == Backend::Postgres {
            snapshot.import_postgres(&test.store, TENANT).await.unwrap();
        } else {
            snapshot.import(&test.store, TENANT).await.unwrap();
            assert_eq!(
                snapshot
                    .import_postgres(&test.store, TENANT)
                    .await
                    .unwrap_err()
                    .code,
                "invalid_config"
            );
        }
    }
}

#[tokio::test]
async fn racing_imports_commit_once_and_late_failure_leaves_no_partial_board() {
    let (dir, _board) = source();
    let snapshot = Snapshot::read(&dir.path().join("board.sqlite3")).unwrap();
    for test in testing::stores().await {
        let store = &test.store;
        seed_tenant(store, TENANT, 1).await;
        seed_tenant(store, OTHER, 0).await;
        let (one, two) = tokio::join!(
            snapshot.import(store, TENANT),
            snapshot.import(store, TENANT)
        );
        assert_eq!(one.unwrap(), two.unwrap());
        let mut bad = Snapshot::read(&dir.path().join("board.sqlite3")).unwrap();
        let tickets = bad
            .tables
            .iter_mut()
            .find(|table| table.source == "tickets")
            .unwrap();
        tickets.rows[0][7] = Cell::Text(Some("missing-parent".into()));
        assert!(bad.import(store, OTHER).await.is_err());
        transaction!(store, |connection| {
            store.scope(connection, OTHER).await?;
            for table in [
                "board_projects",
                "board_tickets",
                "board_imports",
                "board_outbox",
            ] {
                let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1");
                let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(OTHER)
                    .fetch_one(&mut *connection)
                    .await?;
                assert_eq!(count, 0, "partial import in {table}");
            }

            Ok(())
        })
        .unwrap();
    }
}
