//! MCP process control cannot borrow another operation or use the legacy transport.
use super::{
    access_execution::{counts, granted, person},
    access_reads::token,
    access_requests::connect,
    access_run::{admit_start, answer, next, view},
    principals::Admin,
    *,
};
use crate::identity::accounts::api_keys::Scope;
use devagotchi_contracts::*;

async fn control_race(revoke: bool) {
    let admin = or_skip!(Admin::new());
    let grant = granted(&admin).await;
    let (key, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let start=admin.hub.call(&mut agent,"operation_start",json!({"machine":"lab",
        "idempotency_key":"control-start","command":"git status","executable":"","cwd":"/work","deadline_ms":5000}));
    let (result, authority) = tokio::join!(start, admit_start(&admin, &grant, &mut calls));
    assert_ne!(result["isError"], true, "{result}");
    let broker = async {
        let (id, request) = next(&mut calls).await;
        let RemoteRequest::Control {
            control,
            request: RequestBody::OperationCancel(input),
            ..
        } = request
        else {
            panic!("cancel bypassed control authorization")
        };
        assert_eq!(control.action, ProcessControl::Cancel);
        assert_eq!(control.binding, authority.binding);
        assert!(control.expires_at <= authority.expires_at);
        assert_eq!(input.operation_id, "observed-process");
        if revoke {
            let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
            admin
                .hub
                .accounts
                .revoke_key(&owner.tenant, &owner.id, &key)
                .await
                .unwrap();
        }
        let mut operation = view(false);
        operation.cancellation_requested = true;
        answer(
            &admin,
            id,
            RemoteReply::Controlled(ResponseBody::Operation(operation)),
        );
    };
    let (result, ()) = tokio::join!(
        admin.hub.call(
            &mut agent,
            "operation_cancel",
            json!({"machine":"lab","operation_id":"observed-process"})
        ),
        broker
    );
    if revoke {
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(
            result["structuredContent"]["code"], "unauthenticated",
            "{result}"
        );
    } else {
        assert_ne!(result["isError"], true, "{result}");
        assert_eq!(
            result["structuredContent"]["cancellation_requested"], true,
            "{result}"
        );
    }
    assert_eq!(counts(&admin, &grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_control_mcp_cancel_uses_original_admission_without_extra_use() {
    control_race(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_control_mcp_revoked_caller_does_not_receive_an_effect_receipt() {
    control_race(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_control_mcp_unknown_operation_never_reaches_broker() {
    let admin = or_skip!(Admin::new());
    granted(&admin).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    for (tool, args) in [
        (
            "operation_cancel",
            json!({"machine":"lab","operation_id":"foreign"}),
        ),
        (
            "operation_input",
            json!({"machine":"lab","operation_id":"foreign","text":"do not deliver"}),
        ),
        (
            "operation_resize",
            json!({"machine":"lab","operation_id":"foreign","cols":80,"rows":24}),
        ),
    ] {
        let result = admin.hub.call(&mut agent, tool, args).await;
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(result["structuredContent"]["code"], "not_found", "{result}");
    }
    assert!(calls.try_recv().is_err());
}
