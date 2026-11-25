use super::*;
use crate::{
    machines::access::{
        Decision, Narrow,
        admission_tests::call,
        broker_tests::links,
        request_tests::{Fixture, input},
    },
    store,
};
use tokio::sync::mpsc;

fn start(key: &str) -> StartInput {
    StartInput {
        idempotency_key: key.into(),
        executable: "git".into(),
        args: vec!["status".into()],
        command: None,
        cwd: Some("/work".into()),
        deadline_ms: 5000,
        interactive: None,
        combined_output: false,
        caller: None,
    }
}
async fn prepare(links: &Links, rx: &mut mpsc::Receiver<(u64, RemoteWireRequest)>, key: &str) {
    let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(wire.remote, RemoteRequest::Capabilities {}));
    links.answer_remote_for_test(
        "lab",
        id,
        RemoteWireResponse::Ok(RemoteReply::Capabilities {
            version: REMOTE_ACCESS_VERSION,
            os: "linux".into(),
            sandbox_levels: vec![GrantLevel::ReadOnly],
        }),
    );
    let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(wire.remote, RemoteRequest::Prepare { .. }));
    links.answer_remote_for_test(
        "lab",
        id,
        RemoteWireResponse::Ok(RemoteReply::Prepared {
            action: GrantAction::Exec,
            operation_key: key.into(),
            input_sha256: "a".repeat(64),
            call: call(),
        }),
    );
}
fn selection<'a>(f: &'a Fixture, actor: &'a Actor<'a>, links: &'a Links) -> StartSelection<'a> {
    StartSelection {
        account: &f.owner.id,
        actor,
        machine: &f.machine,
        links,
        client: "selection-test",
        boards: None,
        projects: &[],
    }
}

#[tokio::test]
async fn access_start_selection_crosses_page_boundary_and_prepares_only_once_per_level() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let mut matching = String::new();
    for index in 0..101 {
        let mut request = input();
        if let crate::catalog::access::Selection::Items { ref mut items } = request.selection {
            items[0].selector.paths = Some(vec![if index == 100 {
                "/work".into()
            } else {
                format!("/other/{index}")
            }]);
        } else {
            panic!("expected explicit source items")
        }
        let pending = f
            .request(&request, &format!("request-{index}"))
            .await
            .unwrap();
        let grant = f
            .decide(
                &pending.result.request_ids[0],
                &Decision::Approve {
                    narrow: Narrow::default(),
                },
                &format!("approve-{index}"),
            )
            .await
            .unwrap()
            .result
            .grant_id
            .unwrap();
        // Deterministic source approval order, independent of wall-clock resolution.
        transaction!(&f.access.accounts.store, |c| {
            f.access.accounts.store.scope(c, &f.owner.tenant).await?;
            sqlx::query("UPDATE machine_grants SET approved_at=$3 WHERE tenant_id=$1 AND id=$2")
                .bind(&f.owner.tenant)
                .bind(&grant)
                .bind(i64::from(index + 1))
                .execute(c)
                .await?;
            Ok(())
        })
        .unwrap();
        if index == 100 {
            matching = grant;
        }
    }
    let (links, mut rx) = links(&f).await;
    let actor = f.actor();
    let selected = selection(&f, &actor, &links);
    let (result, ()) = tokio::join!(
        f.access.prepare_selected_start(&selected, start("pages")),
        prepare(&links, &mut rx, "pages")
    );
    let prepared = result.unwrap();
    assert_eq!(prepared.grant_id(), matching);
    assert_eq!(f.admission_counts(&matching).await, (0, 0, 0));
    assert!(
        rx.try_recv().is_err(),
        "selection repeated broker preparation or dispatched work"
    );
    let wrong = LinkExecution {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        links: &links,
        grant_id: &matching,
        client: "different-client",
    };
    assert_eq!(
        f.access
            .execute_prepared(&wrong, prepared)
            .await
            .unwrap_err()
            .code,
        "authority_mismatch"
    );
    assert_eq!(f.admission_counts(&matching).await, (0, 0, 0));
}

#[tokio::test]
async fn access_start_selection_retry_keeps_exhausted_original_grant() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    f.admit(&grant, "same-retry", &call()).await.unwrap();
    let (links, mut rx) = links(&f).await;
    let actor = f.actor();
    let selected = selection(&f, &actor, &links);
    let (result, ()) = tokio::join!(
        f.access
            .prepare_selected_start(&selected, start("same-retry")),
        prepare(&links, &mut rx, "same-retry")
    );
    assert_eq!(result.unwrap().grant_id(), grant);
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    assert_eq!(
        f.access
            .prepare_selected_start(&selected, start("new-retry"))
            .await
            .unwrap_err()
            .code,
        "grant_required"
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn access_start_selection_damaged_candidate_does_not_fall_through() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let damaged = f.once().await;
    let pending = f.request(&input(), "later-request").await.unwrap();
    let live = f
        .decide(
            &pending.result.request_ids[0],
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            "later-approve",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap();
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET approved_at=1,task_binding='{}'::jsonb WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant).bind(&damaged).execute(c).await?;
        Ok(())
    }).unwrap();
    let (links, mut rx) = links(&f).await;
    let actor = f.actor();
    let error = f
        .access
        .prepare_selected_start(&selection(&f, &actor, &links), start("damaged"))
        .await
        .unwrap_err();
    assert_eq!(error.code, "access_damaged");
    assert_eq!(f.admission_counts(&damaged).await, (0, 0, 0));
    assert_eq!(f.admission_counts(&live).await, (0, 0, 0));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn access_start_selection_eligibility_propagates_database_failure() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let actor = f.actor();
    let target = f
        .access
        .execution_target(&f.owner.id, &actor, &grant)
        .await
        .unwrap();
    let (links, mut rx) = links(&f).await;
    // This fixture owns an isolated database/pool. Fail the candidate's current-state
    // read after its metadata resolved; an unavailable store is never an ineligible grant.
    f.access.accounts.store.pool().close().await;
    let error = f
        .access
        .start_candidate_live(
            &selection(&f, &actor, &links),
            &grant,
            &target,
            GrantAction::Exec,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "account_store");
    assert!(rx.try_recv().is_err());
}
