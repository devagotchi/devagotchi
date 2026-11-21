use super::*;
use crate::{
    catalog::access::{MachineFacts, Selection},
    identity::accounts::{Account, agents::AgentEdit},
    machines::devices::{Devices, MachineBinding},
    store,
};
use devagotchi_agenthub::{Actor, policy::Identity};
use devagotchi_contracts::GrantDuration;
use serde_json::json;
use sqlx::Row;

pub(super) struct Fixture {
    pub access: Access,
    pub owner: Account,
    pub principal: String,
    pub machine: MachineBinding,
    pub devices: Devices,
}
impl Fixture {
    pub async fn new(store: store::Store) -> Self {
        let accounts = Arc::new(Accounts::open(store.clone()).await.unwrap());
        let owner = accounts
            .ensure_owner(None, "Owner", [101; 16])
            .await
            .unwrap();
        let agent = accounts
            .put_agent(
                &owner.id,
                &owner.id,
                &AgentEdit {
                    id: "worker".into(),
                    name: "Worker".into(),
                    kind: "codex".into(),
                    roles: vec!["worker".into()],
                    enabled: true,
                    wip_limit: None,
                    version: 0,
                },
            )
            .await
            .unwrap();
        let devices = Devices::new(store.clone());
        let code = devices
            .issue_owned_code(&owner.id, "lab", [102; 16], 16)
            .await
            .unwrap();
        devices
            .enroll("lab", &code, "machine-key", false)
            .await
            .unwrap();
        let machine = transaction!(&store, |c| {
            devices.bind_access_in(c, "lab", Some(&owner.id)).await
        })
        .unwrap();
        Self {
            access: Access::new(accounts),
            owner,
            principal: agent.principal(),
            machine,
            devices,
        }
    }
    pub fn actor(&self) -> Actor<'_> {
        Actor {
            id: &self.principal,
            human: false,
            wip_limit: None,
            policy: Some(Identity {
                kind: "codex".into(),
                roles: vec!["worker".into()],
            }),
        }
    }
    pub async fn request(
        &self,
        input: &RequestInput,
        key: &str,
    ) -> BridgeResult<Changed<Requested>> {
        self.access
            .request(
                &self.owner.id,
                &self.actor(),
                None,
                &self.machine,
                &MachineFacts {
                    os: "linux",
                    display_server: "x11",
                    kind: "native",
                },
                input,
                key,
            )
            .await
    }
    async fn counts(&self) -> (i64, i64) {
        transaction!(&self.access.accounts.store, |c| {
            self.access.accounts.store.scope(c,&self.owner.tenant).await?;
            let requests:i64=sqlx::query_scalar("SELECT count(*) FROM machine_access_requests WHERE tenant_id=$1")
                .bind(&self.owner.tenant).fetch_one(&mut *c).await?;
            let events:i64=sqlx::query_scalar("SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event='access.requested'")
                .bind(&self.owner.tenant).fetch_one(c).await?;
            Ok((requests,events))
        }).unwrap()
    }
}
pub(super) fn input() -> RequestInput {
    serde_json::from_value(json!({"machine":"lab","selection":{"kind":"items","items":[
        {"capability":"exec","sandbox_level":"read-only","selector":{}}]},
        "duration":"1h","reason":"Inspect the repository to prepare the requested changes."}))
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_concurrent_retry_preserves_one_group_and_one_audit_event() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let original = input();
    let (a, b) = tokio::join!(f.request(&original, "same"), f.request(&original, "same"));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.result, b.result);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(f.counts().await, (1, 1));
    let mut changed = original.clone();
    changed.reason.push_str(" Changed request.");
    assert_eq!(
        f.request(&changed, "same").await.unwrap_err().code,
        "request_conflict"
    );
    assert_eq!(f.counts().await, (1, 1));
    f.devices.revoke_owned(&f.owner.id, "lab").await.unwrap();
    assert_eq!(
        f.request(&original, "same").await.unwrap_err().code,
        "not_found"
    );
    let code = f
        .devices
        .issue_owned_code(&f.owner.id, "lab", [103; 16], 16)
        .await
        .unwrap();
    f.devices
        .enroll("lab", &code, "machine-key", false)
        .await
        .unwrap();
    assert_eq!(
        f.request(&original, "same").await.unwrap_err().code,
        "machine_reenrolled"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_refusals_leave_no_partial_group_and_once_limits_admissions() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let mut request = input();
    request.reason = "please".into();
    assert_eq!(
        f.request(&request, "short").await.unwrap_err().code,
        "reason_too_short"
    );
    request = input();
    request.duration = Some(GrantDuration::Task);
    assert!(f.request(&request, "unbound").await.is_err());
    request = input();
    request.uses_max = Some(0);
    assert!(f.request(&request, "zero").await.is_err());
    request = input();
    if let Selection::Items { items } = &mut request.selection {
        let mut invalid = items[0].clone();
        invalid.selector.shell_profile = Some("missing-profile".into());
        items.push(invalid);
    }
    assert!(f.request(&request, "bad-last-item").await.is_err());
    assert_eq!(f.counts().await, (0, 0));
    request = input();
    request.duration = Some(GrantDuration::Once);
    request.uses_max = Some(100);
    let saved = f.request(&request, "once").await.unwrap();
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c,&f.owner.tenant).await?;
        let row=sqlx::query("SELECT duration,uses_max,prepared_item::text FROM machine_access_requests WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant).bind(&saved.result.request_ids[0]).fetch_one(c).await?;
        assert_eq!(row.try_get::<String,_>("duration")?,"once");
        assert_eq!(row.try_get::<i64,_>("uses_max")?,1);
        let snapshot:serde_json::Value=serde_json::from_str(&row.try_get::<String,_>("prepared_item")?).unwrap();
        assert_eq!(snapshot["capability"]["id"],"exec");
        Ok(())
    }).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_replay_requires_current_profile_and_account() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let request = input();
    f.request(&request, "accepted").await.unwrap();
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
    assert_eq!(
        f.request(&request, "accepted").await.unwrap_err().code,
        "not_granted"
    );
    assert_eq!(f.counts().await, (1, 1));
}
