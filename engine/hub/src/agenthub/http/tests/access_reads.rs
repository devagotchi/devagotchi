use super::principals::Admin;
use super::*;
use crate::{
    catalog::access::MachineFacts,
    identity::accounts::{
        agents::AgentEdit,
        api_keys::{NewKey, Scope},
    },
    machines::{
        access::{Access, Requested},
        devices::Devices,
    },
};
use devagotchi_agenthub::{Actor, policy::Identity};

pub(super) async fn requested(admin: &Admin) -> Requested {
    requested_for(admin, "exec").await
}
pub(super) async fn requested_for(admin: &Admin, capability: &str) -> Requested {
    let accounts = &admin.hub.accounts;
    let agent = accounts
        .put_agent(
            &admin.owner,
            &admin.owner,
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
    let devices = Devices::new(accounts.store().clone());
    let code = devices
        .issue_owned_code(&admin.owner, "lab", [132; 16], 16)
        .await
        .unwrap();
    devices
        .enroll("lab", &code, "machine-key", false)
        .await
        .unwrap();
    let machine = devagotchi_store::transaction!(accounts.store(), |c| {
        devices.bind_access_in(c, "lab", Some(&admin.owner)).await
    })
    .unwrap();
    let principal = agent.principal();
    let actor = Actor {
        id: &principal,
        human: false,
        wip_limit: None,
        policy: Some(Identity {
            kind: agent.kind,
            roles: agent.roles,
        }),
    };
    let input =
        serde_json::from_value(json!({"machine":"lab","selection":{"kind":"items","items":[
        {"capability":capability,"sandbox_level":"read-only","selector":{}}]},"duration":"1h",
        "reason":"Inspect the source repository to prepare the requested change."}))
        .unwrap();
    Access::new(accounts.clone())
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
            "http-read-request",
        )
        .await
        .unwrap()
        .result
}
pub(super) async fn token(
    admin: &Admin,
    scopes: Vec<Scope>,
    machine: Option<&str>,
    project: Option<&str>,
    bound: bool,
) -> (String, String) {
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (key, token) = admin
        .hub
        .accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &NewKey {
                name: "Access reader".into(),
                scopes,
                machines: machine.map(|m| vec![m.into()]).unwrap_or_default(),
                projects: project.map(|p| vec![p.into()]).unwrap_or_default(),
                agent_id: bound.then(|| "worker".into()),
                hours: 24,
            },
        )
        .await
        .unwrap();
    (key.id, token)
}
fn read(admin: &Admin, token: &str, path: &str) -> reqwest::RequestBuilder {
    admin
        .hub
        .http
        .get(format!("{}{path}", admin.hub.url))
        .bearer_auth(token)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_read_http_preserves_account_principal_and_key_machine_boundaries() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let path = "/api/v1/access-requests";
    let lookup = format!("{path}/{}", group.group_id);
    let (status, page, _) = response(admin.request(Method::GET, path)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["rows"].as_array().unwrap().len(), 1);
    let (_, limited) = token(&admin, vec![Scope::MachinesRead], Some("lab"), None, true).await;
    assert_eq!(
        response(read(&admin, &limited, path)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(read(&admin, &limited, &format!("{path}?machine=elsewhere")))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(read(&admin, &limited, &format!("{path}?machine=lab")))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        response(read(&admin, &limited, &lookup)).await.1["status"],
        "pending"
    );
    for (scopes, machine, project, bound) in [
        (vec![Scope::BoardsRead], None, None, true),
        (vec![Scope::MachinesRead], Some("elsewhere"), None, true),
        (vec![Scope::MachinesRead], None, Some("demo"), true),
        (vec![Scope::MachinesRead], None, None, false),
    ] {
        let (_, token) = token(&admin, scopes, machine, project, bound).await;
        assert_eq!(
            response(read(&admin, &token, &lookup)).await.0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        response(read(&admin, &limited, &lookup).header("X-Devagotchi-Seat", "worker"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(admin.request(Method::GET, &format!("{lookup}?timeout_s=26")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(admin.request(Method::GET, &format!("{path}?after=invalid")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let user = admin
        .hub
        .accounts
        .create_local_user(
            &admin.owner,
            "Member",
            "access-member@example.test",
            "A good local password",
        )
        .await
        .unwrap()
        .account;
    let session = admin
        .hub
        .accounts
        .open_session(&user.id, None)
        .await
        .unwrap();
    let member = |path: &str| {
        admin
            .hub
            .http
            .get(format!("{}{path}", admin.hub.url))
            .header("cookie", format!("dg_session={session}"))
    };
    assert_eq!(response(member(path)).await.1["rows"], json!([]));
    assert_eq!(response(member(&lookup)).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_read_long_poll_rechecks_revoked_credentials_before_returning_history() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let (key, token) = token(&admin, vec![Scope::MachinesRead], None, None, true).await;
    let path = format!("/api/v1/access-requests/{}?timeout_s=3", group.group_id);
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (result, _) = tokio::join!(response(read(&admin, &token, &path)), async {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        admin
            .hub
            .accounts
            .revoke_key(&owner.tenant, &owner.id, &key)
            .await
            .unwrap();
    });
    assert_eq!(result.0, StatusCode::UNAUTHORIZED, "{}", result.1);
}
