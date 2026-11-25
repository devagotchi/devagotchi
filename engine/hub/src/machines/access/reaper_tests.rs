use super::*;
use super::{
    admission_completion::Completion,
    admission_tests::call,
    request_tests::{Fixture, input},
};
use crate::{identity::accounts::agents::AgentEdit, store};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_reaper_keeps_once_operation_until_broker_completion_then_ends_once() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let call = call();
    let admission = f.admit(&grant, "once-operation", &call).await.unwrap();
    assert!(
        f.access
            .reap_grant(&f.owner.tenant, &grant, None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        f.admit(&grant, "once-operation", &call)
            .await
            .unwrap()
            .replayed
    );
    f.access
        .complete_admission(
            &f.owner.id,
            &f.actor(),
            &f.machine,
            &admission.admission_id,
            &Completion::Finished {
                operation_id: "op-1".into(),
                exit_code: Some(0),
                error_code: None,
            },
        )
        .await
        .unwrap();
    let ended = f
        .access
        .reap_grant(&f.owner.tenant, &grant, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ended.reason, "used_up");
    assert_eq!(f.lifecycle_counts().await, (0, 1, 1));
    assert!(
        f.access
            .reap_grant(&f.owner.tenant, &grant, None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(f.lifecycle_counts().await, (0, 1, 1));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_reaper_ends_expired_and_revoked_machine_grants_with_durable_notifications() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let expired = f.grant(false, "expires").await;
    let revoked = f.grant(false, "revoked-machine").await;
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET expires_at=1 WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant)
            .bind(&expired)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        f.access
            .reap_grant(&f.owner.tenant, &expired, None)
            .await
            .unwrap()
            .unwrap()
            .reason,
        "expired"
    );
    f.devices.revoke_owned(&f.owner.id, "lab").await.unwrap();
    assert_eq!(
        f.access
            .reap_grant(&f.owner.tenant, &revoked, None)
            .await
            .unwrap()
            .unwrap()
            .reason,
        "machine_revoked"
    );
    assert_eq!(f.lifecycle_counts().await, (0, 2, 2));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_reaper_ends_disabled_principal_and_pages_only_live_candidates() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let first = f.grant(false, "first").await;
    let second = f.grant(false, "second").await;
    let page = f
        .access
        .reap_candidates(&f.owner.tenant, None, 1)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].account, f.owner.id);
    assert!(page[0].project.is_none());
    let next = f
        .access
        .reap_candidates(&f.owner.tenant, Some(&page[0].id), 1)
        .await
        .unwrap();
    assert_eq!(next.len(), 1);
    assert_ne!(page[0].id, next[0].id);
    f.access
        .accounts
        .put_agent(
            &f.owner.id,
            &f.owner.id,
            &AgentEdit {
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
    for id in [&first, &second] {
        assert_eq!(
            f.access
                .reap_grant(&f.owner.tenant, id, None)
                .await
                .unwrap()
                .unwrap()
                .reason,
            "principal_off"
        );
    }
    assert!(
        f.access
            .reap_candidates(&f.owner.tenant, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(f.lifecycle_counts().await, (0, 2, 2));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_reaper_request_expiry_is_bounded_versioned_and_audited_once() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    for key in ["one", "two"] {
        f.request(&input(), key).await.unwrap();
    }
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query(
            "UPDATE machine_access_requests SET created_at=1,expires_at=2 WHERE tenant_id=$1",
        )
        .bind(&f.owner.tenant)
        .execute(c)
        .await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        f.access.expire_requests(&f.owner.tenant, 1).await.unwrap(),
        1
    );
    assert_eq!(
        f.access.expire_requests(&f.owner.tenant, 1).await.unwrap(),
        1
    );
    assert_eq!(
        f.access.expire_requests(&f.owner.tenant, 1).await.unwrap(),
        0
    );
    transaction!(&f.access.accounts.store,|c| {
        f.access.accounts.store.scope(c,&f.owner.tenant).await?;
        let changed:i64=sqlx::query_scalar("SELECT count(*) FROM machine_access_requests WHERE tenant_id=$1 AND status='expired' AND version=2")
            .bind(&f.owner.tenant).fetch_one(&mut *c).await?;
        let audits:i64=sqlx::query_scalar("SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event='access.expired'")
            .bind(&f.owner.tenant).fetch_one(c).await?;
        assert_eq!((changed,audits),(2,2));Ok(())
    }).unwrap();
}
