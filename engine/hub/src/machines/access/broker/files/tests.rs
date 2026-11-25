use super::*;
use crate::{
    machines::access::{
        Decision, Narrow,
        broker_tests::links,
        request_tests::{Fixture, input},
    },
    store,
};
use tokio::sync::mpsc;

async fn grant(f: &Fixture, capability: GrantCapability) -> String {
    let mut request = input();
    request.duration = Some(GrantDuration::Once);
    request.selection = crate::catalog::access::Selection::Items {
        items: vec![GrantItem {
            capability,
            sandbox_level: GrantLevel::WorkspaceWrite,
            selector: GrantSelector {
                paths: Some(vec!["/work".into()]),
                ..Default::default()
            },
        }],
    };
    let pending = f.request(&request, "file-grant").await.unwrap();
    f.decide(
        &pending.result.request_ids[0],
        &Decision::Approve {
            narrow: Narrow::default(),
        },
        "approve-files",
    )
    .await
    .unwrap()
    .result
    .grant_id
    .unwrap()
}
fn selection<'a>(f: &'a Fixture, actor: &'a Actor<'a>, links: &'a Links) -> StartSelection<'a> {
    StartSelection {
        account: &f.owner.id,
        actor,
        machine: &f.machine,
        links,
        client: "files-test",
        boards: None,
        projects: &[],
    }
}
async fn received(rx: &mut mpsc::Receiver<(u64, RemoteWireRequest)>) -> (u64, RemoteWireRequest) {
    tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
}
async fn prepare_reply(
    links: &Links,
    rx: &mut mpsc::Receiver<(u64, RemoteWireRequest)>,
    input: &FileInvocation,
) {
    let (id, wire) = received(rx).await;
    assert!(matches!(wire.remote, RemoteRequest::FileCapabilities {}));
    links.answer_remote_for_test(
        "lab",
        id,
        RemoteWireResponse::Ok(RemoteReply::FileCapabilities {
            version: REMOTE_FILE_ACCESS_VERSION,
            sandbox_levels: vec![GrantLevel::WorkspaceWrite],
        }),
    );
    let (id, wire) = received(rx).await;
    let RemoteRequest::PrepareFiles { invocation, .. } = wire.remote else {
        panic!("expected file preparation")
    };
    assert_eq!(invocation.key, input.key);
    let mut call = GrantCall::new(input.action().capability(), GrantLevel::WorkspaceWrite);
    call.paths = input.paths().into_iter().map(str::to_owned).collect();
    links.answer_remote_for_test(
        "lab",
        id,
        RemoteWireResponse::Ok(RemoteReply::Prepared {
            action: input.action(),
            operation_key: input.key.clone(),
            input_sha256: "c".repeat(64),
            call,
        }),
    );
}
async fn selected(
    f: &Fixture,
    links: &Links,
    rx: &mut mpsc::Receiver<(u64, RemoteWireRequest)>,
    input: &FileInvocation,
) -> PreparedCall {
    let actor = f.actor();
    let selection = selection(f, &actor, links);
    let (result, ()) = tokio::join!(
        f.access.prepare_selected_files(&selection, input.clone()),
        prepare_reply(links, rx, input)
    );
    result.unwrap()
}
async fn executed(
    f: &Fixture,
    links: &Links,
    rx: &mut mpsc::Receiver<(u64, RemoteWireRequest)>,
    prepared: &PreparedCall,
    step: FileStep,
    outcome: FileOutcome,
) -> Admitted {
    let actor = f.actor();
    let execution = LinkExecution {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        links,
        grant_id: prepared.grant_id(),
        client: "files-test",
    };
    let server = async {
        let (id, wire) = received(rx).await;
        let RemoteRequest::ExecuteFiles { authority, .. } = wire.remote else {
            panic!("expected file execution")
        };
        assert_eq!(authority.binding.operation_key, prepared.operation_key);
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::FileExecuted {
                operation_id: format!("file-{}", prepared.operation_key),
                outcome,
                replayed: false,
            }),
        );
    };
    let (result, ()) = tokio::join!(
        f.access.execute_prepared_files(&execution, prepared, step),
        server
    );
    result.unwrap().0
}

