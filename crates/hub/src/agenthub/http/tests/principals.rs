use super::*;
use crate::tenancy::tests::tenants::{Hosted, hosted_with};

struct Admin {
    hub: Hosted,
    owner: String,
    cookie: String,
}
impl Admin {
    async fn new() -> Option<Self> {
        let hub = hosted_with(None, |_, _| json!({"tenancy":{"mode":"single"}})).await?;
        let owner = hub
            .accounts
            .ensure_owner(None, "Owner", [92; 16])
            .await
            .unwrap();
        hub.config.learn_owner(&owner.id).unwrap();
        let session = hub.accounts.open_session(&owner.id, None).await.unwrap();
        Some(Self {
            hub,
            owner: owner.id,
            cookie: format!("dg_session={session}"),
        })
    }
    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.hub
            .http
            .request(method, format!("{}{path}", self.hub.url))
            .header("cookie", &self.cookie)
            .header("origin", &self.hub.url)
    }
}
fn profile(id: &str) -> Value {
    json!({"id":id,"name":"<Worker>","kind":"codex","roles":["worker"],"enabled":true,"wip_limit":2,"version":0})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn principal_http_versions_receipts_and_browser_share_the_same_profile() {
    let admin = or_skip!(Admin::new());
    let create = || {
        admin
            .request(Method::POST, "/api/v1/principals")
            .header("idempotency-key", "principal-create")
            .json(&profile("worker"))
    };
    let (status, made, replayed) = response(create()).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert!(!replayed);
    assert_eq!(made["result"]["version"], 1);
    let (_, again, replayed) = response(create()).await;
    assert!(replayed);
    assert_eq!(made["result"], again["result"]);
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/principals")
                .header("idempotency-key", "principal-create")
                .json(&profile("other"))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut update = profile("worker");
    update["version"] = 1.into();
    update["name"] = "Changed".into();
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/principals/worker")
                .header("if-match", "2")
                .json(&update)
        )
        .await
        .0,
        StatusCode::PRECONDITION_FAILED
    );
    let (_, changed, _) = response(
        admin
            .request(Method::PUT, "/api/v1/principals/worker")
            .header("if-match", "\"1\"")
            .json(&update),
    )
    .await;
    assert_eq!(changed["result"]["version"], 2, "{changed}");
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/principals/worker")
                .json(&update)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, detail, _) = response(admin.request(Method::GET, "/api/v1/principals/worker")).await;
    assert_eq!(detail["name"], "Changed");
    let page = admin
        .request(Method::GET, "/app/agents")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Changed"));
    assert!(page.contains(&format!("/app/agents/worker?account={}", admin.owner)));
    let mut escalation = profile("human");
    escalation["is_human"] = true.into();
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/principals")
                .json(&escalation)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        admin
            .hub
            .http
            .post(format!("{}/api/v1/principals", admin.hub.url))
            .header("cookie", &admin.cookie)
            .header("origin", "https://foreign.example")
            .json(&profile("foreign"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/principals/foreign"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn principal_token_is_once_only_bound_nonhuman_and_revocation_is_replayable() {
    let admin = or_skip!(Admin::new());
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/principals")
                .json(&profile("worker"))
        )
        .await
        .0,
        StatusCode::OK
    );
    let issue = json!({"name":"Client","scopes":["boards.read"],"projects":["demo"],"hours":24});
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/principals/worker/tokens")
                .header("idempotency-key", "never-cache")
                .json(&issue)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, issued, _) = response(
        admin
            .request(Method::POST, "/api/v1/principals/worker/tokens")
            .json(&issue),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let secret = issued["token"].as_str().unwrap();
    let id = issued["info"]["id"].as_str().unwrap();
    assert_eq!(issued["info"]["agent_id"], "worker");
    let verified = admin
        .hub
        .accounts
        .verify_key(secret)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        verified.0.agent,
        Some(format!("@agent:{}:worker", admin.owner))
    );
    let (_, listed, _) =
        response(admin.request(Method::GET, "/api/v1/principals/worker/tokens")).await;
    assert_eq!(listed["tokens"][0]["id"], id);
    assert!(!listed.to_string().contains(secret));
    for path in [
        "/api/v1/principals",
        "/api/v1/principals/configuration",
        "/api/v1/principals/worker/tokens",
    ] {
        assert_eq!(
            response(
                admin
                    .hub
                    .http
                    .get(format!("{}{path}", admin.hub.url))
                    .bearer_auth(secret)
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let revoke = || {
        admin
            .request(Method::DELETE, &format!("/api/v1/tokens/{id}"))
            .header("idempotency-key", "revoke-worker")
    };
    assert_eq!(response(revoke()).await.0, StatusCode::OK);
    assert!(response(revoke()).await.2);
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    assert!(
        admin
            .hub
            .accounts
            .revoke_key_requested(&owner.tenant, &owner.id, id, Some("revoke-worker"))
            .await
            .unwrap()
            .replayed
    );
    assert!(
        admin
            .hub
            .accounts
            .verify_key(secret)
            .await
            .unwrap()
            .is_none()
    );
    let (_, listed, _) =
        response(admin.request(Method::GET, "/api/v1/principals/worker/tokens")).await;
    assert!(listed["tokens"][0]["revoked_at"].is_number());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn principal_configuration_refuses_in_use_roles_and_preserves_deleted_receipts() {
    let admin = or_skip!(Admin::new());
    let role = json!({"id":"reviewers","description":"Review","version":0});
    let (status, made, _) =
        response(admin.request(Method::POST, "/api/v1/roles").json(&role)).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let mut agent = profile("reviewer");
    agent["roles"] = json!(["reviewers"]);
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/principals")
                .json(&agent)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        response(
            admin
                .request(Method::DELETE, "/api/v1/roles/reviewers")
                .header("if-match", "1")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    agent["roles"] = json!(["worker"]);
    agent["version"] = 1.into();
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/principals/reviewer")
                .json(&agent)
        )
        .await
        .0,
        StatusCode::OK
    );
    let delete = || {
        admin
            .request(Method::DELETE, "/api/v1/roles/reviewers")
            .header("if-match", "1")
            .header("idempotency-key", "delete-role")
    };
    assert_eq!(response(delete()).await.0, StatusCode::OK);
    assert!(response(delete()).await.2);
    let rule = json!({"id":"read-only-test","name":"Read","subject":"role:worker","action":"task.read","selector":{},"effect":"allow","priority":40,"enabled":true,"version":0});
    let (status, value, _) =
        response(admin.request(Method::POST, "/api/v1/policies").json(&rule)).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(
        response(admin.request(Method::DELETE, "/api/v1/policies/read-only-test"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(
            admin
                .request(Method::DELETE, "/api/v1/policies/read-only-test")
                .header("if-match", "1")
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, config, _) =
        response(admin.request(Method::GET, "/api/v1/principals/configuration")).await;
    let mut settings = config["settings"].clone();
    settings["wip_limit_default"] = 3.into();
    let (status, result, _) = response(
        admin
            .request(Method::PUT, "/api/v1/settings/principals")
            .json(&settings),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["result"]["wip_limit_default"], 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn environment_roster_has_account_qualified_cursors_and_live_member_boundaries() {
    let admin = or_skip!(Admin::new());
    let a = admin
        .hub
        .accounts
        .create_local_user(
            &admin.owner,
            "A",
            "roster-a@example.test",
            "A good local password",
        )
        .await
        .unwrap()
        .account;
    let b = admin
        .hub
        .accounts
        .create_local_user(
            &admin.owner,
            "B",
            "roster-b@example.test",
            "A good local password",
        )
        .await
        .unwrap()
        .account;
    for account in [&a.id, &b.id] {
        assert_eq!(
            response(
                admin
                    .request(
                        Method::POST,
                        &format!("/api/v1/principals?account={account}")
                    )
                    .json(&profile("same"))
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let (_, roster, _) =
        response(admin.request(Method::GET, "/api/v1/principals?role=worker")).await;
    let people = roster["principals"].as_array().unwrap();
    assert_eq!(people.len(), 2);
    let first = people[0]["account"].as_str().unwrap();
    let (_, next, _) = response(admin.request(
        Method::GET,
        &format!("/api/v1/principals?after={first}/same"),
    ))
    .await;
    assert_eq!(next["principals"].as_array().unwrap().len(), 1);
    assert_ne!(next["principals"][0]["account"], first);
    let session = admin.hub.accounts.open_session(&a.id, None).await.unwrap();
    let member = |method, path: &str| {
        admin
            .hub
            .http
            .request(method, format!("{}{path}", admin.hub.url))
            .header("cookie", format!("dg_session={session}"))
            .header("origin", &admin.hub.url)
    };
    let (_, own, _) = response(member(Method::GET, "/api/v1/principals")).await;
    assert_eq!(own["principals"].as_array().unwrap().len(), 1);
    assert_eq!(own["principals"][0]["account"], a.id);
    assert_eq!(
        response(member(
            Method::GET,
            &format!("/api/v1/principals?account={}", b.id)
        ))
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(member(Method::GET, "/api/v1/principals/configuration"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(
            member(
                Method::POST,
                &format!("/api/v1/principals/same/tokens?account={}", b.id)
            )
            .json(&json!({"name":"Forbidden","scopes":["boards.read"],"hours":24}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let page = admin
        .request(Method::GET, "/app/agents")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for account in [&a.id, &b.id] {
        assert!(page.contains(&format!("/app/agents/same?account={account}")));
    }
}
