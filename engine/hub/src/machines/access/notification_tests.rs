use super::*;
use super::{notifications::Notification, request_tests::Fixture};
use crate::store;

async fn due(f: &Fixture) {
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE machine_access_notifications SET next_attempt_at=0 WHERE tenant_id=$1")
            .bind(&f.owner.tenant)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
}
async fn notice(f: &Fixture) -> Notification {
    let grant = f.grant(false, "grant").await;
    f.access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        f.access.take_notifications(&f.owner.tenant, 100),
        f.access.take_notifications(&f.owner.tenant, 100)
    );
    let mut all = a.unwrap();
    all.extend(b.unwrap());
    assert_eq!(all.len(), 1);
    all.remove(0)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_notification_failure_retries_and_late_ack_cannot_overwrite_newer_attempt() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let first = notice(&f).await;
    assert_eq!(first.attempt, 1);
    assert!(
        f.access
            .finish_notification(&f.owner.tenant, &first, Some("broker_offline"))
            .await
            .unwrap()
    );
    assert!(
        f.access
            .take_notifications(&f.owner.tenant, 100)
            .await
            .unwrap()
            .is_empty()
    );
    due(&f).await;
    let second = f
        .access
        .take_notifications(&f.owner.tenant, 100)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(second.attempt, 2);
    assert!(
        !f.access
            .finish_notification(&f.owner.tenant, &first, None)
            .await
            .unwrap()
    );
    let mut wrong = second.clone();
    wrong.generation.push_str("-another");
    assert!(
        !f.access
            .finish_notification(&f.owner.tenant, &wrong, None)
            .await
            .unwrap()
    );
    assert!(
        f.access
            .finish_notification(&f.owner.tenant, &second, None)
            .await
            .unwrap()
    );
    assert!(
        !f.access
            .finish_notification(&f.owner.tenant, &second, Some("late_failure"))
            .await
            .unwrap()
    );
    due(&f).await;
    assert!(
        f.access
            .take_notifications(&f.owner.tenant, 100)
            .await
            .unwrap()
            .is_empty()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_notification_crashed_dispatch_claim_expires_and_does_not_require_active_owner() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let first = notice(&f).await;
    assert!(
        f.access
            .take_notifications(&f.owner.tenant, 100)
            .await
            .unwrap()
            .is_empty()
    );
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE auth_accounts SET disabled_at=1 WHERE id=$1")
            .bind(&f.owner.id)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
    due(&f).await;
    let second = f
        .access
        .take_notifications(&f.owner.tenant, 1)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(second.grant_id, first.grant_id);
    assert_eq!(second.attempt, 2);
    assert!(
        f.access
            .finish_notification(&f.owner.tenant, &second, Some("https://secret@broker"))
            .await
            .is_err()
    );
    assert!(
        f.access
            .finish_notification(&f.owner.tenant, &second, None)
            .await
            .unwrap()
    );
}
