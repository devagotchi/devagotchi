use super::*;
use super::{
    admission_completion::Completion,
    admission_tests::call,
    admissions::{AdmissionMachine, Admitted},
    continuation::Continuation,
    request_tests::Fixture,
};
use crate::store;
use devagotchi_contracts::GrantLevel;

impl Fixture {
    async fn bound_admission(&self, grant: &str) -> Admitted {
        let admitted = self
            .admit(grant, "continued-operation", &call())
            .await
            .unwrap();
        self.access
            .bind_operation(
                &self.owner.id,
                &self.actor(),
                &self.machine,
                &admitted.admission_id,
                "operation-123",
            )
            .await
            .unwrap();
        admitted
    }
    async fn renew(&self, admitted: &Admitted) -> BridgeResult<Continuation> {
        self.access
            .renew_admission(
                &self.owner.id,
                &self.actor(),
                None,
                &AdmissionMachine {
                    binding: &self.machine,
                    sandbox_levels: &[GrantLevel::ReadOnly],
                },
                &admitted.admission_id,
            )
            .await
    }
    async fn expire_admission(&self, id: &str) {
        transaction!(&self.access.accounts.store, |c| {
            self.access.accounts.store.scope(c, &self.owner.tenant).await?;
            sqlx::query("UPDATE machine_admissions SET authority_expires_at=admitted_at WHERE tenant_id=$1 AND id=$2")
                .bind(&self.owner.tenant).bind(id).execute(c).await?;
            Ok(())
        }).unwrap();
    }
}

#[tokio::test]
async fn access_continuation_renews_once_without_more_uses_and_binds_only_one_broker_operation() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let admitted = f.bound_admission(&grant).await;
    let first = f.renew(&admitted).await.unwrap();
    let second = f.renew(&admitted).await.unwrap();
    assert_eq!(first.operation_id, "operation-123");
    assert_eq!(first.admitted.admission_id, admitted.admission_id);
    assert_eq!(first.admitted.input_sha256, admitted.input_sha256);
    assert_eq!(first.admitted.effective, admitted.effective);
    assert!(second.admitted.issued_at >= first.admitted.issued_at);
    assert!(second.admitted.expires_at > second.admitted.issued_at);
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    assert_eq!(
        f.admit(&grant, "second-start", &call())
            .await
            .unwrap_err()
            .code,
        "grant_exhausted"
    );
    f.access
        .bind_operation(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admitted.admission_id,
            "operation-123",
        )
        .await
        .unwrap();
    assert_eq!(
        f.access
            .bind_operation(
                &f.owner.id,
                &f.actor(),
                &f.machine,
                &admitted.admission_id,
                "another-operation"
            )
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    let material: String = transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        Ok(sqlx::query_scalar(
            "SELECT effective::text FROM machine_admissions WHERE tenant_id=$1 AND id=$2",
        )
        .bind(&f.owner.tenant)
        .bind(&admitted.admission_id)
        .fetch_one(c)
        .await?)
    })
    .unwrap();
    assert!(material.contains("exec"));
    assert!(!material.contains("git") && !material.contains("status"));
}

#[tokio::test]
async fn access_continuation_refuses_unbound_expired_completed_and_revoked_admissions() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.grant(false, "grant").await;
    let unbound = f.admit(&grant, "unbound", &call()).await.unwrap();
    assert_eq!(
        f.renew(&unbound).await.err().unwrap().code,
        "admission_unbound"
    );
    let admitted = f.bound_admission(&grant).await;
    f.expire_admission(&admitted.admission_id).await;
    assert_eq!(
        f.renew(&admitted).await.err().unwrap().code,
        "authority_expired"
    );
    let finished = f.admit(&grant, "finished", &call()).await.unwrap();
    f.access
        .complete_admission(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &finished.admission_id,
            &Completion::Finished {
                operation_id: "finished-operation".into(),
                exit_code: Some(0),
                error_code: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        f.renew(&finished).await.err().unwrap().code,
        "operation_finished"
    );
    f.access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
    assert_eq!(f.renew(&unbound).await.err().unwrap().code, "grant_ended");
}

