//! Explicit process heartbeats retain the original admission and current credentials.
use super::{
    access_execution::{counts, person},
    access_reads::{requested, token},
    access_requests::connect,
    access_run::{answer, next, view},
    principals::Admin,
    *,
};
use crate::{identity::accounts::api_keys::Scope, tenancy::tests::tenants::Person};
use devagotchi_contracts::*;
use tokio::sync::mpsc;
type Calls = mpsc::Receiver<(u64, RemoteWireRequest)>;
mod fixture;
use fixture::*;

fn input() -> Value {
    json!({"machine":"lab","operation_id":"observed-process"})
}

async fn heartbeat_race(phase: &str) {
    let admin = or_skip!(Admin::new());
    let grant = process_grant(&admin).await;
    let (key, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let original = start(&admin, &grant, &mut agent, &mut calls).await;
    let before = deadline(&admin, &grant).await;
    let broker = async {
        let (id, request) = next(&mut calls).await;
        assert!(matches!(request, RemoteRequest::Capabilities {}));
        if phase == "prepare" {
            revoke(&admin, &key).await;
        }
        capabilities(&admin, id);
        if phase == "prepare" {
            return;
        }
        let (id, request) = next(&mut calls).await;
        let RemoteRequest::Refresh { authority } = request else {
            panic!("heartbeat sent another operation")
        };
        assert_eq!(authority.binding, original.binding);
        assert!(authority.expires_at >= original.expires_at);
        assert_eq!(counts(&admin, &grant).await, (1, 1));
        if phase == "receipt" {
            revoke(&admin, &key).await;
        }
        answer(
            &admin,
            id,
            RemoteReply::Refreshed {
                operation_id: if phase == "mismatch" {
                    "foreign"
                } else {
                    "observed-process"
                }
                .into(),
            },
        );
    };
    let (result, ()) = tokio::join!(
        admin.hub.call(&mut agent, "operation_heartbeat", input()),
        broker
    );
    match phase {
        "prepare" | "receipt" => assert_eq!(
            result["structuredContent"]["code"], "unauthenticated",
            "{result}"
        ),
        "mismatch" => assert_eq!(
            result["structuredContent"]["code"], "invalid_response",
            "{result}"
        ),
        _ => {
            assert_ne!(result["isError"], true, "{result}");
            assert_eq!(
                result["structuredContent"]["operation_id"],
                "observed-process"
            );
            assert_eq!(
                result["structuredContent"]["expires_at_ms"],
                deadline(&admin, &grant).await
            );
        }
    }
    if phase == "prepare" {
        assert_eq!(deadline(&admin, &grant).await, before);
    }
    assert_eq!(counts(&admin, &grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_renews_original_process_without_new_use() {
    heartbeat_race("ok").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_key_revoked_during_capabilities_cannot_renew() {
    heartbeat_race("prepare").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_revoked_caller_cannot_receive_renewal_receipt() {
    heartbeat_race("receipt").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_mismatched_receipt_is_not_success() {
    heartbeat_race("mismatch").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_unknown_operation_refuses_before_contacting_broker() {
    let admin = or_skip!(Admin::new());
    process_grant(&admin).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut calls = connect(&admin).await;
    let result = admin
        .hub
        .call(&mut person(bearer), "operation_heartbeat", input())
        .await;
    assert_eq!(result["structuredContent"]["code"], "not_found", "{result}");
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_expired_authority_is_not_revived() {
    let admin = or_skip!(Admin::new());
    let grant = process_grant(&admin).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    start(&admin, &grant, &mut agent, &mut calls).await;
    expire(&admin, &grant).await;
    let broker = async {
        let (id, request) = next(&mut calls).await;
        assert!(matches!(request, RemoteRequest::Capabilities {}));
        capabilities(&admin, id);
    };
    let (result, ()) = tokio::join!(
        admin.hub.call(&mut agent, "operation_heartbeat", input()),
        broker
    );
    assert_eq!(
        result["structuredContent"]["code"], "authority_expired",
        "{result}"
    );
    assert_eq!(deadline(&admin, &grant).await, 1);
    assert_eq!(counts(&admin, &grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_mcp_current_heartbeat_policy_is_checked_atomically() {
    let admin = or_skip!(Admin::new());
    let grant = process_grant(&admin).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    start(&admin, &grant, &mut agent, &mut calls).await;
    let before = deadline(&admin, &grant).await;
    deny_heartbeat(&admin).await;
    let broker = async {
        let (id, request) = next(&mut calls).await;
        assert!(matches!(request, RemoteRequest::Capabilities {}));
        capabilities(&admin, id);
    };
    let (result, ()) = tokio::join!(
        admin.hub.call(&mut agent, "operation_heartbeat", input()),
        broker
    );
    assert_eq!(
        result["structuredContent"]["code"], "policy_denied",
        "{result}"
    );
    assert_eq!(deadline(&admin, &grant).await, before);
    assert_eq!(counts(&admin, &grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}
