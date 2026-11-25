use super::*;
use crate::{catalog::Kind, store};
use devagotchi_contracts::GrantDuration;
use serde_json::json;

#[test]
fn access_settings_refuse_unbounded_refresh_empty_risks_and_ambiguous_durations() {
    let good = AccessSettings::default();
    good.validate().unwrap();
    let mut changed = good.clone();
    changed.authority_ttl_ms = 30_001;
    assert!(changed.validate().is_err());
    changed = good.clone();
    changed.request_expiry_minutes = 0;
    assert!(changed.validate().is_err());
    changed = good.clone();
    changed.unsandboxed_risk_ids.clear();
    assert!(changed.validate().is_err());
    changed = good.clone();
    changed.grant_duration_options.push(GrantDuration::Once);
    assert!(changed.validate().is_err());
    changed = good;
    changed.grant_duration_options = vec![GrantDuration::Hours(0)];
    assert!(changed.validate().is_err());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_settings_changes_are_versioned_replayable_and_preserve_risk_references() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(Accounts::open(test.store).await.unwrap());
    let owner = accounts
        .ensure_owner(None, "Owner", [96; 16])
        .await
        .unwrap();
    let access = Access::new(accounts);
    let mut edit = access.settings(&owner.id).await.unwrap();
    edit.request_expiry_minutes = 45;
    let saved = access
        .put_settings(&owner.id, &edit, "settings-1")
        .await
        .unwrap();
    assert_eq!(saved.result.version, 2);
    assert!(!saved.replayed);
    let again = access
        .put_settings(&owner.id, &edit, "settings-1")
        .await
        .unwrap();
    assert_eq!(again.result, saved.result);
    assert!(again.replayed);
    assert_eq!(
        access
            .put_settings(&owner.id, &edit, "settings-2")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    edit.request_expiry_minutes = 46;
    assert_eq!(
        access
            .put_settings(&owner.id, &edit, "settings-1")
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    // Removing a required risk is refused by the access owner even without a preset reference.
    access
        .catalog
        .put(
            &owner.id,
            Kind::Risks,
            "access-test-risk",
            0,
            &json!({"title":"A risk","plain_text":"Review this risk","severity":"high"}),
            crate::catalog::WriteOptions {
                request_id: None,
                confirm_default: None,
            },
        )
        .await
        .unwrap();
    edit = saved.result;
    edit.unsandboxed_risk_ids.push("access-test-risk".into());
    access
        .put_settings(&owner.id, &edit, "settings-3")
        .await
        .unwrap();
    assert_eq!(
        access
            .catalog
            .remove(&owner.id, Kind::Risks, "access-test-risk", 1, None)
            .await
            .unwrap_err()
            .code,
        "in_use"
    );
    edit = access.settings(&owner.id).await.unwrap();
    edit.unsandboxed_risk_ids.push("missing-risk".into());
    assert_eq!(
        access
            .put_settings(&owner.id, &edit, "settings-4")
            .await
            .unwrap_err()
            .code,
        "invalid"
    );
    assert_eq!(access.settings(&owner.id).await.unwrap().version, 3);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_settings_require_current_owner_before_receipt_replay_and_audit_once() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(Accounts::open(test.store).await.unwrap());
    let owner = accounts
        .ensure_owner(None, "Owner", [97; 16])
        .await
        .unwrap();
    let user = accounts
        .create_local_user(&owner.id, "User", "user@example.test", "a good password")
        .await
        .unwrap()
        .account;
    let access = Access::new(accounts.clone());
    let mut edit = access.settings(&owner.id).await.unwrap();
    edit.deny_cooldown_minutes = 5;
    assert_eq!(
        access
            .put_settings(&user.id, &edit, "not-owner")
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    access
        .put_settings(&owner.id, &edit, "owner-change")
        .await
        .unwrap();
    access
        .put_settings(&owner.id, &edit, "owner-change")
        .await
        .unwrap();
    transaction!(&accounts.store, |c| {
        accounts.store.scope(c, &owner.tenant).await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event='access.settings_changed'")
            .bind(&owner.tenant).fetch_one(&mut *c).await?;
        assert_eq!(count, 1);
        sqlx::query("UPDATE auth_accounts SET disabled_at=1 WHERE id=$1").bind(&owner.id).execute(c).await?;
        Ok(())
    }).unwrap();
    assert_eq!(
        access
            .put_settings(&owner.id, &edit, "owner-change")
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
}
