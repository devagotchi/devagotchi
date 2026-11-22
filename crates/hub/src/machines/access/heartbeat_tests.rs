use super::*;
use super::{
    admission_tests::call,
    admissions::AdmissionInput,
    broker_tests::links,
    request_tests::{Fixture, input},
};
use crate::{catalog::access::MachineFacts, identity::accounts::agents::Directory, store};
use devagotchi_agenthub::{Actor, NewTicket, hosted::HostedBoard};
use devagotchi_contracts::{GrantAction, GrantCapability, GrantDuration, GrantLevel};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_heartbeat_keeps_original_task_and_refuses_foreign_holder_and_reclaimed_lease() {
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
                title: "Run a task process".into(),
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
    request.selection = serde_json::from_value(serde_json::json!({"kind":"items","items":[{
        "capability":"proc.interactive","sandbox_level":"read-only","selector":{}}]}))
    .unwrap();
    let pending = f
        .access
        .request(
            &f.owner.id,
            &f.actor(),
            Some(&board),
            &f.machine,
            &MachineFacts {
                os: "linux",
                display_server: "x11",
                kind: "native",
            },
            &request,
            "task-process",
        )
        .await
        .unwrap();
    let grant = f
        .access
        .decide(
            &f.owner.id,
            Some(&board),
            &pending.result.request_ids[0],
            1,
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            "task-process-approval",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap();
    let machine = AdmissionMachine {
        binding: &f.machine,
        sandbox_levels: &[GrantLevel::ReadOnly],
    };
    let mut call = call();
    call.capability = GrantCapability::Process;
    let admitted = f
        .access
        .admit(
            &f.owner.id,
            &f.actor(),
            Some(&board),
            &machine,
            &AdmissionInput {
                action: GrantAction::ProcessStart,
                grant_id: &grant,
                operation_key: "task-process",
                input_sha256: &"a".repeat(64),
                call: &call,
            },
        )
        .await
        .unwrap();
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "task-process",
        )
        .await
        .unwrap();
    let (links, _rx) = links(&f).await;
    let target = f
        .access
        .operation_target(&f.owner.id, &f.actor(), &f.machine, "task-process", &links)
        .await
        .unwrap();
    let renewed = f
        .access
        .heartbeat_operation(&f.owner.id, &f.actor(), Some(&board), &machine, &target)
        .await
        .unwrap();
    assert_eq!(
        renewed.admitted.task.as_ref().unwrap().lease_id,
        lease.lease_id
    );
    assert_eq!(
        renewed.admitted.task.as_ref().unwrap().expires_at,
        lease.lease_expires_at
    );
    assert!(renewed.admitted.expires_at <= lease.lease_expires_at);
    assert_eq!(
        f.access
            .heartbeat_operation(
                &f.owner.id,
                &Actor::human(&f.owner.id),
                Some(&board),
                &machine,
                &target
            )
            .await
            .err()
            .unwrap()
            .code,
        "not_granted"
    );
    board
        .release(&f.actor(), &ticket.id, lease.fence, "pause task")
        .await
        .unwrap();
    let next = board.claim(&f.actor(), &ticket.id, None).await.unwrap();
    assert!(next.fence > lease.fence);
    assert_eq!(
        f.access
            .heartbeat_operation(&f.owner.id, &f.actor(), Some(&board), &machine, &target)
            .await
            .err()
            .unwrap()
            .code,
        "stale_fence"
    );
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}

#[tokio::test]
async fn access_heartbeat_refuses_noninteractive_admission() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let admitted = f.admit(&grant, "exec-only", &call()).await.unwrap();
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "exec-only",
        )
        .await
        .unwrap();
    let (links, _rx) = links(&f).await;
    let target = f
        .access
        .operation_target(&f.owner.id, &f.actor(), &f.machine, "exec-only", &links)
        .await
        .unwrap();
    assert_eq!(
        f.access
            .heartbeat_operation(
                &f.owner.id,
                &f.actor(),
                None,
                &AdmissionMachine {
                    binding: &f.machine,
                    sandbox_levels: &[GrantLevel::ReadOnly]
                },
                &target
            )
            .await
            .err()
            .unwrap()
            .code,
        "not_granted"
    );
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}
