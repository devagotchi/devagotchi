use super::*;
use super::{
    admission_action::AdmissionAction,
    admission_tests::call,
    admissions::{AdmissionInput, AdmissionMachine},
    control::ControlChange,
    decisions::Decision,
    request_tests::{Fixture, input},
};
use crate::store;
use devagotchi_contracts::{GrantCall, GrantCapability, GrantLevel};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_control_pause_retires_old_operations_and_resume_permits_only_new_admissions() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.grant(false, "grant").await;
    let call = call();
    f.admit(&grant, "old-operation", &call).await.unwrap();
    let paused = f
        .access
        .control_machine(&f.owner.id, &f.machine, 1, ControlChange::Pause, "pause")
        .await
        .unwrap();
    assert!(paused.result.control.paused);
    assert_eq!(paused.result.control.version, 2);
    assert_eq!(paused.result.interrupted_grants, 1);
    let again = f
        .access
        .control_machine(&f.owner.id, &f.machine, 1, ControlChange::Pause, "pause")
        .await
        .unwrap();
    assert!(again.replayed);
    assert_eq!(again.result, paused.result);
    assert_eq!(
        f.admit(&grant, "old-operation", &call)
            .await
            .unwrap_err()
            .code,
        "machine_paused"
    );
    assert_eq!(
        f.admit(&grant, "new-operation", &call)
            .await
            .unwrap_err()
            .code,
        "machine_paused"
    );
    assert_eq!(
        f.access
            .control_machine(&f.owner.id, &f.machine, 1, ControlChange::Resume, "stale")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    let resumed = f
        .access
        .control_machine(&f.owner.id, &f.machine, 2, ControlChange::Resume, "resume")
        .await
        .unwrap();
    assert!(!resumed.result.control.paused);
    assert_eq!(resumed.result.interrupted_grants, 0);
    assert!(f.admit(&grant, "old-operation", &call).await.is_err());
    let fresh = f.admit(&grant, "new-operation", &call).await.unwrap();
    assert!(!fresh.replayed);
    assert_eq!(fresh.grant_version, 2);
    assert_eq!(f.lifecycle_counts().await, (1, 0, 1));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_control_gui_stop_interrupts_only_gui_and_resume_clears_the_stop() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let exec = f.grant(false, "exec").await;
    let exec_call = call();
    f.admit(&exec, "exec-operation", &exec_call).await.unwrap();
    let mut request = input();
    request.selection = serde_json::from_value(json!({"kind":"items","items":[
        {"capability":"gui.observe","sandbox_level":"desktop","selector":{}}]}))
    .unwrap();
    let id = f
        .request(&request, "observe")
        .await
        .unwrap()
        .result
        .request_ids[0]
        .clone();
    let grant = f
        .decide(
            &id,
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            "approve-observe",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap();
    let mut call = GrantCall::new(GrantCapability::Observe, GrantLevel::Desktop);
    call.monitors = vec!["screen-1".into()];
    let machine = AdmissionMachine {
        binding: &f.machine,
        sandbox_levels: &[GrantLevel::Desktop],
    };
    let input = AdmissionInput {
        action: AdmissionAction::Observe,
        grant_id: &grant,
        operation_key: "gui-operation",
        input_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        call: &call,
    };
    f.access
        .admit(&f.owner.id, &f.actor(), None, &machine, &input)
        .await
        .unwrap();
    let stopped = f
        .access
        .control_machine(&f.owner.id, &f.machine, 1, ControlChange::StopGui, "stop")
        .await
        .unwrap();
    assert_eq!(stopped.result.interrupted_grants, 1);
    assert!(stopped.result.control.gui_paused);
    assert!(!stopped.result.control.paused);
    assert_eq!(
        f.access
            .admit(&f.owner.id, &f.actor(), None, &machine, &input)
            .await
            .unwrap_err()
            .code,
        "gui_stopped_by_operator"
    );
    assert!(
        f.admit(&exec, "exec-operation", &exec_call)
            .await
            .unwrap()
            .replayed
    );
    let resumed = f
        .access
        .control_machine(&f.owner.id, &f.machine, 2, ControlChange::Resume, "resume")
        .await
        .unwrap();
    assert!(!resumed.result.control.gui_paused);
    let fresh = AdmissionInput {
        operation_key: "new-gui-operation",
        ..input
    };
    f.access
        .admit(&f.owner.id, &f.actor(), None, &machine, &fresh)
        .await
        .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_control_requires_owner_and_never_replays_against_new_enrollment() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let other = f
        .access
        .accounts
        .create_local_user(
            &f.owner.id,
            "Other",
            "other@example.test",
            "a good password",
        )
        .await
        .unwrap()
        .account;
    assert_eq!(
        f.access
            .control_machine(&other.id, &f.machine, 1, ControlChange::Pause, "other")
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    f.access
        .control_machine(&f.owner.id, &f.machine, 1, ControlChange::Pause, "pause")
        .await
        .unwrap();
    let code = f
        .devices
        .issue_owned_code(&f.owner.id, "lab", [106; 16], 16)
        .await
        .unwrap();
    f.devices
        .enroll("lab", &code, "same-key", false)
        .await
        .unwrap();
    assert_eq!(
        f.access
            .control_machine(&f.owner.id, &f.machine, 1, ControlChange::Pause, "pause")
            .await
            .unwrap_err()
            .code,
        "machine_reenrolled"
    );
    transaction!(&f.access.accounts.store, |c| {
        let binding = f
            .devices
            .bind_access_in(c, "lab", Some(&f.owner.id))
            .await?;
        let state = f.devices.lock_control_in(c, &binding).await?;
        assert_eq!(state.version, 1);
        assert!(!state.paused);
        assert!(!state.gui_paused);
        Ok(())
    })
    .unwrap();
}
