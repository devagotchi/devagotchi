use super::*;
use super::{
    broker_tests::links,
    request_tests::{Fixture, input},
};
use crate::{store, tenancy::tests::tenants::hosted_with};
use devagotchi_contracts::{RemoteReply, RemoteRequest, RemoteWireResponse};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_access_runtime_expires_requests_reaps_grants_and_delivers_the_committed_interruptions()
 {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    // A real board registry is used even though this particular grant has no task.
    let hosted = or_skip!(hosted_with(None, |_, _| serde_json::json!({})));
    let grant = f.grant(false, "expired-grant").await;
    let request = f
        .request(&input(), "expired-request")
        .await
        .unwrap()
        .result
        .request_ids[0]
        .clone();
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET expires_at=1 WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant)
            .bind(&grant)
            .execute(&mut *c)
            .await?;
        sqlx::query("UPDATE machine_access_requests SET created_at=0,expires_at=1 WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant)
            .bind(&request)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
    let (links, mut rx) = links(&f).await;
    let broker = async {
        let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(wire.remote,RemoteRequest::Interrupt {notice} if notice.grant_id==grant));
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Interrupted { count: 0 }),
        );
    };
    let (result, ()) = tokio::join!(f.access.maintain_access(&links, &hosted.hub.boards), broker);
    result.unwrap();
    let lookup = f
        .access
        .access_lookup(&f.owner.id, &f.actor(), &request)
        .await
        .unwrap();
    assert_eq!(lookup.status, "expired");
    assert_eq!(f.lifecycle_counts().await, (0, 1, 1));
    assert_eq!(
        f.access
            .deliver_interruptions(&links, &f.owner.tenant, 100)
            .await
            .unwrap(),
        0
    );
    f.access
        .maintain_access(&links, &hosted.hub.boards)
        .await
        .unwrap();
    assert!(rx.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_access_failed_delivery_stays_pending_with_a_bounded_diagnostic() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    f.access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
    let (links, mut rx) = links(&f).await;
    let broker = async {
        let (id, _) = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Error(BridgeError::new("ipc_io", "private transport detail")),
        );
    };
    let (result, ()) = tokio::join!(
        f.access.deliver_interruptions(&links, &f.owner.tenant, 100),
        broker
    );
    assert_eq!(result.unwrap(), 0);
    transaction!(&f.access.accounts.store,|c| {
        f.access.accounts.store.scope(c,&f.owner.tenant).await?;
        let (delivered,diagnostic):(Option<i64>,Option<String>)=sqlx::query_as("SELECT delivered_at,last_error FROM machine_access_notifications WHERE tenant_id=$1 AND grant_id=$2")
            .bind(&f.owner.tenant).bind(&grant).fetch_one(c).await?;
        assert_eq!(delivered,None);assert_eq!(diagnostic.as_deref(),Some("delivery_failed"));Ok(())
    }).unwrap();
}
