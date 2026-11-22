use super::*;
use super::{admission_tests::call, broker_tests::links, request_tests::Fixture};
use crate::store;
use devagotchi_agenthub::{
    Actor,
    policy::{Effect, Rule, Subject, catalog as policy},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_operation_control_interactive_grant_allows_input_and_resize_until_paused() {
    use super::{
        admission_action::AdmissionAction,
        admissions::{AdmissionInput, AdmissionMachine},
        decisions::Decision,
    };
    use devagotchi_contracts::{GrantCapability, GrantDuration, GrantLevel, ProcessControl};
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let mut request = super::request_tests::input();
    request.duration = Some(GrantDuration::Once);
    request.selection = serde_json::from_value(serde_json::json!({"kind":"items","items":[{
        "capability":"proc.interactive","sandbox_level":"read-only","selector":{}}]}))
    .unwrap();
    let pending = f.request(&request, "interactive").await.unwrap();
    let grant = f
        .decide(
            &pending.result.request_ids[0],
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            "approve-interactive",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap();
    let mut call = call();
    call.capability = GrantCapability::Process;
    let admitted = f
        .access
        .admit(
            &f.owner.id,
            &f.actor(),
            None,
            &AdmissionMachine {
                binding: &f.machine,
                sandbox_levels: &[GrantLevel::ReadOnly],
            },
            &AdmissionInput {
                action: AdmissionAction::ProcessStart,
                grant_id: &grant,
                operation_key: "interactive",
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
            "interactive-1",
        )
        .await
        .unwrap();
    let (links, _rx) = links(&f).await;
    let target = f
        .access
        .operation_target(&f.owner.id, &f.actor(), &f.machine, "interactive-1", &links)
        .await
        .unwrap();
    for action in [ProcessControl::Input, ProcessControl::Resize] {
        let proof = f
            .access
            .operation_control(&f.owner.id, &f.actor(), None, &target, action)
            .await
            .unwrap();
        assert_eq!(proof.action, action);
        assert_eq!(proof.binding.grant_id, grant);
        assert!(proof.expires_at <= admitted.expires_at);
    }
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    f.access
        .control_machine(
            &f.owner.id,
            &f.machine,
            1,
            ControlChange::Pause,
            "pause-interactive",
        )
        .await
        .unwrap();
    for action in [ProcessControl::Input, ProcessControl::Resize] {
        assert!(
            f.access
                .operation_control(&f.owner.id, &f.actor(), None, &target, action)
                .await
                .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_operation_control_requires_current_action_and_never_renews_or_spends() {
    use devagotchi_contracts::ProcessControl;
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let admitted = f.admit(&grant, "controlled", &call()).await.unwrap();
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "process-control",
        )
        .await
        .unwrap();
    let (links, _rx) = links(&f).await;
    let target = f
        .access
        .operation_target(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            "process-control",
            &links,
        )
        .await
        .unwrap();
    for _ in 0..2 {
        let proof = f
            .access
            .operation_control(
                &f.owner.id,
                &f.actor(),
                None,
                &target,
                ProcessControl::Cancel,
            )
            .await
            .unwrap();
        assert_eq!(proof.binding.admission_id, admitted.admission_id);
        assert!(proof.expires_at <= admitted.expires_at);
    }
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    assert_eq!(
        f.access
            .operation_control(
                &f.owner.id,
                &f.actor(),
                None,
                &target,
                ProcessControl::Input
            )
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        let expires: i64 = sqlx::query_scalar(
            "SELECT authority_expires_at FROM machine_admissions WHERE tenant_id=$1 AND id=$2",
        )
        .bind(&f.owner.tenant)
        .bind(&admitted.admission_id)
        .fetch_one(&mut *c)
        .await?;
        assert_eq!(expires, admitted.expires_at);
        policy::put_rule_in(
            c,
            &f.owner.tenant,
            &Subject {
                id: &f.owner.id,
                human: true,
                kind: "human",
                roles: &["admin".into()],
            },
            &Rule {
                id: "deny-control".into(),
                name: "Deny process cancellation".into(),
                subject: "agent".into(),
                action: "proc.kill".into(),
                selector: Default::default(),
                effect: Effect::Deny,
                priority: 100,
                enabled: true,
                version: 0,
            },
        )
        .await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        f.access
            .operation_control(
                &f.owner.id,
                &f.actor(),
                None,
                &target,
                ProcessControl::Cancel
            )
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_operation_observation_keeps_original_admission_after_pause_and_obeys_current_policy()
 {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let admitted = f.admit(&grant, "observed", &call()).await.unwrap();
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "process-1",
        )
        .await
        .unwrap();
    let (links, _rx) = links(&f).await;
    let target = f
        .access
        .operation_target(&f.owner.id, &f.actor(), &f.machine, "process-1", &links)
        .await
        .unwrap();
    let before = f.admission_counts(&grant).await;
    let proof = f
        .access
        .operation_observation(&f.owner.id, &f.actor(), None, &target)
        .await
        .unwrap();
    assert_eq!(proof.binding.admission_id, admitted.admission_id);
    f.access
        .control_machine(&f.owner.id, &f.machine, 1, ControlChange::Pause, "pause")
        .await
        .unwrap();
    let after = f
        .access
        .operation_observation(&f.owner.id, &f.actor(), None, &target)
        .await
        .unwrap();
    assert_eq!(after.binding, proof.binding);
    assert_eq!(f.admission_counts(&grant).await, before);
    assert!(
        f.access
            .operation_observation(&f.owner.id, &Actor::human(&f.owner.id), None, &target)
            .await
            .is_err()
    );
    assert!(
        f.access
            .operation_target(
                &f.owner.id,
                &Actor::human(&f.owner.id),
                &f.machine,
                "process-1",
                &links
            )
            .await
            .is_err()
    );
    let mut changed = f.machine.clone();
    changed.generation.push('x');
    assert!(
        f.access
            .operation_target(&f.owner.id, &f.actor(), &changed, "process-1", &links)
            .await
            .is_err()
    );
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        policy::put_rule_in(
            c,
            &f.owner.tenant,
            &Subject {
                id: &f.owner.id,
                human: true,
                kind: "human",
                roles: &["admin".into()],
            },
            &Rule {
                id: "deny-observation".into(),
                name: "Hide process results".into(),
                subject: "agent".into(),
                action: "proc.read".into(),
                selector: Default::default(),
                effect: Effect::Deny,
                priority: 100,
                enabled: true,
                version: 0,
            },
        )
        .await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        f.access
            .operation_observation(&f.owner.id, &f.actor(), None, &target)
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    assert_eq!(f.admission_counts(&grant).await, before);
}
