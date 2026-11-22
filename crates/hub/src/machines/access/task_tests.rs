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
    .with_principals(Arc::new(Directory(Default::default(), None)));
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
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "task-operation",
        )
        .await
        .unwrap();
    let renewed = f
        .access
        .renew_admission(
            &f.owner.id,
            &f.actor(),
            Some(&board),
            &machine,
            &admitted.admission_id,
        )
        .await
        .unwrap();
    assert_eq!(
        renewed.admitted.task.as_ref().unwrap().lease_id,
        lease.lease_id
    );
    assert!(renewed.admitted.expires_at <= renewed.admitted.task.as_ref().unwrap().expires_at);
    let (control_links, _control_rx) = super::broker_tests::links(&f).await;
    let control_target = f
        .access
        .operation_target(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            "task-operation",
            &control_links,
        )
        .await
        .unwrap();
    f.access
        .operation_control(
            &f.owner.id,
            &f.actor(),
            Some(&board),
            &control_target,
            devagotchi_contracts::ProcessControl::Cancel,
        )
        .await
        .unwrap();
    board
        .release(&f.actor(), &ticket.id, lease.fence, "pause")
        .await
        .unwrap();
    let next = board.claim(&f.actor(), &ticket.id, None).await.unwrap();
    assert!(next.fence > lease.fence);
    assert!(
        f.access
            .operation_control(
                &f.owner.id,
                &f.actor(),
                Some(&board),
                &control_target,
                devagotchi_contracts::ProcessControl::Cancel
            )
            .await
            .is_err()
    );
    assert!(
        f.access
            .renew_admission(
                &f.owner.id,
                &f.actor(),
                Some(&board),
                &machine,
                &admitted.admission_id
            )
            .await
            .is_err()
    );
    let ended = f
        .access
        .reap_grant(&f.owner.tenant, input.grant_id, Some(&board))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ended.reason, "task_ended");
    // Original results remain readable under current permission, with no lease renewal.
    let (links, _rx) = super::broker_tests::links(&f).await;
    let target = f
        .access
        .operation_target(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            "task-operation",
            &links,
        )
        .await
        .unwrap();
    let proof = f
        .access
        .operation_observation(&f.owner.id, &f.actor(), Some(&board), &target)
        .await
        .unwrap();
    assert_eq!(proof.binding.admission_id, admitted.admission_id);
    assert_eq!(proof.binding.grant_version, admitted.grant_version);
    assert!(
        f.access
            .operation_observation(&f.owner.id, &f.actor(), None, &target)
            .await
            .is_err()
    );
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
