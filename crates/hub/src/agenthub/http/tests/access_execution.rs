//! Real MCP credentials may change while the authenticated broker prepares a call.
use super::{
    access_reads::{requested, token},
    access_requests::connect,
    principals::Admin,
    *,
};
use crate::{identity::accounts::api_keys::Scope, tenancy::tests::tenants::Person};
use devagotchi_contracts::*;

fn command() -> Value {
    json!({"machine":"lab","idempotency_key":"start-key","executable":"git",
        "args":["status"],"cwd":"/work","deadline_ms":5000})
}
fn person(token: String) -> Person {
    Person {
        token,
        cookie: String::new(),
        session: None,
    }
}
async fn granted(admin: &Admin) -> String {
    let request = requested(admin).await;
    let (status, value, _) = response(
        admin
            .request(
                Method::POST,
                &format!("/api/v1/access-requests/{}/approve", request.request_ids[0]),
            )
            .header("idempotency-key", "approve-execution")
            .json(&json!({"version":1,"duration":"once"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value["result"]["grant_id"].as_str().unwrap().into()
}
async fn counts(admin: &Admin, grant: &str) -> (i64, i64) {
    let accounts = &admin.hub.accounts;
    let owner = accounts.get(&admin.owner).await.unwrap().unwrap();
    devagotchi_store::transaction!(accounts.store(), |c| {
        accounts.store().scope(c, &owner.tenant).await?;
        let uses: i64 =
            sqlx::query_scalar("SELECT uses FROM machine_grants WHERE tenant_id=$1 AND id=$2")
                .bind(&owner.tenant)
                .bind(grant)
                .fetch_one(&mut *c)
                .await?;
        let admissions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM machine_admissions WHERE tenant_id=$1 AND grant_id=$2",
        )
        .bind(&owner.tenant)
        .bind(grant)
        .fetch_one(c)
        .await?;
        Ok((uses, admissions))
    })
    .unwrap()
}
async fn start_race(revoke: bool) {
    let admin = or_skip!(Admin::new());
    let grant = granted(&admin).await;
    let (key, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let mut agent = person(bearer);
    let mut calls = connect(&admin).await;
    let broker = async {
        let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(5), calls.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(wire.remote, RemoteRequest::Capabilities {}));
        admin.hub.hub.links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                os: "linux".into(),
                sandbox_levels: vec![GrantLevel::ReadOnly],
            }),
        );
        let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(5), calls.recv())
            .await
            .unwrap()
            .unwrap();
        let RemoteRequest::Prepare {
            actor,
            request: RequestBody::OperationStart(input),
            ..
        } = wire.remote
        else {
            panic!("expected preparation")
        };
        assert_eq!(actor.account, admin.owner);
        assert_eq!(actor.principal, format!("@agent:{}:worker", admin.owner));
        assert_eq!(input.idempotency_key, "start-key");
        assert_eq!(counts(&admin, &grant).await, (0, 0));
        if revoke {
            let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
            admin
                .hub
                .accounts
                .revoke_key(&owner.tenant, &owner.id, &key)
                .await
                .unwrap();
        }
        let mut call = GrantCall::new(GrantCapability::Exec, GrantLevel::ReadOnly);
        call.argv = vec!["/usr/bin/git".into(), "status".into()];
        call.paths = vec!["/work".into()];
        call.runtime_s = Some(5);
        admin.hub.hub.links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Prepared {
                action: GrantAction::Exec,
                operation_key: "start-key".into(),
                input_sha256: "a".repeat(64),
                call,
            }),
        );
        if !revoke {
            let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(5), calls.recv())
                .await
                .unwrap()
                .unwrap();
            let RemoteRequest::Execute {
                authority,
                request: RequestBody::OperationStart(input),
                ..
            } = wire.remote
            else {
                panic!("expected execution")
            };
            assert_eq!(authority.binding.grant_id, grant);
            assert_eq!(authority.binding.principal, actor.principal);
            assert_eq!(input.idempotency_key, "start-key");
            assert_eq!(counts(&admin, &grant).await, (1, 1));
            // An uncertain broker refusal must preserve the consumed retry identity.
            admin.hub.hub.links.answer_remote_for_test(
                "lab",
                id,
                RemoteWireResponse::Error(BridgeError::new(
                    "test_broker_refusal",
                    "controlled broker outcome",
                )),
            );
        }
    };
    let (result, ()) = tokio::join!(
        admin.hub.call(&mut agent, "operation_start", command()),
        broker
    );
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(
        result["structuredContent"]["code"],
        if revoke {
            "unauthenticated"
        } else {
            "test_broker_refusal"
        },
        "{result}"
    );
    assert_eq!(
        counts(&admin, &grant).await,
        if revoke { (0, 0) } else { (1, 1) }
    );
    assert!(
        calls.try_recv().is_err(),
        "unexpected dispatch after final response"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_mcp_revoked_credential_during_prepare_spends_nothing_and_never_executes() {
    start_race(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_mcp_selects_grant_and_preserves_admission_on_transport_failure() {
    start_race(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_mcp_pending_request_and_unbound_owner_key_cannot_run() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let mut calls = connect(&admin).await;
    for bound in [true, false] {
        let (_, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, bound).await;
        let result = admin
            .hub
            .call(&mut person(bearer), "operation_start", command())
            .await;
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(
            result["structuredContent"]["code"],
            if bound {
                "grant_required"
            } else {
                "not_granted"
            },
            "{result}"
        );
        assert!(
            calls.try_recv().is_err(),
            "an unapproved call reached the broker"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_start_mcp_configured_local_socket_cannot_bypass_account_grants() {
    let hub = or_skip!(crate::tenancy::tests::tenants::hosted_with(
        None,
        |dir, _| {
            json!({"tenancy":{"mode":"single"},"brokers":[{
                "name":"local-test","socket":dir.join("unused-broker.sock")
            }]})
        }
    ));
    let owner = hub
        .accounts
        .ensure_owner(None, "Owner", [92; 16])
        .await
        .unwrap();
    hub.config.learn_owner(&owner.id).unwrap();
    let session = hub.accounts.open_session(&owner.id, None).await.unwrap();
    let admin = Admin {
        hub,
        owner: owner.id,
        cookie: format!("dg_session={session}"),
    };
    requested(&admin).await;
    let (_, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let mut input = command();
    input["machine"] = "local-test".into();
    let result = admin
        .hub
        .call(&mut person(bearer), "operation_start", input)
        .await;
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(
        result["structuredContent"]["code"], "not_granted",
        "{result}"
    );
    assert!(
        result["structuredContent"]["message"]
            .as_str()
            .unwrap()
            .contains("local IPC")
    );
}
