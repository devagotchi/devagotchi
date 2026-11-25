//! Fresh credentials and grant checks across actual MCP file waits.
use super::{
    access_execution::{counts, person},
    access_reads::{requested_for, token},
    access_requests::connect,
    access_run::{answer, next},
    principals::Admin,
    *,
};
use crate::{identity::accounts::api_keys::Scope, machines::access::Access};
use devagotchi_contracts::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Prepare,
    Page,
    Finish,
}

async fn end(admin: &Admin, key: &str, grant: &str, revoke_grant: bool) {
    if revoke_grant {
        Access::new(admin.hub.accounts.clone())
            .revoke(&admin.owner, grant, 1, "file-test-end")
            .await
            .unwrap();
    } else {
        let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
        admin
            .hub
            .accounts
            .revoke_key(&owner.tenant, &owner.id, key)
            .await
            .unwrap();
    }
}
fn content(path: String) -> FileOutcome {
    FileOutcome::Ok(ResponseBody::File(
        serde_json::from_value(json!({
            "path":path,"size":7,"offset":0,"text":"private","next_offset":7,"eof":true
        }))
        .unwrap(),
    ))
}
async fn race(stage: Stage, revoke_grant: bool) {
    let admin = or_skip!(Admin::new());
    let pending = requested_for(&admin, "fs.read").await;
    let (status, value, _) = response(
        admin
            .request(
                Method::POST,
                &format!("/api/v1/access-requests/{}/approve", pending.request_ids[0]),
            )
            .header("idempotency-key", "file-approve")
            .json(&json!({"version":1,"duration":"once"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    let grant = value["result"]["grant_id"].as_str().unwrap().to_owned();
    let (key, bearer) = token(&admin, vec![Scope::FilesRead], None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let broker = async {
        let (id, request) = next(&mut calls).await;
        assert!(matches!(request, RemoteRequest::FileCapabilities {}));
        answer(
            &admin,
            id,
            RemoteReply::FileCapabilities {
                version: REMOTE_FILE_ACCESS_VERSION,
                sandbox_levels: vec![GrantLevel::ReadOnly],
            },
        );
        let (id, request) = next(&mut calls).await;
        let RemoteRequest::PrepareFiles { invocation, .. } = request else {
            panic!("file prepare")
        };
        if stage == Stage::Prepare {
            end(&admin, &key, &grant, revoke_grant).await;
        }
        let mut call = GrantCall::new(GrantCapability::FileRead, GrantLevel::ReadOnly);
        call.paths = invocation.paths().into_iter().map(str::to_owned).collect();
        answer(
            &admin,
            id,
            RemoteReply::Prepared {
                action: GrantAction::FileRead,
                operation_key: invocation.key,
                input_sha256: "a".repeat(64),
                call,
            },
        );
        if stage == Stage::Prepare {
            return;
        }
        for page in 0..2 {
            let (id, request) = next(&mut calls).await;
            let RemoteRequest::ExecuteFiles {
                step: FileStep::Read { input },
                ..
            } = request
            else {
                panic!("file page")
            };
            if page == 1 && stage == Stage::Page {
                end(&admin, &key, &grant, revoke_grant).await;
            }
            answer(
                &admin,
                id,
                RemoteReply::FileExecuted {
                    operation_id: "file-batch".into(),
                    outcome: content(input.path),
                    replayed: false,
                },
            );
        }
        if stage == Stage::Finish || revoke_grant {
            let (id, request) = next(&mut calls).await;
            assert!(matches!(request, RemoteRequest::FinishFiles { .. }));
            if stage == Stage::Finish {
                end(&admin, &key, &grant, revoke_grant).await;
            }
            answer(
                &admin,
                id,
                RemoteReply::FileFinished {
                    operation_id: "file-batch".into(),
                    error_code: None,
                },
            );
        }
    };
    let (result, ()) = tokio::join!(
        admin.hub.call(
            &mut agent,
            "read_multiple_files",
            json!({"machine":"lab","paths":["/work/a","/work/b"]})
        ),
        broker
    );
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        !result.to_string().contains("private"),
        "earlier bytes leaked: {result}"
    );
    assert_eq!(
        counts(&admin, &grant).await,
        if stage == Stage::Prepare {
            (0, 0)
        } else {
            (1, 1)
        }
    );
    assert!(
        calls.try_recv().is_err(),
        "unexpected file dispatch after revocation"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_mcp_credential_revocation_during_prepare_prevents_admission() {
    race(Stage::Prepare, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_mcp_credential_revocation_during_page_discards_earlier_bytes() {
    race(Stage::Page, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_mcp_credential_revocation_during_finish_discards_earlier_bytes() {
    race(Stage::Finish, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_mcp_grant_revocation_during_prepare_prevents_admission() {
    race(Stage::Prepare, true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_mcp_grant_revocation_during_page_discards_earlier_bytes() {
    race(Stage::Page, true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_mcp_grant_revocation_during_finish_discards_earlier_bytes() {
    race(Stage::Finish, true).await;
}
