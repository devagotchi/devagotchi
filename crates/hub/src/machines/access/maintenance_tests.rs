use super::*;
use super::{admission_tests::call, broker_tests::links, request_tests::Fixture};
use crate::{store, tenancy::tests::tenants::hosted_with};
use devagotchi_contracts::{
    GrantLevel, OperationState, REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest,
    RemoteWireResponse,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_maintenance_recovers_start_receipt_renews_once_and_records_actual_completion() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let hosted = or_skip!(hosted_with(None, |_, _| serde_json::json!({})));
    let grant = f.once().await;
    let admitted = f.admit(&grant, "operation", &call()).await.unwrap();
    let (links, mut rx) = links(&f).await;
    let links = Arc::new(links);
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE machine_admissions SET authority_expires_at=admitted_at+4000 WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant).bind(&admitted.admission_id).execute(c).await?;
        Ok(())
    }).unwrap();
    let peer = async {
        let (id, wire) = rx.recv().await.unwrap();
        let RemoteRequest::Status { binding } = wire.remote else {
            panic!("expected status")
        };
        assert_eq!(binding.admission_id, admitted.admission_id);
        assert_eq!(binding.principal, f.principal);
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Status {
                operation_id: "operation-123".into(),
                state: OperationState::Running,
                exit_code: None,
            }),
        );
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
        assert_eq!(authority.binding, binding);
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Refreshed {
                operation_id: "operation-123".into(),
            }),
        );
    };
    let runtime = super::maintenance::start(
        f.access.accounts.clone(),
        links.clone(),
        hosted.hub.boards.clone(),
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), peer)
        .await
        .unwrap();
    runtime.abort();
    assert_eq!(f.admission_counts(&grant).await, (1, 1, 1));
    // Termination evidence remains recordable after authority ends. No refresh may
    // be sent in response to a terminal broker record.
    f.access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
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
    for (operation, exit_code) in [("wrong-operation", Some(0)), ("operation-123", None)] {
        let peer = async {
            let (id, wire) = rx.recv().await.unwrap();
            assert!(matches!(wire.remote, RemoteRequest::Status { .. }));
            links.answer_remote_for_test(
                "lab",
                id,
                RemoteWireResponse::Ok(RemoteReply::Status {
                    operation_id: operation.into(),
                    state: OperationState::Succeeded,
                    exit_code,
                }),
            );
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(
                f.access.maintain_admission(
                    &f.owner.tenant,
                    &admitted.admission_id,
                    &links,
                    &hosted.hub.boards
                ),
                peer
            )
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().code, "invalid_response");
        assert!(rx.try_recv().is_err());
    }
    for _ in 0..2 {
        let peer = async {
            let (id, wire) = rx.recv().await.unwrap();
            assert!(matches!(wire.remote, RemoteRequest::Status { .. }));
            links.answer_remote_for_test(
                "lab",
                id,
                RemoteWireResponse::Ok(RemoteReply::Status {
                    operation_id: "operation-123".into(),
                    state: OperationState::Cancelled,
                    exit_code: None,
                }),
            );
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(
                f.access.maintain_admission(
                    &f.owner.tenant,
                    &admitted.admission_id,
                    &links,
                    &hosted.hub.boards
                ),
                peer
            )
        })
        .await
        .unwrap();
        result.unwrap();
        assert!(rx.try_recv().is_err());
    }
    let state: (String, i64) = transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        let state: String =
            sqlx::query_scalar("SELECT state FROM machine_admissions WHERE tenant_id=$1 AND id=$2")
                .bind(&f.owner.tenant)
                .bind(&admitted.admission_id)
                .fetch_one(&mut *c)
                .await?;
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event='grant.used'",
        )
        .bind(&f.owner.tenant)
        .fetch_one(c)
        .await?;
        Ok((state, events))
    })
    .unwrap();
    assert_eq!(state, ("finished".into(), 1));
}
