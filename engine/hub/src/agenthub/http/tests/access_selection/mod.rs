//! Selection through real MCP credentials, hosted boards and durable grants.
use super::{
    access_execution::{counts, granted, person},
    access_reads::{requested, token},
    access_requests::connect,
    access_run::{admit_start, answer, next, view},
    principals::Admin,
    *,
};
use crate::{
    catalog::access::MachineFacts,
    identity::accounts::api_keys::Scope,
    machines::access::{Access, Decision, Narrow},
};
use devagotchi_agenthub::{Actor, Board, Lease, NewTicket, policy::Identity};
use devagotchi_contracts::*;
use std::sync::Arc;
mod fixture;
use fixture::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_selection_mcp_skips_reclaimed_task_and_uses_later_live_grant() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let old = task_grant(&admin, "delivery", 1).await;
    old.release().await;
    old.board
        .hosted()
        .claim(&old.actor(), &old.lease.ticket.id, None)
        .await
        .unwrap();
    let live = task_grant(&admin, "delivery", 2).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let (result, _) = tokio::join!(
        admin
            .hub
            .call(&mut agent, "operation_start", command("fresh")),
        admit_start(&admin, &live.grant, &mut calls)
    );
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(counts(&admin, &old.grant).await, (0, 0));
    assert_eq!(counts(&admin, &live.grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_selection_mcp_filters_projects_and_keeps_retry_affinity() {
    let admin = or_skip!(Admin::new());
    let unbound = granted(&admin).await;
    rank(&admin, &unbound, 1).await;
    let other = task_grant(&admin, "other", 2).await;
    let allowed = task_grant(&admin, "delivery", 3).await;
    let (_, bearer) = token(
        &admin,
        vec![Scope::MachinesRun],
        Some("lab"),
        Some("delivery"),
        true,
    )
    .await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let (result, _) = tokio::join!(
        admin
            .hub
            .call(&mut agent, "operation_start", command("fixed-retry")),
        admit_start(&admin, &allowed.grant, &mut calls)
    );
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(counts(&admin, &unbound).await, (0, 0));
    assert_eq!(counts(&admin, &other.grant).await, (0, 0));
    let (_, bearer) = token(
        &admin,
        vec![Scope::MachinesRun],
        Some("lab"),
        Some("other"),
        true,
    )
    .await;
    let result = admin
        .hub
        .call(
            &mut person(bearer),
            "operation_start",
            command("fixed-retry"),
        )
        .await;
    assert_eq!(
        result["structuredContent"]["code"], "key_denied",
        "{result}"
    );
    assert_eq!(counts(&admin, &allowed.grant).await, (1, 1));
    assert_eq!(counts(&admin, &other.grant).await, (0, 0));
    assert!(
        calls.try_recv().is_err(),
        "retry contacted broker or moved to another grant"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_selection_mcp_rechecks_task_after_prepare_without_repreparing() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let old = task_grant(&admin, "delivery", 1).await;
    let live = task_grant(&admin, "delivery", 2).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let broker = async {
        let (id, request) = next(&mut calls).await;
        assert!(matches!(request, RemoteRequest::Capabilities {}));
        answer(
            &admin,
            id,
            RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                os: "linux".into(),
                sandbox_levels: vec![GrantLevel::ReadOnly],
            },
        );
        let (id, request) = next(&mut calls).await;
        let RemoteRequest::Prepare {
            request: RequestBody::OperationStart(input),
            ..
        } = request
        else {
            panic!("expected preparation")
        };
        old.release().await;
        let mut call = GrantCall::new(GrantCapability::Exec, GrantLevel::ReadOnly);
        call.argv = vec!["/usr/bin/git".into(), "status".into()];
        call.paths = vec!["/work".into()];
        call.runtime_s = Some(5);
        answer(
            &admin,
            id,
            RemoteReply::Prepared {
                action: GrantAction::Exec,
                operation_key: input.idempotency_key,
                input_sha256: "a".repeat(64),
                call,
            },
        );
        let (id, request) = next(&mut calls).await;
        let RemoteRequest::Execute { authority, .. } = request else {
            panic!("selection prepared twice instead of reusing canonical call")
        };
        assert_eq!(authority.binding.grant_id, live.grant);
        assert_eq!(counts(&admin, &old.grant).await, (0, 0));
        answer(
            &admin,
            id,
            RemoteReply::Executed(ResponseBody::Start(StartOutput {
                operation: view(false),
                replayed: false,
            })),
        );
    };
    let (result, ()) = tokio::join!(
        admin
            .hub
            .call(&mut agent, "operation_start", command("release-race")),
        broker
    );
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(counts(&admin, &live.grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_selection_mcp_out_of_scope_never_prepares() {
    let admin = or_skip!(Admin::new());
    let unbound = granted(&admin).await;
    let (_, bearer) = token(
        &admin,
        vec![Scope::MachinesRun],
        Some("lab"),
        Some("delivery"),
        true,
    )
    .await;
    let mut calls = connect(&admin).await;
    let result = admin
        .hub
        .call(
            &mut person(bearer),
            "operation_start",
            command("no-project"),
        )
        .await;
    assert_eq!(
        result["structuredContent"]["code"], "grant_required",
        "{result}"
    );
    assert_eq!(counts(&admin, &unbound).await, (0, 0));
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_selection_mcp_corrupt_task_identity_is_not_a_fallback() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let corrupt = task_grant(&admin, "delivery", 1).await;
    let live = task_grant(&admin, "delivery", 2).await;
    let accounts = &admin.hub.accounts;
    let owner = accounts.get(&admin.owner).await.unwrap().unwrap();
    devagotchi_store::transaction!(accounts.store(), |c| {
        accounts.store().scope(c, &owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET task_binding=jsonb_set(task_binding,'{tenant}','\"foreign\"'::jsonb) WHERE tenant_id=$1 AND id=$2")
            .bind(&owner.tenant).bind(&corrupt.grant).execute(c).await?;
        Ok(())
    }).unwrap();
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut calls = connect(&admin).await;
    let result = admin
        .hub
        .call(
            &mut person(bearer),
            "operation_start",
            command("bad-identity"),
        )
        .await;
    assert_eq!(
        result["structuredContent"]["code"], "authority_mismatch",
        "{result}"
    );
    assert_eq!(counts(&admin, &corrupt.grant).await, (0, 0));
    assert_eq!(counts(&admin, &live.grant).await, (0, 0));
    assert!(calls.try_recv().is_err());
}
