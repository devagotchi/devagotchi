use super::admission_action::AdmissionAction;
use super::*;
use super::{
    admission_completion::Completion,
    admissions::{AdmissionInput, AdmissionMachine, Admitted},
    decisions::Decision,
    request_tests::{Fixture, input},
};
use crate::store;
use devagotchi_agenthub::policy::{Effect, Rule, Subject, catalog as policy};
use devagotchi_contracts::{GrantCall, GrantCapability, GrantDuration, GrantLevel};
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(super) fn call() -> GrantCall {
    let mut call = GrantCall::new(GrantCapability::Exec, GrantLevel::ReadOnly);
    call.argv = vec!["git".into(), "status".into()];
    call.paths = vec!["/work".into()];
    call
}
impl Fixture {
    pub(super) async fn once(&self) -> String {
        let mut request = input();
        request.duration = Some(GrantDuration::Once);
        let id = self
            .request(&request, "once")
            .await
            .unwrap()
            .result
            .request_ids[0]
            .clone();
        self.decide(
            &id,
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            "approve-once",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap()
    }
    pub(super) async fn admit(
        &self,
        grant: &str,
        key: &str,
        call: &GrantCall,
    ) -> BridgeResult<Admitted> {
        self.access
            .admit(
                &self.owner.id,
                &self.actor(),
                None,
                &AdmissionMachine {
                    binding: &self.machine,
                    sandbox_levels: &[GrantLevel::ReadOnly],
                },
                &AdmissionInput {
                    action: AdmissionAction::Exec,
                    grant_id: grant,
                    operation_key: key,
                    input_sha256: HASH,
                    call,
                },
            )
            .await
    }
    pub(super) async fn admission_counts(&self, grant: &str) -> (i64, i64, i64) {
        transaction!(&self.access.accounts.store, |c| {
            self.access
                .accounts
                .store
                .scope(c, &self.owner.tenant)
                .await?;
            let uses: i64 =
                sqlx::query_scalar("SELECT uses FROM machine_grants WHERE tenant_id=$1 AND id=$2")
                    .bind(&self.owner.tenant)
                    .bind(grant)
                    .fetch_one(&mut *c)
                    .await?;
            let admissions: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM machine_admissions WHERE tenant_id=$1 AND grant_id=$2",
            )
            .bind(&self.owner.tenant)
            .bind(grant)
            .fetch_one(&mut *c)
            .await?;
            let events: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event='grant.admitted'",
            )
            .bind(&self.owner.tenant)
            .fetch_one(c)
            .await?;
            Ok((uses, admissions, events))
        })
        .unwrap()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admission_once_retry_and_continuation_consume_one_use_but_new_start_is_refused() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let call = call();
    let (a, b) = tokio::join!(
        f.admit(&grant, "operation", &call),
        f.admit(&grant, "operation", &call)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.admission_id, b.admission_id);
    assert_ne!(a.replayed, b.replayed);
    assert!(a.expires_at > a.issued_at && a.expires_at - a.issued_at <= 10_000);
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    assert!(f.admit(&grant, "operation", &call).await.unwrap().replayed);
    assert_eq!(
        f.admit(&grant, "new-operation", &call)
            .await
            .unwrap_err()
            .code,
        "grant_exhausted"
    );
    let mut changed = call.clone();
    changed.argv = vec!["git".into(), "diff".into()];
    assert_eq!(
        f.admit(&grant, "operation", &changed)
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    f.access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
    assert_eq!(
        f.admit(&grant, "operation", &call).await.unwrap_err().code,
        "grant_ended"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admission_concurrent_last_use_has_one_winner() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let call = call();
    let (a, b) = tokio::join!(
        f.admit(&grant, "first", &call),
        f.admit(&grant, "second", &call)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(a.err().or_else(|| b.err()).unwrap().code, "grant_exhausted");
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admission_refuses_unproved_sandbox_changed_digest_and_changed_owner() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let call = call();
    let input = AdmissionInput {
        action: AdmissionAction::Exec,
        grant_id: &grant,
        operation_key: "operation",
        input_sha256: HASH,
        call: &call,
    };
    assert_eq!(
        f.access
            .admit(
                &f.owner.id,
                &f.actor(),
                None,
                &AdmissionMachine {
                    binding: &f.machine,
                    sandbox_levels: &[]
                },
                &input
            )
            .await
            .unwrap_err()
            .code,
        "sandbox_unavailable"
    );
    assert_eq!(f.admission_counts(&grant).await, (0, 0, 0));
    f.admit(&grant, "operation", &call).await.unwrap();
    let changed = AdmissionInput {
        input_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ..input
    };
    assert_eq!(
        f.access
            .admit(
                &f.owner.id,
                &f.actor(),
                None,
                &AdmissionMachine {
                    binding: &f.machine,
                    sandbox_levels: &[GrantLevel::ReadOnly]
                },
                &changed
            )
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    let mut forged = f.actor();
    forged.id = &f.owner.id;
    assert_eq!(
        f.access
            .admit(
                &f.owner.id,
                &forged,
                None,
                &AdmissionMachine {
                    binding: &f.machine,
                    sandbox_levels: &[GrantLevel::ReadOnly]
                },
                &changed
            )
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admission_completion_and_retirement_preserve_permanent_identity() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.grant(false, "grant").await;
    let call = call();
    let admission = f.admit(&grant, "operation", &call).await.unwrap();
    let finished = Completion::Finished {
        operation_id: "operation-123".into(),
        exit_code: Some(0),
        error_code: None,
    };
    assert!(
        f.access
            .complete_admission(
                &f.owner.id,
                &f.actor(),
                &f.machine,
                &admission.admission_id,
                &finished
            )
            .await
            .unwrap()
    );
    assert!(
        !f.access
            .complete_admission(
                &f.owner.id,
                &f.actor(),
                &f.machine,
                &admission.admission_id,
                &finished
            )
            .await
            .unwrap()
    );
    assert_eq!(
        f.admit(&grant, "operation", &call).await.unwrap_err().code,
        "operation_finished"
    );
    let retired = Completion::Retired {
        operation_id: "operation-123".into(),
    };
    assert!(
        f.access
            .complete_admission(
                &f.owner.id,
                &f.actor(),
                &f.machine,
                &admission.admission_id,
                &retired
            )
            .await
            .unwrap()
    );
    assert_eq!(
        f.admit(&grant, "operation", &call).await.unwrap_err().code,
        "request_retired"
    );
    assert_eq!(
        f.access
            .complete_admission(
                &f.owner.id,
                &f.actor(),
                &f.machine,
                &admission.admission_id,
                &finished
            )
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admission_exact_operation_policy_is_rechecked_before_continuation() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let mut request = input();
    request.selection = serde_json::from_value(serde_json::json!({"kind":"items","items":[
        {"capability":"proc.interactive","sandbox_level":"read-only","selector":{}}]}))
    .unwrap();
    let id = f
        .request(&request, "process")
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
            "approve-process",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap();
    let mut call = call();
    call.capability = GrantCapability::Process;
    let input = AdmissionInput {
        action: AdmissionAction::ProcessStart,
        grant_id: &grant,
        operation_key: "process",
        input_sha256: HASH,
        call: &call,
    };
    let machine = AdmissionMachine {
        binding: &f.machine,
        sandbox_levels: &[GrantLevel::ReadOnly],
    };
    let admitted = f
        .access
        .admit(&f.owner.id, &f.actor(), None, &machine, &input)
        .await
        .unwrap();
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "process-123",
        )
        .await
        .unwrap();
    f.access
        .renew_admission(
            &f.owner.id,
            &f.actor(),
            None,
            &machine,
            &admitted.admission_id,
        )
        .await
        .unwrap();
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
                id: "deny-process-start".into(),
                name: "Stop process admission".into(),
                subject: "agent".into(),
                action: "proc.start".into(),
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
            .admit(&f.owner.id, &f.actor(), None, &machine, &input)
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    assert_eq!(
        f.access
            .renew_admission(
                &f.owner.id,
                &f.actor(),
                None,
                &machine,
                &admitted.admission_id
            )
            .await
            .err()
            .unwrap()
            .code,
        "policy_denied"
    );
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}
