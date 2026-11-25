use super::*;
use crate::{
    catalog::access::MachineFacts,
    machines::access::{Access, Decision, Narrow},
};
use devagotchi_agenthub::{
    Actor,
    policy::{Effect, Identity, Rule, Subject, catalog as policy},
};

pub(super) async fn process_grant(admin: &Admin) -> String {
    requested(admin).await;
    let access = Access::new(admin.hub.accounts.clone());
    let principal = format!("@agent:{}:worker", admin.owner);
    let actor = Actor {
        id: &principal,
        human: false,
        wip_limit: None,
        policy: Some(Identity {
            kind: "codex".into(),
            roles: vec!["worker".into()],
        }),
    };
    let machine = access
        .machine_administration(&admin.owner, "lab", None)
        .await
        .unwrap()
        .binding;
    let input = serde_json::from_value(json!({"machine":"lab", "duration":"once",
        "selection":{"kind":"items","items":[{"capability":"proc.interactive","sandbox_level":"read-only","selector":{}}]},
        "reason":"Run an interactive process to inspect this source repository."})).unwrap();
    let pending = access
        .request(
            &admin.owner,
            &actor,
            None,
            &machine,
            &MachineFacts {
                os: "linux",
                display_server: "x11",
                kind: "native",
            },
            &input,
            "heartbeat-request",
        )
        .await
        .unwrap();
    access
        .decide(
            &admin.owner,
            None,
            &pending.result.request_ids[0],
            1,
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            "heartbeat-approve",
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap()
}
pub(super) fn capabilities(admin: &Admin, id: u64) {
    answer(
        admin,
        id,
        RemoteReply::Capabilities {
            version: REMOTE_ACCESS_VERSION,
            os: "linux".into(),
            sandbox_levels: vec![GrantLevel::ReadOnly],
        },
    );
}
pub(super) async fn start(
    admin: &Admin,
    grant: &str,
    agent: &mut Person,
    calls: &mut Calls,
) -> GrantAuthority {
    let broker = async {
        let (id, request) = next(calls).await;
        assert!(matches!(request, RemoteRequest::Capabilities {}));
        capabilities(admin, id);
        let (id, request) = next(calls).await;
        let RemoteRequest::Prepare {
            request: RequestBody::OperationStart(input),
            ..
        } = request
        else {
            panic!("expected prepare")
        };
        assert!(input.interactive.is_some());
        let mut call = GrantCall::new(GrantCapability::Process, GrantLevel::ReadOnly);
        call.argv = vec!["/usr/bin/git".into(), "status".into()];
        call.paths = vec!["/work".into()];
        call.runtime_s = Some(5);
        answer(
            admin,
            id,
            RemoteReply::Prepared {
                action: GrantAction::ProcessStart,
                operation_key: input.idempotency_key,
                input_sha256: "b".repeat(64),
                call,
            },
        );
        let (id, request) = next(calls).await;
        let RemoteRequest::Execute {
            authority,
            request: RequestBody::OperationStart(_),
            ..
        } = request
        else {
            panic!("expected admitted start")
        };
        assert_eq!(authority.binding.grant_id, grant);
        answer(
            admin,
            id,
            RemoteReply::Executed(ResponseBody::Start(StartOutput {
                operation: view(false),
                replayed: false,
            })),
        );
        authority
    };
    let (result, authority) = tokio::join!(
        admin.hub.call(
            agent,
            "operation_start",
            json!({
        "machine":"lab","idempotency_key":"heartbeat-start","executable":"git","args":["status"],
        "cwd":"/work","deadline_ms":5000,"interactive":{"mode":"pipe"}})
        ),
        broker
    );
    assert_ne!(result["isError"], true, "{result}");
    authority
}
pub(super) async fn revoke(admin: &Admin, key: &str) {
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    admin
        .hub
        .accounts
        .revoke_key(&owner.tenant, &owner.id, key)
        .await
        .unwrap();
}
pub(super) async fn deadline(admin: &Admin, grant: &str) -> i64 {
    let accounts = &admin.hub.accounts;
    let owner = accounts.get(&admin.owner).await.unwrap().unwrap();
    devagotchi_store::transaction!(accounts.store(), |c| {
        accounts.store().scope(c, &owner.tenant).await?;
        Ok(sqlx::query_scalar("SELECT authority_expires_at FROM machine_admissions WHERE tenant_id=$1 AND grant_id=$2")
            .bind(&owner.tenant).bind(grant).fetch_one(c).await?)
    }).unwrap()
}
pub(super) async fn expire(admin: &Admin, grant: &str) {
    let accounts = &admin.hub.accounts;
    let owner = accounts.get(&admin.owner).await.unwrap().unwrap();
    devagotchi_store::transaction!(accounts.store(), |c| {
        accounts.store().scope(c, &owner.tenant).await?;
        sqlx::query("UPDATE machine_admissions SET authority_expires_at=1 WHERE tenant_id=$1 AND grant_id=$2")
            .bind(&owner.tenant).bind(grant).execute(c).await?;
        Ok(())
    }).unwrap();
}
pub(super) async fn deny_heartbeat(admin: &Admin) {
    let accounts = &admin.hub.accounts;
    let owner = accounts.get(&admin.owner).await.unwrap().unwrap();
    devagotchi_store::transaction!(accounts.store(), |c| {
        accounts.store().scope(c, &owner.tenant).await?;
        policy::put_rule_in(
            c,
            &owner.tenant,
            &Subject {
                id: &admin.owner,
                human: true,
                kind: "human",
                roles: &["admin".into()],
            },
            &Rule {
                id: "deny-heartbeat".into(),
                name: "Deny explicit heartbeat".into(),
                subject: "agent".into(),
                action: "proc.heartbeat".into(),
                selector: Default::default(),
                effect: Effect::Deny,
                priority: 100,
                enabled: true,
                version: 0,
            },
        )
        .await?;
        Ok(())
    })
    .unwrap();
}
