use super::*;
use super::{
    decisions::Decision,
    request_tests::{Fixture, input},
};
use crate::{identity::accounts::agents::AgentEdit, store};
use devagotchi_agenthub::{Actor, policy::Identity};
use devagotchi_contracts::GrantDuration;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_read_group_returns_narrowed_decisions_and_bounded_history() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let mut request = input();
    request.selection = serde_json::from_value(json!({"kind":"items","items":[
        {"capability":"exec","sandbox_level":"read-only","selector":{}},
        {"capability":"fs.read","sandbox_level":"read-only","selector":{}}]}))
    .unwrap();
    let group = f.request(&request, "group").await.unwrap().result;
    let first = &group.request_ids[0];
    let second = &group.request_ids[1];
    let lookup = f
        .access
        .access_lookup(&f.owner.id, &f.actor(), &group.group_id)
        .await
        .unwrap();
    assert_eq!(lookup.status, "pending");
    assert_eq!(lookup.items.len(), 2);
    f.decide(
        first,
        &Decision::Approve {
            narrow: Narrow {
                duration: Some(GrantDuration::Once),
                note: "One inspection".into(),
                ..Default::default()
            },
        },
        "approve",
    )
    .await
    .unwrap();
    assert_eq!(
        f.access
            .access_lookup(&f.owner.id, &f.actor(), &group.group_id)
            .await
            .unwrap()
            .status,
        "pending"
    );
    f.decide(
        second,
        &Decision::Deny {
            note: "Not needed".into(),
        },
        "deny",
    )
    .await
    .unwrap();
    let lookup = f
        .access
        .access_lookup(&f.owner.id, &f.actor(), &group.group_id)
        .await
        .unwrap();
    assert_eq!(lookup.status, "approved");
    let approved = lookup
        .items
        .iter()
        .find(|i| &i.request_id == first)
        .unwrap();
    assert_eq!(approved.status, "narrowed");
    assert_eq!(approved.granted.as_ref().unwrap()["duration"], "once");
    assert_eq!(approved.granted.as_ref().unwrap()["uses_max"], 1);
    assert_eq!(approved.note.as_deref(), Some("One inspection"));
    let mut query = AccessPageQuery {
        limit: Some(1),
        ..Default::default()
    };
    let page = f
        .access
        .access_requests(&f.owner.id, &f.actor(), &query)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    query.after = page.next;
    assert!(query.after.is_some());
    let last = f
        .access
        .access_requests(&f.owner.id, &f.actor(), &query)
        .await
        .unwrap();
    assert_eq!(last.rows.len(), 1);
    assert_ne!(page.rows[0]["id"], last.rows[0]["id"]);
    assert!(last.next.is_none());
    let before = f.lifecycle_counts().await;
    let grants = f
        .access
        .access_grants(
            &f.owner.id,
            &f.actor(),
            &AccessPageQuery {
                live: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(grants.rows.len(), 1);
    assert!(grants.rows[0].get("tenant_id").is_none());
    assert!(grants.rows[0].get("machine_binding").is_none());
    assert_eq!(before, f.lifecycle_counts().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_read_current_principal_cannot_read_another_agents_requests_or_grants() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let request = f.request(&input(), "request").await.unwrap().result;
    f.grant(false, "other-request").await;
    let other = f
        .access
        .accounts
        .put_agent(
            &f.owner.id,
            &f.owner.id,
            &AgentEdit {
                id: "other".into(),
                name: "Other".into(),
                kind: "codex".into(),
                roles: vec!["worker".into()],
                enabled: true,
                wip_limit: None,
                version: 0,
            },
        )
        .await
        .unwrap();
    let principal = other.principal();
    let other = Actor {
        id: &principal,
        human: false,
        wip_limit: None,
        policy: Some(Identity {
            kind: other.kind,
            roles: other.roles,
        }),
    };
    let query = AccessPageQuery {
        principal: Some(f.principal.clone()),
        ..Default::default()
    };
    assert!(
        f.access
            .access_requests(&f.owner.id, &other, &query)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(
        f.access
            .access_grants(&f.owner.id, &other, &query)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    assert_eq!(
        f.access
            .access_lookup(&f.owner.id, &other, &request.group_id)
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        f.access
            .access_requests(&f.owner.id, &Actor::human(&f.owner.id), &query)
            .await
            .unwrap()
            .rows
            .len(),
        2
    );
    f.access
        .accounts
        .put_agent(
            &f.owner.id,
            &f.owner.id,
            &AgentEdit {
                id: "worker".into(),
                name: "Worker".into(),
                kind: "codex".into(),
                roles: vec!["worker".into()],
                enabled: false,
                wip_limit: None,
                version: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        f.access
            .access_lookup(&f.owner.id, &f.actor(), &request.group_id)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    assert!(
        f.access
            .access_grants(&f.owner.id, &f.actor(), &AccessPageQuery::default())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_read_expiry_filters_work_before_the_reaper_and_reject_invalid_cursors() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let pending = f.request(&input(), "pending").await.unwrap().result;
    let grant = f.grant(false, "approved").await;
    sqlx::query(
        "UPDATE machine_access_requests SET created_at=1,expires_at=2 WHERE tenant_id=$1 AND id=$2",
    )
    .bind(&f.owner.tenant)
    .bind(&pending.request_ids[0])
    .execute(f.access.accounts.store.admin())
    .await
    .unwrap();
    sqlx::query("UPDATE machine_grants SET expires_at=2 WHERE tenant_id=$1 AND id=$2")
        .bind(&f.owner.tenant)
        .bind(&grant)
        .execute(f.access.accounts.store.admin())
        .await
        .unwrap();
    assert_eq!(
        f.access
            .access_lookup(&f.owner.id, &f.actor(), &pending.group_id)
            .await
            .unwrap()
            .status,
        "expired"
    );
    assert!(
        f.access
            .access_requests(
                &f.owner.id,
                &f.actor(),
                &AccessPageQuery {
                    status: Some("pending".into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    assert_eq!(
        f.access
            .access_requests(
                &f.owner.id,
                &f.actor(),
                &AccessPageQuery {
                    status: Some("expired".into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .rows
            .len(),
        1
    );
    assert!(
        f.access
            .access_grants(
                &f.owner.id,
                &f.actor(),
                &AccessPageQuery {
                    live: true,
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    assert_eq!(
        f.access
            .access_grants(&f.owner.id, &f.actor(), &AccessPageQuery::default())
            .await
            .unwrap()
            .rows[0]["live"],
        false
    );
    for query in [
        AccessPageQuery {
            after: Some("broken".into()),
            ..Default::default()
        },
        AccessPageQuery {
            limit: Some(101),
            ..Default::default()
        },
        AccessPageQuery {
            status: Some("unknown".into()),
            ..Default::default()
        },
    ] {
        assert_eq!(
            f.access
                .access_requests(&f.owner.id, &f.actor(), &query)
                .await
                .unwrap_err()
                .code,
            "invalid"
        );
    }
}
