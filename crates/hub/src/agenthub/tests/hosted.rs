//! The hub's hosted boards as the hub opens them: the router on Postgres alone, its listener,
//! the lease reaper over every tenant, and the home tenant. The hosted board itself is
//! AgentHub's (`crates/agenthub/src/tests/hosted*.rs`).

use crate::agenthub::hosted::*;
use crate::agenthub::{registry, *};
use crate::store::{Store, testing, transaction};

use crate::agenthub::Filter;

const TENANT: &str = "t-00000000-0000-4000-8000-000000000001";
const OTHER: &str = "t-00000000-0000-4000-8000-000000000002";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_runtime_resolver_uses_postgres_without_board_files_and_observes_other_hubs() {
    for test in testing::stores().await {
        seed(&test.store).await;
        let dir = tempfile::tempdir().unwrap();
        let config = crate::config::HubConfig::parse_with(
            &serde_json::json!({
                "listen":"127.0.0.1:8787","public_url":"http://127.0.0.1:8787",
                "auth":{"mode":"dev","tokens_file":dir.path().join("tokens.json")},
                "brokers":[],"grants":[],
                "boards":[{"project":"shared"}],
                "account_boards":{}
            })
            .to_string(),
            |_| None,
        )
        .unwrap();
        let boards = std::sync::Arc::new(
            crate::agenthub::mcp::Boards::new(&config, test.store.clone()).unwrap(),
        );
        let peer = std::sync::Arc::new(
            crate::agenthub::mcp::Boards::new(&config, test.store.clone()).unwrap(),
        );
        let bound = boards.bind("alice", "demo", &[]).unwrap();
        assert!(bound.created);
        assert!(!peer.bind("alice", "demo", &[]).unwrap().created);
        assert_eq!(peer.account_boards("alice").unwrap(), vec!["demo"]);
        assert_eq!(
            boards.bind("alice", "second", &[]).unwrap_err().code,
            "plan_limit"
        );
        assert!(peer.open_account("bob", "demo").unwrap().is_none());
        boards.bind("bob", "demo", &[]).unwrap();
        let alice = boards.open_account("alice", "demo").unwrap().unwrap();
        let bob = boards.open_account("bob", "demo").unwrap().unwrap();
        assert!(!alice.is_local());
        assert!(!bob.is_local());
        let actor = Actor::agent("cli");
        let ticket = alice
            .create(
                &actor,
                NewTicket {
                    title: "Shared runtime".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(bob.get(&ticket.id).unwrap_err().code, "not_found");
        let same = peer.open_account("alice", "demo").unwrap().unwrap();
        let other = peer.open_account("bob", "demo").unwrap().unwrap();
        let mut unrelated = other.changes();
        let listener = crate::agenthub::mcp::Boards::postgres_listener(&test.url)
            .await
            .unwrap();
        let listener_task = tokio::spawn(peer.clone().listen_postgres(listener));
        let mut changes = same.changes();
        let fence = alice.claim(&actor, &ticket.id, None).unwrap().fence;
        tokio::time::timeout(std::time::Duration::from_secs(3), changes.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(same.get(&ticket.id).unwrap().ticket.fence, fence);
        assert_eq!(same.prime(&actor, 5).unwrap().held[0].id, ticket.id);
        assert_eq!(same.reap().unwrap(), 0);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), changes.changed())
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), unrelated.changed())
                .await
                .is_err()
        );
        let (one, two) = tokio::join!(
            tokio::task::spawn_blocking({
                let boards = boards.clone();
                move || boards.bind("racer", "one", &[])
            }),
            tokio::task::spawn_blocking({
                let boards = peer.clone();
                move || boards.bind("racer", "two", &[])
            })
        );
        let (one, two) = (one.unwrap(), two.unwrap());
        assert_ne!(one.is_ok(), two.is_ok());
        assert_eq!(one.err().or_else(|| two.err()).unwrap().code, "plan_limit");
        assert_eq!(boards.account_boards("racer").unwrap().len(), 1);
        let (one, two) = tokio::join!(
            tokio::task::spawn_blocking({
                let boards = boards.clone();
                move || boards.bind("same-racer", "same", &[])
            }),
            tokio::task::spawn_blocking({
                let boards = peer.clone();
                move || boards.bind("same-racer", "same", &[])
            })
        );
        assert_ne!(one.unwrap().unwrap().created, two.unwrap().unwrap().created);
        let shared = boards.open("shared").unwrap();
        assert!(!shared.is_local());
        assert_eq!(shared.project(), "shared");
        assert!(!dir.path().join("never-created-shared").exists());
        assert!(!dir.path().join("never-created-accounts").exists());
        assert!(same.verify_chain().unwrap().ok);
        listener_task.abort();
        assert!(listener_task.await.unwrap_err().is_cancelled());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listener_reconnect_invalidates_waiters_even_without_a_new_event() {
    let test = or_skip!(testing::postgres());
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::HubConfig::parse_with(
        &serde_json::json!({
            "listen":"127.0.0.1:8787","public_url":"http://127.0.0.1:8787",
            "auth":{"mode":"dev","tokens_file":dir.path().join("tokens.json")},
            "brokers":[],"grants":[],"boards":[{"project":"shared"}]
        })
        .to_string(),
        |_| None,
    )
    .unwrap();
    let boards = std::sync::Arc::new(
        crate::agenthub::mcp::Boards::new(&config, test.store.clone()).unwrap(),
    );
    let board = boards.open("shared").unwrap();
    let mut changes = board.changes();
    let mut listener = crate::agenthub::mcp::Boards::postgres_listener(&test.url)
        .await
        .unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut listener)
        .await
        .unwrap();
    let task = tokio::spawn(boards.clone().listen_postgres(listener));
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .execute(test.store.admin())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), changes.changed())
        .await
        .unwrap()
        .unwrap();
    let next = board
        .create(
            &Actor::agent("agent"),
            NewTicket {
                title: "After reconnect".into(),
                ..Default::default()
            },
        )
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(board.get(&next.id).unwrap().ticket.id, next.id);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn persisted_leases_are_reaped_without_opening_any_runtime_board() {
    let test = or_skip!(testing::postgres());
    seed(&test.store).await;
    for (tenant, owner) in [(TENANT, "one"), (OTHER, "two")] {
        let board = HostedBoard::for_owner(test.store.clone(), tenant, owner, "demo").unwrap();
        board.initialize().await.unwrap();
        let actor = Actor::agent("agent");
        let ticket = board
            .create(
                &actor,
                &NewTicket {
                    title: "Restart recovery".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        board.claim(&actor, &ticket.id, None).await.unwrap();
        transaction!(&test.store, |connection| {
            test.store.scope(connection, tenant).await?;
            sqlx::query("UPDATE board_tickets SET lease_expires_at=1 WHERE tenant_id=$1")
                .bind(tenant)
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
        .unwrap();
    }
    let registry = registry::Registry::new(test.store.clone()).unwrap();
    assert!(registry.next_lease_delay().await.unwrap().unwrap() < 0);
    assert_eq!(registry.reap_due().await.unwrap(), 2);
    assert_eq!(registry.next_lease_delay().await.unwrap(), None);
    assert_eq!(registry.reap_due().await.unwrap(), 0);
    for (tenant, owner) in [(TENANT, "one"), (OTHER, "two")] {
        let board = HostedBoard::for_owner(test.store.clone(), tenant, owner, "demo").unwrap();
        let tickets = board.list(&Filter::default()).await.unwrap();
        assert!(tickets[0].holder.is_none());
        assert_eq!(tickets[0].expiry_count, 1);
        assert!(board.verify_chain().await.unwrap().ok);
    }
}

async fn seed(store: &Store) {
    let mut connection = store.read().await.unwrap();
    for (tenant, home) in [(TENANT, 1i64), (OTHER, 0)] {
        sqlx::query("INSERT INTO tenancy_tenants (id,home,created_at) VALUES ($1,$2,1)")
            .bind(tenant)
            .bind(home)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn externally_authenticated_boards_provision_the_home_tenant_without_local_accounts() {
    let test = or_skip!(testing::postgres());
    let registry = registry::Registry::new(test.store.clone()).unwrap();
    let (a, b) = tokio::join!(
        registry.open("@shared", "demo", true, None),
        registry.open("org-acme", "demo", true, None)
    );
    assert!(a.unwrap().is_some());
    assert!(b.unwrap().is_some());
    assert_eq!(registry.names("@shared").await.unwrap(), vec!["demo"]);
    assert_eq!(registry.names("org-acme").await.unwrap(), vec!["demo"]);
    let mut connection = test.store.read().await.unwrap();
    let homes: i64 = sqlx::query_scalar("SELECT count(*) FROM tenancy_tenants WHERE home=1")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(homes, 1);
}
