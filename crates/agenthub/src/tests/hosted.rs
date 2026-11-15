//! A hosted board in Postgres: the ticket graph, filters, versions, hand-off and approval
//! rules; lease expiry by the database's clock across instances; an outbox that fails rolls
//! the whole write back.

use devagotchi_store::{or_skip, transaction};

use crate::hosted::*;
use crate::{
    Actor, AnchorPatch, Anchors, DepKind, Filter, NewTicket, NextFilter, Patch, ProjectSettings,
    approval::Verdict,
};

const TENANT: &str = "t-00000000-0000-4000-8000-000000000001";

#[tokio::test]
async fn native_board_graph_filters_versions_handoff_and_approval_rules() {
    for test in super::stores().await {
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
    for test in super::stores().await {
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

#[tokio::test]
async fn outbox_failure_rolls_back_ticket_lease_event_and_fence() {
    let test = or_skip!(super::postgres());
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
    let listener = super::board_changes(&test.url).await;
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
