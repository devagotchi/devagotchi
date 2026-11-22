use super::*;
use super::{admission_tests::call, broker_tests::links, request_tests::Fixture};
use crate::store;
use devagotchi_agenthub::{
    Actor,
    policy::{Effect, Rule, Subject, catalog as policy},
};

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
        .operation_read_target(&f.owner.id, &f.actor(), &f.machine, "process-1", &links)
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
            .operation_read_target(
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
            .operation_read_target(&f.owner.id, &f.actor(), &changed, "process-1", &links)
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