#[tokio::test]
async fn file_batch_uses_write_grant_coverage_once_and_finishes_one_intent() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = grant(&f, GrantCapability::FileWrite).await;
    let (links, mut rx) = links(&f).await;
    let input = FileInvocation {
        key: "batch".into(),
        intent: FileIntent::Batch {
            paths: vec!["/work/a".into(), "/work/b".into()],
            max_bytes: 10,
        },
    };
    let prepared = selected(&f, &links, &mut rx, &input).await;
    assert_eq!(prepared.grant_id(), grant);
    assert_eq!(
        prepared.action,
        GrantAction::FileRead,
        "policy action stays fs.read even under a write grant"
    );
    let mut admitted = None;
    for path in ["/work/a", "/work/b"] {
        let step = FileStep::Read {
            input: FileReadInput {
                path: path.into(),
                offset: 0,
                max_bytes: 10,
                start_line: None,
                line_count: None,
            },
        };
        // A missing file is still part of the same admitted batch, never another use.
        admitted = Some(
            executed(
                &f,
                &links,
                &mut rx,
                &prepared,
                step,
                FileOutcome::Error(BridgeError::new("path_not_found", "missing")),
            )
            .await,
        );
    }
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    let actor = f.actor();
    let execution = LinkExecution {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        links: &links,
        grant_id: &grant,
        client: "files-test",
    };
    let admitted = admitted.unwrap();
    let server = async {
        let (id, wire) = received(&mut rx).await;
        assert!(matches!(wire.remote, RemoteRequest::FinishFiles { .. }));
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::FileFinished {
                error_code: None,
                operation_id: "file-batch".into(),
            }),
        );
    };
    let (finished, ()) = tokio::join!(
        f.access
            .finish_prepared_files(&execution, &prepared, &admitted),
        server
    );
    finished.unwrap();
    f.access
        .check_finished_files(&execution, &prepared, &admitted)
        .await
        .unwrap();
    let retry = selected(&f, &links, &mut rx, &input).await;
    assert_eq!(
        f.access
            .check_prepared_files(&execution, &retry)
            .await
            .unwrap_err()
            .code,
        "operation_finished"
    );
    let other = FileInvocation {
        key: "another".into(),
        ..input
    };
    assert_eq!(
        f.access
            .prepare_selected_files(&selection(&f, &actor, &links), other)
            .await
            .unwrap_err()
            .code,
        "grant_required"
    );
    assert!(
        rx.try_recv().is_err(),
        "exhausted grant makes no broker request"
    );
    f.access
        .revoke(&f.owner.id, &grant, 1, "ended-after-finish")
        .await
        .unwrap();
    assert_eq!(
        f.access
            .check_finished_files(&execution, &prepared, &admitted)
            .await
            .unwrap_err()
            .code,
        "grant_ended"
    );
}

#[tokio::test]
async fn file_mutation_finished_retry_reuses_original_authority_and_revocation_still_refuses() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = grant(&f, GrantCapability::FileWrite).await;
    let (links, mut rx) = links(&f).await;
    let input = FileInvocation {
        key: "delete".into(),
        intent: FileIntent::Operation {
            operation: FileOperation::Delete(FileDeleteInput {
                path: "/work/a".into(),
                expected_sha256: "d".repeat(64),
            }),
        },
    };
    let prepared = selected(&f, &links, &mut rx, &input).await;
    let outcome = FileOutcome::Ok(ResponseBody::Deleted(FileDeleted {
        path: "/work/a".into(),
        sha256: "d".repeat(64),
    }));
    let first = executed(
        &f,
        &links,
        &mut rx,
        &prepared,
        FileStep::Operation {},
        outcome.clone(),
    )
    .await;
    let retry = selected(&f, &links, &mut rx, &input).await;
    let again = executed(&f, &links, &mut rx, &retry, FileStep::Operation {}, outcome).await;
    assert_eq!(first.admission_id, again.admission_id);
    assert!(again.replayed);
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    f.access
        .revoke(&f.owner.id, &grant, 1, "stop")
        .await
        .unwrap();
    let actor = f.actor();
    let execution = LinkExecution {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        links: &links,
        grant_id: &grant,
        client: "files-test",
    };
    assert_eq!(
        f.access
            .check_prepared_files(&execution, &retry)
            .await
            .unwrap_err()
            .code,
        "grant_ended"
    );
    assert!(rx.try_recv().is_err());
}
