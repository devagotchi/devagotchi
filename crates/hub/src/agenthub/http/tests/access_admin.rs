use super::*;
use super::{access_reads::requested, principals::Admin};
use crate::{
    catalog::access::MachineFacts,
    machines::{access::Access, devices::Devices},
};
use devagotchi_agenthub::{Actor, policy::Identity};

async fn policy(admin: &Admin) -> Value {
    let (_, settings, _) = response(admin.request(Method::GET, "/api/v1/access-settings")).await;
    let (_, risks, _) = response(admin.request(Method::GET, "/api/v1/risks")).await;
    let acknowledgments: Vec<Value> = settings["unsandboxed_risk_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let risk = risks["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| &r["id"] == id)
                .unwrap();
            json!({"id":id,"version":risk["version"]})
        })
        .collect();
    json!({"version":0,"allowed":true,"acknowledged":acknowledgments,"expires_at":null})
}
async fn approve(admin: &Admin, request: &str, key: &str) -> String {
    let decision = serde_json::from_value(json!({"decision":"approve","narrow":{}})).unwrap();
    Access::new(admin.hub.accounts.clone())
        .decide(&admin.owner, None, request, 1, &decision, key)
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admin_machine_control_requires_current_generation_versions_and_person_origin() {
    let admin = or_skip!(Admin::new());
    let group = requested(&admin).await;
    let (_, token) = super::access_reads::token(
        &admin,
        vec![crate::identity::accounts::api_keys::Scope::MachinesManage],
        None,
        None,
        true,
    )
    .await;
    for (method, path) in [
        (Method::PUT, "/api/v1/sandbox-policies"),
        (Method::POST, "/api/v1/resume-alerts/check"),
        (Method::POST, "/api/v1/machines/lab/access-control"),
    ] {
        assert_eq!(
            response(
                admin
                    .hub
                    .http
                    .request(method, format!("{}{path}", admin.hub.url))
                    .bearer_auth(&token)
                    .header("idempotency-key", "agent-write")
                    .json(&json!({}))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let grant = approve(&admin, &group.request_ids[0], "approve").await;
    let path = "/api/v1/machines/lab/access-control";
    let (status, current, _) = response(admin.request(Method::GET, path)).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    let generation = &current["binding"]["generation"];
    let mut machine_policy = policy(&admin).await;
    machine_policy["machine"] = json!({"name":"lab","generation":generation});
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/sandbox-policies")
                .header("idempotency-key", "machine-policy")
                .json(&machine_policy)
        )
        .await
        .0,
        StatusCode::OK
    );
    let pause = json!({"generation":generation,"version":1,"action":"pause"});
    let send = || {
        admin
            .request(Method::POST, path)
            .header("idempotency-key", "pause")
            .json(&pause)
    };
    assert_eq!(
        admin
            .hub
            .http
            .post(format!("{}{path}", admin.hub.url))
            .header("cookie", &admin.cookie)
            .header("origin", "https://elsewhere.example.test")
            .header("idempotency-key", "pause")
            .json(&pause)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let (status, changed, _) = response(send()).await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["result"]["control"]["paused"], true);
    assert_eq!(changed["result"]["interrupted_grants"], 1);
    assert!(response(send()).await.2);
    let resume = json!({"generation":generation,"version":2,"action":"resume"});
    assert_eq!(
        response(
            admin
                .request(Method::POST, path)
                .header("idempotency-key", "resume")
                .json(&resume)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        response(
            admin
                .request(Method::POST, path)
                .header("idempotency-key", "stale")
                .json(&pause)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let devices = Devices::new(admin.hub.accounts.store().clone());
    let code = devices
        .issue_owned_code(&admin.owner, "lab", [133; 16], 16)
        .await
        .unwrap();
    devices
        .enroll("lab", &code, "machine-key", false)
        .await
        .unwrap();
    assert_eq!(
        response(send()).await.0,
        StatusCode::CONFLICT,
        "old enrollment receipt cannot administer replacement"
    );
    let revoke = format!("/api/v1/grants/{grant}/revoke");
    let (_, policies, _) = response(admin.request(Method::GET, "/api/v1/sandbox-policies")).await;
    assert_eq!(
        policies["rows"][0]["live"], false,
        "old enrollment policy is no longer live"
    );
    let (status, ended, _) = response(
        admin
            .request(Method::POST, &revoke)
            .header("idempotency-key", "revoke")
            .json(&json!({"version":2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ended}");
    assert_eq!(ended["result"]["reason"], "revoked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_admin_policy_and_resume_actions_are_replayable_and_never_create_authority() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let value = policy(&admin).await;
    let send = || {
        admin
            .request(Method::PUT, "/api/v1/sandbox-policies")
            .header("idempotency-key", "policy")
            .json(&value)
    };
    let (status, saved, _) = response(send()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert!(response(send()).await.2);
    let (_, listed, _) = response(admin.request(Method::GET, "/api/v1/sandbox-policies")).await;
    assert_eq!(listed["rows"][0]["live"], true);
    let access = Access::new(admin.hub.accounts.clone());
    let machine = access
        .machine_administration(&admin.owner, "lab", None)
        .await
        .unwrap()
        .binding;
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
    let request=serde_json::from_value(json!({"machine":"lab","selection":{"kind":"items","items":[
        {"capability":"exec","sandbox_level":"unsandboxed","selector":{}}]},"duration":"1h","reason":"Run the explicitly approved local development inspection."})).unwrap();
    let requested = access
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
            &request,
            "risky",
        )
        .await
        .unwrap()
        .result;
    let grant = approve(&admin, &requested.request_ids[0], "approve-risky").await;
    let check = || {
        admin
            .request(Method::POST, "/api/v1/resume-alerts/check")
            .header("idempotency-key", "resume-check")
            .json(&json!({"ui_session_id":"browser-test","reason":"load"}))
    };
    let (status, checked, _) = response(check()).await;
    assert_eq!(status, StatusCode::OK, "{checked}");
    assert_eq!(checked["result"]["shown"], 1);
    assert!(response(check()).await.2);
    let (_, alerts, _) = response(admin.request(Method::GET, "/api/v1/resume-alerts")).await;
    let id = alerts["alerts"][0]["id"].as_str().unwrap();
    let path = format!("/api/v1/resume-alerts/{id}/revoke");
    let ack = || {
        admin
            .request(Method::POST, &path)
            .header("idempotency-key", "ack")
            .json(&json!({"version":1}))
    };
    let (status, ended, _) = response(ack()).await;
    assert_eq!(status, StatusCode::OK, "{ended}");
    assert_eq!(ended["result"]["grant_id"], grant);
    assert!(response(ack()).await.2);
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/resume-alerts"))
            .await
            .1["alerts"],
        json!([])
    );
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/grants?live=true"))
            .await
            .1["rows"],
        json!([])
    );
}
