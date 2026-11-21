use super::*;
use super::{
    admission_action::AdmissionAction,
    admissions::{AdmissionInput, AdmissionMachine},
    decisions::Decision,
    request_tests::{Fixture, input},
};
use crate::store;
use devagotchi_contracts::{GrantCall, GrantCapability, GrantLevel};
use serde_json::json;

fn observe() -> RequestInput {
    let mut request = input();
    request.selection = serde_json::from_value(json!({"kind":"items","items":[
        {"capability":"gui.observe","sandbox_level":"desktop","selector":{}}]}))
    .unwrap();
    request
}
fn input_request() -> RequestInput {
    let mut request = input();
    request.selection = serde_json::from_value(json!({"kind":"items","items":[
        {"capability":"gui.input","sandbox_level":"desktop","selector":{"apps":["editor"]}}]}))
    .unwrap();
    request
}
async fn approve(f: &Fixture, request: &RequestInput, key: &str) -> String {
    let id = f.request(request, key).await.unwrap().result.request_ids[0].clone();
    f.decide(
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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_observation_existing_live_grant_allows_input_but_revoke_blocks_request_and_admission()
 {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    assert_eq!(
        f.request(&input_request(), "missing")
            .await
            .unwrap_err()
            .code,
        "observe_required"
    );
    let observation = approve(&f, &observe(), "observe").await;
    let input = approve(&f, &input_request(), "input").await;
    let mut call = GrantCall::new(GrantCapability::Input, GrantLevel::Desktop);
    call.apps = vec!["editor".into()];
    call.actions = vec!["click".into()];
    let machine = AdmissionMachine {
        binding: &f.machine,
        sandbox_levels: &[GrantLevel::Desktop],
    };
    let request = AdmissionInput {
        action: AdmissionAction::Input,
        grant_id: &input,
        operation_key: "click",
        input_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        call: &call,
    };
    f.access
        .admit(&f.owner.id, &f.actor(), None, &machine, &request)
        .await
        .unwrap();
    f.access
        .revoke(&f.owner.id, &observation, 1, "revoke-observation")
        .await
        .unwrap();
    assert_eq!(
        f.request(&input_request(), "after-revoke")
            .await
            .unwrap_err()
            .code,
        "observe_required"
    );
    assert_eq!(
        f.access
            .admit(&f.owner.id, &f.actor(), None, &machine, &request)
            .await
            .unwrap_err()
            .code,
        "observe_required"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_observation_expired_or_exhausted_grant_does_not_count_as_live() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let observation = approve(&f, &observe(), "observe").await;
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET expires_at=1 WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant)
            .bind(&observation)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        f.request(&input_request(), "expired")
            .await
            .unwrap_err()
            .code,
        "observe_required"
    );
    transaction!(&f.access.accounts.store,|c| {
        f.access.accounts.store.scope(c,&f.owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET expires_at=NULL,uses=1,uses_max=1 WHERE tenant_id=$1 AND id=$2").bind(&f.owner.tenant).bind(&observation).execute(c).await?;Ok(())
    }).unwrap();
    assert_eq!(
        f.request(&input_request(), "exhausted")
            .await
            .unwrap_err()
            .code,
        "observe_required"
    );
}
