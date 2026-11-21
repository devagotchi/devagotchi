use super::*;
use super::{
    admission_action::AdmissionAction,
    admission_tests::call,
    admissions::{AdmissionInput, AdmissionMachine},
};
use super::{
    decisions::Decision,
    request_tests::{Fixture, input},
    requests::TaskRequest,
};
use crate::{catalog::access::MachineFacts, identity::accounts::agents::Directory, store};
use devagotchi_agenthub::{Actor, NewTicket, hosted::HostedBoard};
use devagotchi_contracts::GrantDuration;
use devagotchi_contracts::GrantLevel;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_task_approval_requires_the_original_lease_after_release_and_reclaim() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let board = HostedBoard::for_owner(
        f.access.accounts.store.clone(),
        &f.owner.tenant,
        &f.owner.id,
        "work",
    )
    .unwrap()
    .with_principals(Arc::new(Directory(Default::default())));
    board.initialize().await.unwrap();
    let ticket = board
        .create(
            &Actor::human(&f.owner.id),
            &NewTicket {
                title: "Inspect repository".into(),
                labels: vec!["review".into()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let lease = board.claim(&f.actor(), &ticket.id, None).await.unwrap();
    let mut request = input();
    request.duration = Some(GrantDuration::Task);
    request.task = Some(TaskRequest {
        project: "work".into(),
        ticket: ticket.id.clone(),
        fence: lease.fence,
    });
    let facts = MachineFacts {
        os: "linux",
        display_server: "x11",
        kind: "native",
    };
    let pending = f
        .access
        .request(
            &f.owner.id,
            &f.actor(),
            Some(&board),
            &f.machine,
            &facts,
            &request,
            "bound",
        )
        .await
        .unwrap();
    board
        .heartbeat(&f.actor(), &ticket.id, lease.fence, None)
        .await
        .unwrap();
    let decision = Decision::Approve {
        narrow: Narrow::default(),
    };
    let first = f
        .access
        .decide(
            &f.owner.id,
            Some(&board),
            &pending.result.request_ids[0],
            1,
            &decision,
            "approve",
        )
        .await
        .unwrap();
    assert!(first.result.grant_id.is_some());
    let call = call();
    let input = AdmissionInput {
        action: AdmissionAction::Exec,
        grant_id: first.result.grant_id.as_ref().unwrap(),
        operation_key: "task-operation",
        input_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        call: &call,
    };
    let machine = AdmissionMachine {
        binding: &f.machine,
        sandbox_levels: &[GrantLevel::ReadOnly],
    };
    let admitted = f
        .access
        .admit(&f.owner.id, &f.actor(), Some(&board), &machine, &input)
        .await
        .unwrap();
    assert_eq!(admitted.task.as_ref().unwrap().lease_id, lease.lease_id);
    assert!(admitted.expires_at <= admitted.task.as_ref().unwrap().expires_at);
    board
        .release(&f.actor(), &ticket.id, lease.fence, "pause")
        .await
        .unwrap();
    let next = board.claim(&f.actor(), &ticket.id, None).await.unwrap();
    assert!(next.fence > lease.fence);
    assert!(
        f.access
            .admit(&f.owner.id, &f.actor(), Some(&board), &machine, &input)
            .await
            .is_err()
    );
    assert!(
        f.access
            .request(
                &f.owner.id,
                &f.actor(),
                Some(&board),
                &f.machine,
                &facts,
                &request,
                "bound"
            )
            .await
            .is_err()
    );
    assert!(
        f.access
            .decide(
                &f.owner.id,
                Some(&board),
                &pending.result.request_ids[0],
                1,
                &decision,
                "approve"
            )
            .await
            .is_err()
    );
}
