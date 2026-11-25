use super::request_tests::Fixture;
use super::*;
use crate::store;
use sqlx::Connection;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_grant_revocation_is_versioned_replayable_and_has_one_notification() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.grant(false, "grant").await;
    assert_eq!(
        f.access
            .revoke(&f.owner.id, &grant, 2, "stale")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    let (a, b) = tokio::join!(
        f.access.revoke(&f.owner.id, &grant, 1, "revoke"),
        f.access.revoke(&f.owner.id, &grant, 1, "revoke")
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.result, b.result);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.result.generation, f.machine.generation);
    assert_eq!(a.result.version, 2);
    assert_eq!(f.lifecycle_counts().await, (0, 1, 1));
    assert_eq!(
        f.access
            .revoke(&f.owner.id, &grant, 2, "again")
            .await
            .unwrap_err()
            .code,
        "not_live"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_grant_notification_failure_rolls_back_ending_and_receipt() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.grant(false, "grant").await;
    // Only fixture installation uses the test database's migration connection. The
    // mutation being tested still runs as the actual restricted hub role.
    let mut admin = sqlx::AnyConnection::connect(&test.url).await.unwrap();
    sqlx::query("CREATE FUNCTION devagotchi.reject_access_notification() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'outbox unavailable'; END $$")
        .execute(&mut admin).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_access_notification BEFORE INSERT ON devagotchi.machine_access_notifications FOR EACH ROW EXECUTE FUNCTION devagotchi.reject_access_notification()")
        .execute(&mut admin).await.unwrap();
    assert!(
        f.access
            .revoke(&f.owner.id, &grant, 1, "revoke")
            .await
            .is_err()
    );
    assert_eq!(f.lifecycle_counts().await, (1, 0, 0));
    sqlx::query(
        "DROP TRIGGER reject_access_notification ON devagotchi.machine_access_notifications",
    )
    .execute(&mut admin)
    .await
    .unwrap();
    let ended = f
        .access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
    assert!(!ended.replayed);
    assert_eq!(f.lifecycle_counts().await, (0, 1, 1));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_grant_machine_revocation_is_scoped_to_the_original_generation() {
    let test = or_skip!(store::testing::postgres());
    let mut f = Fixture::new(test.store).await;
    f.grant(false, "old-a").await;
    f.grant(false, "old-b").await;
    let old = f.machine.clone();
    let code = f
        .devices
        .issue_owned_code(&f.owner.id, "lab", [105; 16], 16)
        .await
        .unwrap();
    f.devices
        .enroll("lab", &code, "same-key", false)
        .await
        .unwrap();
    f.machine = transaction!(&f.access.accounts.store, |c| {
        f.devices.bind_access_in(c, "lab", Some(&f.owner.id)).await
    })
    .unwrap();
    f.grant(false, "new").await;
    let ended = f
        .access
        .revoke_machine(&f.owner.id, &f.owner.id, &old, "old-machine")
        .await
        .unwrap();
    assert_eq!(ended.result.count, 2);
    assert_eq!(f.lifecycle_counts().await, (1, 2, 2));
    assert!(
        f.access
            .revoke_machine(&f.owner.id, &f.owner.id, &old, "old-machine")
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(f.lifecycle_counts().await, (1, 2, 2));
}
