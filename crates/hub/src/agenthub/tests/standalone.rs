//! A standalone board (`devagotchi board init`) opened on the database a hub uses, before the
//! hub first starts there or after: the hub's role still runs the export and the board, and
//! the standalone board stays out of the hub's tenants.

use devagotchi_store::Store;

/// What the hub's role does with AgentHub's exporter functions (`clickhouse.rs`), as the hub.
async fn the_hubs_role_exports(store: &Store) -> Result<(), String> {
    let mut connection = store.begin().await.map_err(|e| e.message)?;
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(role, "devagotchi_hub", "the hub's pool runs as its role");
    for sql in [
        "SELECT count(*) FROM devagotchi.outbox_pending_tenants(10)",
        "SELECT count(*) FROM devagotchi.outbox_backlog()",
        "SELECT devagotchi.outbox_resend()",
    ] {
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&mut *connection)
            .await
            .map_err(|e| format!("{sql}: {e}"))?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_standalone_board_opened_first_leaves_the_hub_its_export() {
    let url = or_skip!(devagotchi_store::testing::empty_postgres());
    // `board init` on the empty database, then the hub's first start on it (tests/e2e.py).
    let board = crate::agenthub::open_board(&url, "e2e-board", true)
        .await
        .unwrap();
    // A role is the server's, not the database's: on a new server (CI's e2e) there is no
    // `devagotchi_hub` yet, so AgentHub's migration 0001 grants it nothing. This server has
    // one from other tests; take back what 0001 granted, to stand where a new server is.
    sqlx::query(
        "REVOKE EXECUTE ON FUNCTION devagotchi.outbox_pending_tenants(bigint),
           devagotchi.outbox_backlog(), devagotchi.outbox_resend() FROM devagotchi_hub",
    )
    .execute(Store::open(&url, &[]).await.unwrap().admin())
    .await
    .unwrap();
    let hub = crate::store::open(&url).await.unwrap();
    the_hubs_role_exports(&hub).await.unwrap();
    // And the other way round: the hub's own database, a standalone board opened on it.
    let after = crate::store::testing::postgres().await.unwrap();
    crate::agenthub::open_board(&after.url, "e2e-board", true)
        .await
        .unwrap();
    the_hubs_role_exports(&after.store).await.unwrap();
    // The standalone board is under the local tenant: none the hub serves.
    assert_eq!(board.hosted().tenant(), crate::agenthub::LOCAL_TENANT);
}
