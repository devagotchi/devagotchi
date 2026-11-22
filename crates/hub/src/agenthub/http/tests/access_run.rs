//! Actual MCP run must admit its start and independently authorize every output read.
use super::{
    access_execution::{counts, granted, person},
    access_reads::{requested, token},
    access_requests::connect,
    principals::Admin,
    *,
};
use crate::identity::accounts::api_keys::Scope;
use devagotchi_contracts::*;
use tokio::sync::mpsc;

fn command() -> Value {
    json!({"machine":"lab","command":"git status","cwd":"/work",
    "wait_seconds":5,"timeout_seconds":5})
}
fn view(terminal: bool) -> OperationView {
    OperationView {
        operation_id: "observed-process".into(),
        executable: "/usr/bin/git".into(),
        args: vec!["status".into()],
        cwd: "/work".into(),
        deadline_ms: 5000,
        state: if terminal {
            OperationState::Succeeded
        } else {
            OperationState::Running
        },
        created_at_ms: 1,
        updated_at_ms: 2,
        exit_code: terminal.then_some(0),
        detail: None,
        ownership_scope: "test".into(),
        cancellation_requested: false,
        output_complete: terminal,
        interactive: None,
    }
}
async fn next(calls: &mut mpsc::Receiver<(u64, RemoteWireRequest)>) -> (u64, RemoteRequest) {
    let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(10), calls.recv())
        .await
        .unwrap()
        .unwrap();
    (id, wire.remote)
}
fn answer(admin: &Admin, id: u64, reply: RemoteReply) {
    admin
        .hub
        .hub
        .links
        .answer_remote_for_test("lab", id, RemoteWireResponse::Ok(reply));
}
async fn run_race(revoke: bool) {
    let admin = or_skip!(Admin::new());
    let grant = granted(&admin).await;
    // A run-only key can retrieve the result of its own admitted run.
    let (key, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
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
            panic!("expected prepared run")
        };
        assert_eq!(input.command.as_deref(), Some("git status"));
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
        let RemoteRequest::Execute {
            authority,
            request: RequestBody::OperationStart(_),
            ..
        } = request
        else {
            panic!("run bypassed admission")
        };
        assert_eq!(authority.binding.grant_id, grant);
        assert_eq!(counts(&admin, &grant).await, (1, 1));
        answer(
            &admin,
            id,
            RemoteReply::Executed(ResponseBody::Start(StartOutput {
                operation: view(false),
                replayed: false,
            })),
        );
        // First wait expires its observation slice; the adapter must get fresh permission.
        for ready in [false, true] {
            let (id, request) = next(&mut calls).await;
            let RemoteRequest::Observe {
                observation,
                request: RequestBody::OperationWait(wait),
                ..
            } = request
            else {
                panic!("wait bypassed observation")
            };
            assert_eq!(observation.binding, authority.binding);
            assert_eq!(wait.operation_id, "observed-process");
            observation.remaining_ms(observation.issued_at).unwrap();
            answer(
                &admin,
                id,
                RemoteReply::Observed {
                    response: ResponseBody::Operation(view(ready)),
                    ready,
                },
            );
        }
        let (id, request) = next(&mut calls).await;
        let RemoteRequest::Observe {
            observation,
            request: RequestBody::OperationRead(read),
            ..
        } = request
        else {
            panic!("read bypassed observation")
        };
        assert_eq!(observation.binding, authority.binding);
        assert_eq!(read.operation_id, "observed-process");
        if revoke {
            let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
            admin
                .hub
                .accounts
                .revoke_key(&owner.tenant, &owner.id, &key)
                .await
                .unwrap();
        }
        answer(
            &admin,
            id,
            RemoteReply::Observed {
                ready: true,
                response: ResponseBody::Output(OutputPage {
                    operation_id: "observed-process".into(),
                    frames: vec![OutputFrame {
                        sequence: 1,
                        stream: "stdout".into(),
                        bytes_base64: "cHJpdmF0ZS1vdXRwdXQ=".into(),
                        text: "private-output".into(),
                    }],
                    next_cursor: 1,
                    gap: false,
                    earliest_retained_sequence: 1,
                    latest_sequence: 1,
                    has_more: false,
                    operation: view(true),
                }),
            },
        );
    };
    let (result, ()) = tokio::join!(admin.hub.call(&mut agent, "run", command()), broker);
    if revoke {
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(
            result["structuredContent"]["code"], "unauthenticated",
            "{result}"
        );
        assert!(!result.to_string().contains("private-output"));
    } else {
        assert_ne!(result["isError"], true, "{result}");
        assert_eq!(
            result["structuredContent"]["output"], "private-output",
            "{result}"
        );
    }
    assert_eq!(counts(&admin, &grant).await, (1, 1));
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_run_mcp_uses_granted_start_and_observation_without_extra_uses() {
    run_race(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_run_mcp_revoked_reader_receives_no_output() {
    run_race(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_run_mcp_pending_grant_cannot_dispatch() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRun], None, None, true).await;
    let mut calls = connect(&admin).await;
    let result = admin.hub.call(&mut person(bearer), "run", command()).await;
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(
        result["structuredContent"]["code"], "grant_required",
        "{result}"
    );
    assert!(calls.try_recv().is_err());
}
