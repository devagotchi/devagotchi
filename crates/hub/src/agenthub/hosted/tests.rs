use super::*;
use crate::store::testing;

use crate::agenthub::{
    AnchorPatch, Anchors, DepKind, Filter, NextFilter, Patch, ProjectSettings, approval::Verdict,
};

const TENANT: &str = "t-00000000-0000-4000-8000-000000000001";
const OTHER: &str = "t-00000000-0000-4000-8000-000000000002";

#[tokio::test]
async fn native_board_graph_filters_versions_handoff_and_approval_rules() {
    for test in testing::stores().await {
        if test.store.backend() != Backend::Postgres {
            continue;
        }
        seed(&test.store).await;
        let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        board.initialize().await.unwrap();
        let human = Actor::human("owner");
        let agent = Actor::agent("agent");
        let next = Actor::agent("next");
        let settings = board.settings().await.unwrap();
        board.put_settings(&human, &settings).await.unwrap();
        let mut bad = settings.clone();
        bad.lease_min_ttl_ms = 0;
        assert_eq!(
            board.put_settings(&human, &bad).await.unwrap_err().code,
            "invalid"
        );
        let project = board
            .set_project(
                &human,
                &ProjectSettings {
                    repo: Some("https://github.com/acme/demo".into()),
                    environments: Some(vec!["test".into()]),
                },
            )
            .await
            .unwrap();
        assert_eq!(project.environments, vec!["test"]);
        let workflow = board.workflow().await.unwrap();
        board.put_workflow(&human, &workflow).await.unwrap();
        let parent = board
            .create(
                &human,
                &NewTicket {
                    title: "Epic".into(),
                    kind: Some("epic".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let ticket = board
            .create(
                &human,
                &NewTicket {
                    title: "Literal 100%_work".into(),
                    parent: Some(parent.id.clone()),
                    labels: vec!["poam".into()],
                    anchors: Anchors {
                        environment: Some("test".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(ticket.id, format!("{}.1", parent.id));
        assert_eq!(
            board.view(&parent.id).await.unwrap().children,
            vec![ticket.id.clone()]
        );
        assert_eq!(
            board
                .list(&Filter {
                    text: Some("100%_".into()),
                    label: Some("poam".into()),
                    ..Default::default()
                })
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            board
                .list(&Filter {
                    text: Some("100x".into()),
                    ..Default::default()
                })
                .await
                .unwrap()
                .is_empty()
        );
        board
            .add_dep(&human, &ticket.id, &parent.id, DepKind::Blocks)
            .await
            .unwrap();
        assert_eq!(
            board
                .claim(&agent, &ticket.id, None)
                .await
                .unwrap_err()
                .code,
            "blocked"
        );
        assert_eq!(
            board
                .add_dep(&human, &parent.id, &ticket.id, DepKind::Blocks)
                .await
                .unwrap_err()
                .code,
            "dependency_cycle"
        );
        board
            .remove_dep(&human, &ticket.id, &parent.id, DepKind::Blocks)
            .await
            .unwrap();
        let updated = board
            .update(
                &human,
                &ticket.id,
                ticket.version,
                &Patch {
                    assignee: Some(Some("agent".into())),
                    anchors: AnchorPatch {
                        repo: Some(Some("https://github.com/acme/demo".into())),
                        ..Default::default()
                    },
                    occurrences: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.occurrences, 2);
        assert_eq!(
            board
                .update(
                    &human,
                    &ticket.id,
                    ticket.version,
                    &Patch {
                        title: Some("stale".into()),
                        ..Default::default()
                    }
                )
                .await
                .unwrap_err()
                .code,
            "version_conflict"
        );
        let held = board
            .claim_next(
                &agent,
                &NextFilter {
                    parent: Some(parent.id.clone()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(held.ticket.id, ticket.id);
        assert!(board.next_lease_expiry().await.unwrap().is_some());
        assert_eq!(
            board
                .force_release(&agent, &ticket.id, "force")
                .await
                .unwrap_err()
                .code,
            "humans_only"
        );
        board
            .handoff(
                &agent,
                &ticket.id,
                held.fence,
                "Continue here",
                Some("next"),
            )
            .await
            .unwrap();
        assert!(
            !board
                .ready(&agent, 20)
                .await
                .unwrap()
                .iter()
                .any(|t| t.id == ticket.id)
        );
        let held = board
            .claim_next(
                &next,
                &NextFilter {
                    parent: Some(parent.id.clone()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        board
            .force_release(&human, &ticket.id, "operator intervention")
            .await
            .unwrap();
        assert_eq!(
            board
                .heartbeat(&next, &ticket.id, held.fence, None)
                .await
                .unwrap_err()
                .code,
            "not_holder"
        );
        assert_eq!(board.reap().await.unwrap(), 0);
        let approval = board
            .request_approval(
                &agent,
                "Dangerous step",
                "Change config",
                "Testing",
                "Restore backup",
                &["approval".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(board.pending_approvals().await.unwrap().len(), 1);
        assert_eq!(
            board
                .update(&human, &approval.id, approval.version, &Patch::default())
                .await
                .unwrap_err()
                .code,
            "approval_decide_only"
        );
        assert_eq!(
            board
                .decide_approval(
                    &agent,
                    &approval.id,
                    approval.version,
                    Verdict::Approved,
                    ""
                )
                .await
                .unwrap_err()
                .code,
            "humans_only"
        );
        assert_eq!(
            board
                .decide_approval(
                    &human,
                    &approval.id,
                    approval.version + 1,
                    Verdict::Approved,
                    ""
                )
                .await
                .unwrap_err()
                .code,
            "approval_changed"
        );
        let decided = board
            .decide_approval(
                &human,
                &approval.id,
                approval.version,
                Verdict::Approved,
                "Confirmed",
            )
            .await
            .unwrap();
        assert_eq!(decided.status, "approved");
        assert_eq!(board.approval_status(&approval.id).await.unwrap(), decided);
        assert!(board.pending_approvals().await.unwrap().is_empty());
        assert_eq!(
            board
                .decide_approval(&human, &approval.id, approval.version, Verdict::Denied, "")
                .await
                .unwrap_err()
                .code,
            "already_decided"
        );
        assert!(board.verify_chain().await.unwrap().ok);
    }
}

#[tokio::test]
async fn database_time_expiry_reaping_and_wip_are_enforced_across_instances() {
    for test in testing::stores().await {
        if test.store.backend() != Backend::Postgres {
            continue;
        }
        seed(&test.store).await;
        let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        board.initialize().await.unwrap();
        let agent = Actor {
            id: "agent",
            human: false,
            wip_limit: Some(1),
        };
        let a = board
            .create(
                &agent,
                &NewTicket {
                    title: "A".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let b = board
            .create(
                &agent,
                &NewTicket {
                    title: "B".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let held = board.claim(&agent, &a.id, None).await.unwrap();
        assert_eq!(
            board.claim(&agent, &b.id, None).await.unwrap_err().code,
            "wip_limit"
        );
        assert_eq!(
            board
                .claim_next(&agent, &NextFilter::default())
                .await
                .unwrap_err()
                .code,
            "wip_limit"
        );
        transaction!(&test.store, |connection| {
            test.store.scope(connection,TENANT).await?;
            sqlx::query("UPDATE board_tickets SET lease_expires_at=1 WHERE tenant_id=$1 AND board_id='demo' AND id=$2")
                .bind(TENANT).bind(&a.id).execute(&mut *connection).await?;
            Ok(())
        }).unwrap();
        assert_eq!(
            board
                .heartbeat(&agent, &a.id, held.fence, None)
                .await
                .unwrap_err()
                .code,
            "lease_expired"
        );
        assert_eq!(board.reap().await.unwrap(), 1);
        let again = board.claim(&agent, &a.id, None).await.unwrap();
        assert!(again.fence > held.fence);
        assert_eq!(again.ticket.expiry_count, 1);
        assert!(board.verify_chain().await.unwrap().ok);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_runtime_resolver_uses_postgres_without_board_files_and_observes_other_hubs() {
    for test in testing::stores().await {
        if test.store.backend() != Backend::Postgres {
            continue;
        }
        seed(&test.store).await;
        let dir = tempfile::tempdir().unwrap();
        let config = crate::config::HubConfig::parse_with(
            &serde_json::json!({
                "listen":"127.0.0.1:8787","public_url":"http://127.0.0.1:8787",
                "auth":{"mode":"dev","tokens_file":dir.path().join("tokens.json")},
                "brokers":[],"grants":[],
                "boards":[{"project":"shared","dir":dir.path().join("never-created-shared")}],
                "account_boards":{"dir":dir.path().join("never-created-accounts")}
            })
            .to_string(),
            |_| None,
        )
        .unwrap();
        let boards = std::sync::Arc::new(
            crate::agenthub::mcp::Boards::from_config(&config, dir.path())
                .using_postgres(test.store.clone())
                .unwrap(),
        );
        let peer = std::sync::Arc::new(
            crate::agenthub::mcp::Boards::from_config(&config, dir.path())
                .using_postgres(test.store.clone())
                .unwrap(),
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
        assert!(alice.connection.is_none());
        assert!(bob.connection.is_none());
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
        assert!(shared.connection.is_none());
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
    let Some(test) = testing::postgres().await else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::HubConfig::parse_with(
        &serde_json::json!({
            "listen":"127.0.0.1:8787","public_url":"http://127.0.0.1:8787",
            "auth":{"mode":"dev","tokens_file":dir.path().join("tokens.json")},
            "brokers":[],"grants":[],"boards":[{"project":"shared","dir":dir.path().join("unused")}]
        })
        .to_string(),
        |_| None,
    )
    .unwrap();
    let boards = std::sync::Arc::new(
        crate::agenthub::mcp::Boards::from_config(&config, dir.path())
            .using_postgres(test.store.clone())
            .unwrap(),
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
async fn outbox_failure_rolls_back_ticket_lease_event_and_fence() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    seed(&test.store).await;
    let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
    board.initialize().await.unwrap();
    let actor = Actor::agent("agent");
    let ticket = board
        .create(
            &actor,
            &NewTicket {
                title: "Atomic".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let events = board.events().await.unwrap();
    let listener = crate::agenthub::mcp::Boards::postgres_listener(&test.url)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE devagotchi.board_outbox ADD CONSTRAINT refuse_claim CHECK(kind <> 'ticket.claimed')")
        .execute(test.store.admin()).await.unwrap();
    assert!(board.claim(&actor, &ticket.id, None).await.is_err());
    assert_eq!(board.get(&ticket.id).await.unwrap(), ticket);
    assert_eq!(board.events().await.unwrap(), events);
    let mut listener = listener;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), listener.recv())
            .await
            .is_err()
    );
    sqlx::query("ALTER TABLE devagotchi.board_outbox DROP CONSTRAINT refuse_claim")
        .execute(test.store.admin())
        .await
        .unwrap();
    assert_eq!(
        board.claim(&actor, &ticket.id, None).await.unwrap().fence,
        1
    );
}

#[tokio::test]
async fn persisted_leases_are_reaped_without_opening_any_runtime_board() {
    let Some(test) = testing::postgres().await else {
        return;
    };
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
    let Some(test) = testing::postgres().await else {
        return;
    };
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

#[tokio::test]
async fn native_postgres_board_lifecycle_has_one_authority_and_atomic_history() {
    for test in testing::stores().await {
        if test.store.backend() != Backend::Postgres {
            assert!(HostedBoard::new(test.store.clone(), TENANT, "demo").is_err());
            continue;
        }
        seed(&test.store).await;
        let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        let peer = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        let other = HostedBoard::new(test.store.clone(), OTHER, "demo").unwrap();
        assert!(board.initialize().await.unwrap());
        assert!(!peer.initialize().await.unwrap());
        assert!(other.initialize().await.unwrap());
        let owner = Actor::human("owner");
        let worker = Actor::agent("worker");
        let reviewer = Actor::agent("reviewer");
        let ticket = board
            .create(
                &owner,
                &NewTicket {
                    title: "Native Postgres".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(other.get(&ticket.id).await.unwrap_err().code, "not_found");
        let held = board.claim(&worker, &ticket.id, None).await.unwrap();
        assert_eq!(
            peer.claim(&reviewer, &ticket.id, None)
                .await
                .unwrap_err()
                .code,
            "held_by_other"
        );
        assert!(
            peer.claim(&worker, &ticket.id, None)
                .await
                .unwrap()
                .already_held
        );
        assert_eq!(
            board
                .heartbeat(&worker, &ticket.id, held.fence + 1, None)
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
        board
            .heartbeat(
                &worker,
                &ticket.id,
                held.fence,
                Some(serde_json::json!({"step":1})),
            )
            .await
            .unwrap();
        assert_eq!(
            peer.get(&ticket.id).await.unwrap().progress,
            Some(serde_json::json!({"step":1}))
        );
        assert_eq!(
            board
                .add_note(
                    &worker,
                    &ticket.id,
                    "decision",
                    "Keep the fence",
                    None,
                    None
                )
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
        board
            .add_note(
                &worker,
                &ticket.id,
                "decision",
                "Keep the fence",
                None,
                Some(held.fence),
            )
            .await
            .unwrap();
        let reviewed = board
            .complete(&worker, &ticket.id, held.fence, "Ready for review")
            .await
            .unwrap();
        assert_eq!(reviewed.state, "review");
        assert_eq!(
            board
                .claim(&worker, &ticket.id, None)
                .await
                .unwrap_err()
                .code,
            "distinct_required"
        );
        let review = peer.claim(&reviewer, &ticket.id, None).await.unwrap();
        assert!(review.fence > held.fence);
        assert_eq!(
            board
                .complete(&worker, &ticket.id, held.fence, "stale")
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
        assert_eq!(
            peer.complete(&reviewer, &ticket.id, review.fence, "Verified")
                .await
                .unwrap()
                .state,
            "done"
        );
        let events = board.events().await.unwrap();
        let mut previous = GENESIS.to_string();
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event.seq, index as i64 + 1);
            assert_eq!(event.prev_hash, previous);
            assert_eq!(event.hash, event_hash(&previous, event));
            previous = event.hash.clone();
        }
        transaction!(&test.store, |connection| {
            test.store.scope(connection, TENANT).await?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM board_outbox WHERE tenant_id=$1 AND board_id='demo'",
            )
            .bind(TENANT)
            .fetch_one(&mut *connection)
            .await?;
            assert_eq!(count, events.len() as i64);
            Ok(())
        })
        .unwrap();
    }
}

#[tokio::test]
async fn concurrent_native_claims_have_one_winner_and_reclaim_burns_fences() {
    for test in testing::stores().await {
        if test.store.backend() != Backend::Postgres {
            continue;
        }
        seed(&test.store).await;
        let board = HostedBoard::new(test.store.clone(), TENANT, "demo").unwrap();
        board.initialize().await.unwrap();
        let a = Actor::agent("a");
        let b = Actor::agent("b");
        let ticket = board
            .create(
                &a,
                &NewTicket {
                    title: "Racing claims".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let (one, two) = tokio::join!(
            board.claim(&a, &ticket.id, None),
            board.claim(&b, &ticket.id, None)
        );
        assert_ne!(one.is_ok(), two.is_ok());
        let (holder, other, held) = match (one, two) {
            (Ok(held), Err(error)) => {
                assert_eq!(error.code, "held_by_other");
                (&a, &b, held)
            }
            (Err(error), Ok(held)) => {
                assert_eq!(error.code, "held_by_other");
                (&b, &a, held)
            }
            _ => panic!("claims did not settle with one winner"),
        };
        board
            .release(holder, &ticket.id, held.fence, "handover")
            .await
            .unwrap();
        let next = board.claim(other, &ticket.id, None).await.unwrap();
        assert!(next.fence > held.fence);
        assert_eq!(
            board
                .heartbeat(holder, &ticket.id, held.fence, None)
                .await
                .unwrap_err()
                .code,
            "stale_fence"
        );
    }
}

/// Approving ahead of time on a hosted board: granted, used and ended in Postgres
/// transactions, on the board's chain.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_board_approves_a_kind_ahead_of_time() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    seed(&test.store).await;
    let hosted = HostedBoard::new(test.store.clone(), TENANT, "ahead").unwrap();
    hosted.initialize().await.unwrap();
    let board = crate::agenthub::Board::from_hosted(hosted).await.unwrap();
    tokio::task::spawn_blocking(move || {
        crate::agenthub::standing_tests::standing_approvals_hold(&board, None)
    })
    .await
    .unwrap();
}

/// A note reaching who it is for on a hosted board (`agenthub::inbox`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_board_delivers_a_note_once() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    seed(&test.store).await;
    let hosted = HostedBoard::new(test.store.clone(), TENANT, "inbox").unwrap();
    hosted.initialize().await.unwrap();
    let board = crate::agenthub::Board::from_hosted(hosted).await.unwrap();
    tokio::task::spawn_blocking(move || crate::agenthub::inbox_tests::inbox_holds(&board))
        .await
        .unwrap();
}