#[tokio::test]
async fn access_continuation_refuses_changed_principal_enrollment_or_sandbox() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let admitted = f.bound_admission(&grant).await;
    let mut machine = f.machine.clone();
    machine.generation = "another-generation".into();
    assert_eq!(
        f.access
            .renew_admission(
                &f.owner.id,
                &f.actor(),
                None,
                &AdmissionMachine {
                    binding: &machine,
                    sandbox_levels: &[GrantLevel::ReadOnly]
                },
                &admitted.admission_id
            )
            .await
            .err()
            .unwrap()
            .code,
        "not_granted"
    );
    assert_eq!(
        f.access
            .renew_admission(
                &f.owner.id,
                &f.actor(),
                None,
                &AdmissionMachine {
                    binding: &f.machine,
                    sandbox_levels: &[]
                },
                &admitted.admission_id
            )
            .await
            .err()
            .unwrap()
            .code,
        "sandbox_unavailable"
    );
    let mut actor = f.actor();
    actor.id = &f.owner.id;
    assert_eq!(
        f.access
            .renew_admission(
                &f.owner.id,
                &actor,
                None,
                &AdmissionMachine {
                    binding: &f.machine,
                    sandbox_levels: &[GrantLevel::ReadOnly]
                },
                &admitted.admission_id
            )
            .await
            .err()
            .unwrap()
            .code,
        "not_granted"
    );
    f.access
        .accounts
        .put_agent(
            &f.owner.id,
            &f.owner.id,
            &crate::identity::accounts::agents::AgentEdit {
                id: "worker".into(),
                name: "Worker".into(),
                kind: "codex".into(),
                roles: vec!["worker".into()],
                enabled: false,
                wip_limit: None,
                version: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(f.renew(&admitted).await.err().unwrap().code, "not_granted");
}

#[tokio::test]
async fn access_continuation_broker_refresh_requires_matching_receipt_and_rechecks_revocation() {
    use super::{broker_tests::links, renewal::LinkContinuation};
    use devagotchi_contracts::{
        REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest, RemoteWireResponse,
    };
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let admitted = f.bound_admission(&grant).await;
    let (links, mut rx) = links(&f).await;
    let actor = f.actor();
    let execution = LinkContinuation {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        machine: &f.machine,
        links: &links,
    };
    for answer in ["operation-123", "wrong-operation"] {
        let peer = async {
            let (id, wire) = rx.recv().await.unwrap();
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
            let (id, wire) = rx.recv().await.unwrap();
            let RemoteRequest::Refresh { authority } = wire.remote else {
                panic!("expected renewal")
            };
            assert_eq!(authority.binding.admission_id, admitted.admission_id);
            assert_eq!(authority.binding.principal, f.principal);
            assert_eq!(authority.binding.operation_key, admitted.operation_key);
            assert_eq!(authority.binding.origin, links.grant_origin(&f.machine));
            links.answer_remote_for_test(
                "lab",
                id,
                RemoteWireResponse::Ok(RemoteReply::Refreshed {
                    operation_id: answer.into(),
                }),
            );
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(
                f.access
                    .refresh_execution(&execution, &admitted.admission_id),
                peer
            )
        })
        .await
        .unwrap();
        if answer == "operation-123" {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.unwrap_err().code, "invalid_response");
        }
    }
    let peer = async {
        let (id, _) = rx.recv().await.unwrap();
        f.access
            .revoke(&f.owner.id, &grant, 1, "revoke-during-wait")
            .await
            .unwrap();
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                os: "linux".into(),
                sandbox_levels: vec![GrantLevel::ReadOnly],
            }),
        );
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            f.access
                .refresh_execution(&execution, &admitted.admission_id),
            peer
        )
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, "grant_ended");
    assert!(rx.try_recv().is_err());
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
}
