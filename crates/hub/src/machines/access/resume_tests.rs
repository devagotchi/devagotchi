use super::*;
use super::{
    admission_tests::call, request_tests::Fixture, resume::ResumeReason, resume_ack::ResumeAction,
};
use crate::store;
use sqlx::{Connection, Row};

impl Fixture {
    async fn risky(&self, key: &str) -> String {
        let edit = self.policy_edit().await;
        self.access
            .set_policy(&self.owner.id, None, &edit, "enable-policy")
            .await
            .unwrap();
        self.grant(true, key).await
    }
    async fn alert(&self, key: &str) -> super::resume::ResumeAlert {
        self.access
            .check_resume(
                &self.owner.id,
                "browser-instance",
                ResumeReason::Load,
                None,
                100,
                key,
            )
            .await
            .unwrap();
        self.access
            .resume_alerts(&self.owner.id, None, 100)
            .await
            .unwrap()
            .alerts
            .remove(0)
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_resume_deduplicates_open_alerts_and_continue_only_acknowledges() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let risky = f.risky("risky").await;
    f.grant(false, "safe").await;
    let first = f
        .access
        .check_resume(
            &f.owner.id,
            "browser-1",
            ResumeReason::Load,
            None,
            100,
            "load",
        )
        .await
        .unwrap();
    assert_eq!(first.result.shown, 1);
    assert_eq!(first.result.idle_minutes, 15);
    assert!(
        f.access
            .check_resume(
                &f.owner.id,
                "browser-1",
                ResumeReason::Load,
                None,
                100,
                "load"
            )
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(
        f.access
            .check_resume(
                &f.owner.id,
                "browser-2",
                ResumeReason::Reload,
                None,
                100,
                "reload"
            )
            .await
            .unwrap()
            .result
            .shown,
        0
    );
    let alert = f
        .access
        .resume_alerts(&f.owner.id, None, 100)
        .await
        .unwrap()
        .alerts
        .remove(0);
    assert_eq!(alert.grant_id, risky);
    assert!(!alert.risks.as_array().unwrap().is_empty());
    let ack = f
        .access
        .acknowledge_resume(
            &f.owner.id,
            &alert.id,
            1,
            ResumeAction::Continue,
            "continue",
        )
        .await
        .unwrap();
    assert_eq!(ack.result.version, 2);
    assert!(
        f.access
            .acknowledge_resume(
                &f.owner.id,
                &alert.id,
                1,
                ResumeAction::Continue,
                "continue"
            )
            .await
            .unwrap()
            .replayed
    );
    assert!(
        f.access
            .resume_alerts(&f.owner.id, None, 100)
            .await
            .unwrap()
            .alerts
            .is_empty()
    );
    assert_eq!(f.lifecycle_counts().await, (2, 0, 0));
    let later = f.alert("later-load").await;
    assert_ne!(later.id, alert.id);
    assert_eq!(later.grant_id, alert.grant_id);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_resume_revoke_and_pause_are_atomic_actions_with_durable_notifications() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let risky = f.risky("risky").await;
    let alert = f.alert("show").await;
    f.access
        .acknowledge_resume(&f.owner.id, &alert.id, 1, ResumeAction::Revoke, "revoke")
        .await
        .unwrap();
    assert_eq!(f.lifecycle_counts().await, (0, 1, 1));
    assert!(
        f.access
            .acknowledge_resume(&f.owner.id, &alert.id, 1, ResumeAction::Revoke, "revoke")
            .await
            .unwrap()
            .replayed
    );
    assert!(
        f.access
            .resume_alerts(&f.owner.id, None, 100)
            .await
            .unwrap()
            .alerts
            .is_empty()
    );
    let next = f.grant(true, "next-risky").await;
    let safe = f.grant(false, "safe").await;
    let active = f.admit(&safe, "running", &call()).await.unwrap();
    assert!(!active.replayed);
    let alert = f.alert("show-next").await;
    assert_eq!(alert.grant_id, next);
    assert_ne!(alert.grant_id, risky);
    f.access
        .acknowledge_resume(&f.owner.id, &alert.id, 1, ResumeAction::Pause, "pause")
        .await
        .unwrap();
    assert_eq!(f.lifecycle_counts().await, (2, 1, 3));
    assert_eq!(
        f.admit(&safe, "running", &call()).await.unwrap_err().code,
        "machine_paused"
    );
    assert!(
        f.access
            .acknowledge_resume(&f.owner.id, &alert.id, 1, ResumeAction::Pause, "pause")
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(f.lifecycle_counts().await, (2, 1, 3));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_resume_does_not_disclose_or_acknowledge_another_persons_alert() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    f.risky("risky").await;
    let alert = f.alert("show").await;
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
            .check_resume(
                &other.id,
                "other-browser",
                ResumeReason::Load,
                None,
                100,
                "show-other"
            )
            .await
            .unwrap()
            .result
            .shown,
        0
    );
    assert!(
        f.access
            .resume_alerts(&other.id, None, 100)
            .await
            .unwrap()
            .alerts
            .is_empty()
    );
    assert_eq!(
        f.access
            .acknowledge_resume(&other.id, &alert.id, 1, ResumeAction::Continue, "foreign")
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    assert!(
        f.access
            .acknowledge_resume(&f.principal, &alert.id, 1, ResumeAction::Continue, "agent")
            .await
            .is_err()
    );
    assert_eq!(
        f.access
            .acknowledge_resume(&f.owner.id, &alert.id, 2, ResumeAction::Revoke, "stale")
            .await
            .unwrap_err()
            .code,
        "not_open"
    );
    assert_eq!(f.lifecycle_counts().await, (1, 0, 0));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_resume_agent_callback_targets_the_approver_without_granting_human_authority() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.risky("risky").await;
    let shown = f
        .access
        .agent_resumed(
            &f.owner.id,
            &f.actor(),
            "mcp-instance",
            None,
            100,
            "agent-resume",
        )
        .await
        .unwrap();
    assert_eq!(shown.result.shown, 1);
    assert!(
        f.access
            .agent_resumed(
                &f.owner.id,
                &f.actor(),
                "mcp-instance",
                None,
                100,
                "agent-resume"
            )
            .await
            .unwrap()
            .replayed
    );
    let alert = f
        .access
        .resume_alerts(&f.owner.id, None, 100)
        .await
        .unwrap()
        .alerts
        .remove(0);
    assert_eq!(alert.grant_id, grant);
    assert_eq!(alert.reason, "agent_resumed");
    assert!(
        f.access
            .agent_resumed(
                &f.owner.id,
                &devagotchi_agenthub::Actor::human(&f.owner.id),
                "fake",
                None,
                100,
                "fake"
            )
            .await
            .is_err()
    );
    assert_eq!(f.lifecycle_counts().await, (1, 0, 0));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_resume_pages_all_risky_grants_without_duplicating_open_alerts() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    f.risky("first").await;
    f.grant(true, "second").await;
    f.grant(true, "third").await;
    let mut after = None;
    let mut shown = 0;
    for page in 0..3 {
        let result = f
            .access
            .check_resume(
                &f.owner.id,
                "browser",
                ResumeReason::Load,
                after.as_deref(),
                1,
                &format!("page-{page}"),
            )
            .await
            .unwrap()
            .result;
        shown += result.shown;
        after = result.next;
        if page < 2 {
            assert!(after.is_some());
        } else {
            assert!(after.is_none());
        }
    }
    assert_eq!(shown, 3);
    let mut after = None;
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..3 {
        let page = f
            .access
            .resume_alerts(&f.owner.id, after.as_deref(), 1)
            .await
            .unwrap();
        assert_eq!(page.alerts.len(), 1);
        assert!(ids.insert(page.alerts[0].id.clone()));
        after = page.next;
    }
    assert!(after.is_none());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_resume_interval_migration_preserves_saved_settings_and_invalidates_stale_edits() {
    let url = or_skip!(store::testing::empty_postgres());
    let mut admin = sqlx::AnyConnection::connect(&url).await.unwrap();
    // Match Store::open's migration connection; the generic historical fixture lacks
    // the hub's schema search path required by the machine registry migrations.
    sqlx::query("CREATE SCHEMA IF NOT EXISTS devagotchi")
        .execute(&mut admin)
        .await
        .unwrap();
    sqlx::query("SET search_path TO public, devagotchi")
        .execute(&mut admin)
        .await
        .unwrap();
    store::MIGRATIONS.run_to(42, &mut admin).await.unwrap();
    let tenant = "t-00000000-0000-4000-8000-000000000001";
    sqlx::query("INSERT INTO devagotchi.tenancy_tenants(id,home,created_at) VALUES($1,1,1)")
        .bind(tenant)
        .execute(&mut admin)
        .await
        .unwrap();
    let mut old = serde_json::to_value(AccessSettings::default()).unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("resume_alert_idle_minutes");
    old["version"] = 4.into();
    old["request_expiry_minutes"] = 45.into();
    sqlx::query("INSERT INTO devagotchi.machine_access_settings(tenant_id,version,value) VALUES($1,4,$2::jsonb)")
        .bind(tenant).bind(old.to_string()).execute(&mut admin).await.unwrap();
    store::MIGRATIONS.run(&mut admin).await.unwrap();
    let row = sqlx::query(
        "SELECT version,value::text FROM devagotchi.machine_access_settings WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(&mut admin)
    .await
    .unwrap();
    let settings: AccessSettings =
        serde_json::from_str(&row.try_get::<String, _>("value").unwrap()).unwrap();
    assert_eq!(row.try_get::<i64, _>("version").unwrap(), 5);
    assert_eq!(settings.version, 5);
    assert_eq!(settings.request_expiry_minutes, 45);
    assert_eq!(settings.resume_alert_idle_minutes, 15);
    let forced:bool=sqlx::query_scalar("SELECT relforcerowsecurity FROM pg_class WHERE oid='devagotchi.machine_access_settings'::regclass")
        .fetch_one(&mut admin).await.unwrap();
    assert!(forced);
    let mut invalid = settings;
    invalid.resume_alert_idle_minutes = 0;
    assert!(invalid.validate().is_err());
    invalid.resume_alert_idle_minutes = 61;
    assert!(invalid.validate().is_err());
}
