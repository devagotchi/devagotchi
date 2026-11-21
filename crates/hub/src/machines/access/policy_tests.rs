use super::*;
use super::{
    decisions::Decision,
    policies::{AcknowledgedRisk, PolicyEdit},
    request_tests::{Fixture, input},
};
use crate::{catalog::Kind, store};
use serde_json::json;

impl Fixture {
    pub(super) async fn policy_edit(&self) -> PolicyEdit {
        let settings = self.access.settings(&self.owner.id).await.unwrap();
        let risks = self
            .access
            .catalog
            .list(&self.owner.id, Kind::Risks, None)
            .await
            .unwrap();
        PolicyEdit {
            version: 0,
            allowed: true,
            expires_at: None,
            acknowledged: settings
                .unsandboxed_risk_ids
                .iter()
                .map(|id| {
                    let row = risks.iter().find(|r| &r.id == id).unwrap();
                    AcknowledgedRisk {
                        id: row.id.clone(),
                        version: row.version,
                    }
                })
                .collect(),
        }
    }
    pub(super) async fn grant(&self, unsandboxed: bool, key: &str) -> String {
        let mut request = input();
        if unsandboxed {
            request.selection = serde_json::from_value(json!({"kind":"items","items":[
            {"capability":"exec","sandbox_level":"unsandboxed","selector":{}}]}))
            .unwrap();
        }
        let id = self
            .request(&request, key)
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
            &format!("approve-{key}"),
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap()
    }
    pub(super) async fn lifecycle_counts(&self) -> (i64, i64, i64) {
        transaction!(&self.access.accounts.store, |c| {
            self.access
                .accounts
                .store
                .scope(c, &self.owner.tenant)
                .await?;
            let live: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM machine_grants WHERE tenant_id=$1 AND ended_at IS NULL",
            )
            .bind(&self.owner.tenant)
            .fetch_one(&mut *c)
            .await?;
            let ended: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM machine_grants WHERE tenant_id=$1 AND ended_at IS NOT NULL",
            )
            .bind(&self.owner.tenant)
            .fetch_one(&mut *c)
            .await?;
            let notices: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM machine_access_notifications WHERE tenant_id=$1",
            )
            .bind(&self.owner.tenant)
            .fetch_one(c)
            .await?;
            Ok((live, ended, notices))
        })
        .unwrap()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_policy_requires_current_complete_acknowledgments_and_cas() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let mut edit = f.policy_edit().await;
    let risk = edit.acknowledged.pop().unwrap();
    assert_eq!(
        f.access
            .set_policy(&f.owner.id, None, &edit, "missing")
            .await
            .unwrap_err()
            .code,
        "risks_not_acknowledged"
    );
    edit.acknowledged.push(risk);
    let old = edit.acknowledged[0].clone();
    f.access
        .catalog
        .put(
            &f.owner.id,
            Kind::Risks,
            &old.id,
            old.version,
            &json!({"plain_text":"Updated risk explanation"}),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        f.access
            .set_policy(&f.owner.id, None, &edit, "stale-risk")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    edit.acknowledged[0].version += 1;
    edit.expires_at = Some(1);
    assert!(
        f.access
            .set_policy(&f.owner.id, None, &edit, "expired")
            .await
            .is_err()
    );
    edit.expires_at = None;
    let enabled = f
        .access
        .set_policy(&f.owner.id, None, &edit, "enable")
        .await
        .unwrap();
    assert_eq!(enabled.result.version, 1);
    assert_eq!(
        enabled.result.acknowledged[0].data["plain_text"],
        "Updated risk explanation"
    );
    assert!(
        f.access
            .set_policy(&f.owner.id, None, &edit, "enable")
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(
        f.access
            .set_policy(&f.owner.id, None, &edit, "stale-policy")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_policy_disable_keeps_other_coverage_then_ends_only_uncovered_grants() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let edit = f.policy_edit().await;
    f.access
        .set_policy(&f.owner.id, None, &edit, "environment-enable")
        .await
        .unwrap();
    f.access
        .set_policy(&f.owner.id, Some(&f.machine), &edit, "machine-enable")
        .await
        .unwrap();
    f.grant(true, "unsafe").await;
    f.grant(false, "readonly").await;
    let disable = PolicyEdit {
        version: 1,
        allowed: false,
        acknowledged: vec![],
        expires_at: None,
    };
    let env = f
        .access
        .set_policy(&f.owner.id, None, &disable, "environment-disable")
        .await
        .unwrap();
    assert_eq!(env.result.ended_grants, 0);
    assert_eq!(f.lifecycle_counts().await, (2, 0, 0));
    let machine = f
        .access
        .set_policy(&f.owner.id, Some(&f.machine), &disable, "machine-disable")
        .await
        .unwrap();
    assert_eq!(machine.result.ended_grants, 1);
    assert_eq!(f.lifecycle_counts().await, (1, 1, 1));
    assert!(
        f.access
            .set_policy(&f.owner.id, Some(&f.machine), &disable, "machine-disable")
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(f.lifecycle_counts().await, (1, 1, 1));
    // Re-enabling policy must not revive an ended grant.
    let enable = PolicyEdit { version: 2, ..edit };
    f.access
        .set_policy(&f.owner.id, None, &enable, "reenable")
        .await
        .unwrap();
    assert_eq!(f.lifecycle_counts().await, (1, 1, 1));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_policy_machine_generation_cannot_authorize_reenrolled_machine() {
    let test = or_skip!(store::testing::postgres());
    let mut f = Fixture::new(test.store).await;
    let edit = f.policy_edit().await;
    f.access
        .set_policy(&f.owner.id, Some(&f.machine), &edit, "enable")
        .await
        .unwrap();
    let old = f.machine.clone();
    let code = f
        .devices
        .issue_owned_code(&f.owner.id, "lab", [104; 16], 16)
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
    let mut request = input();
    request.selection=serde_json::from_value(json!({"kind":"items","items":[{"capability":"exec","sandbox_level":"unsandboxed","selector":{}}]})).unwrap();
    assert_eq!(
        f.request(&request, "old-policy").await.unwrap_err().code,
        "policy_denied"
    );
    assert_eq!(
        f.access
            .set_policy(&f.owner.id, Some(&old), &edit, "enable")
            .await
            .unwrap_err()
            .code,
        "machine_reenrolled"
    );
}
