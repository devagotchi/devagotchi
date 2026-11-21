use super::principals::Admin;
use super::*;
use crate::agenthub::{SettingsEdit, Workflow};

async fn configured() -> Option<Admin> {
    let admin = Admin::new().await?;
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let subject = admin.hub.accounts.subject(&owner, "browser");
    for project in ["alpha", "beta"] {
        admin.hub.hub.bind_project(&subject, project).await.unwrap();
    }
    Some(admin)
}
async fn configuration(admin: &Admin, project: &str) -> Value {
    let (status, value, _) = response(admin.request(
        Method::GET,
        &format!("/api/v1/projects/{project}/configuration"),
    ))
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
fn definition() -> Workflow {
    let mut workflow = Workflow::default_workflow();
    workflow.id = "delivery".into();
    workflow.name = "Delivery".into();
    workflow
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_workflow_creation_replays_without_binding_or_duplicate_events() {
    let admin = or_skip!(configured());
    let before = configuration(&admin, "alpha").await;
    let create = || {
        admin
            .request(Method::POST, "/api/v1/workflows?project=alpha")
            .header("idempotency-key", "independent-workflow")
            .json(&json!({"version":0,"workflow":definition()}))
    };
    let (one, two) = tokio::join!(response(create()), response(create()));
    assert_eq!(one.0, StatusCode::OK, "{:?}", one.1);
    assert_eq!(two.0, StatusCode::OK, "{:?}", two.1);
    assert_ne!(one.2, two.2);
    assert_eq!(one.1["version"], 1);
    assert_eq!(one.1["used_by"], json!([]));
    assert_eq!(configuration(&admin, "alpha").await, before);
    for project in ["alpha", "beta"] {
        let (_, list, _) =
            response(admin.request(Method::GET, &format!("/api/v1/workflows?project={project}")))
                .await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["workflow"]["id"], "delivery");
    }
    let (_, events, _) = response(admin.request(Method::GET, "/api/v1/events?project=alpha")).await;
    assert_eq!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["action"] == "workflow.published")
            .count(),
        1
    );
    let mut changed = definition();
    changed.name = "Changed".into();
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/workflows?project=alpha")
                .header("idempotency-key", "independent-workflow")
                .json(&json!({"version":0,"workflow":changed}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_workflow_http_updates_all_bound_boards_and_deletes_only_when_unused() {
    let admin = or_skip!(configured());
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/workflows?project=alpha")
                .json(&json!({"version":0,"workflow":definition()}))
        )
        .await
        .0,
        StatusCode::OK
    );
    for project in ["alpha", "beta"] {
        let (status, bound, _) = response(
            admin
                .request(
                    Method::POST,
                    &format!("/api/v1/projects/{project}/workflow/bind"),
                )
                .json(&json!({"version":1,"workflow_id":"delivery","workflow_version":1})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{bound}");
        assert_eq!(bound["workflow_version"], 2);
    }
    let mut changed = definition();
    changed.name = "Changed".into();
    let update = || {
        admin
            .request(Method::PUT, "/api/v1/workflows/delivery?project=alpha")
            .header("idempotency-key", "workflow-edit")
            .header("if-match", "1")
            .json(&json!({"version":1,"workflow":changed}))
    };
    let (status, edited, _) = response(update()).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["version"], 2);
    assert!(response(update()).await.2);
    assert_eq!(
        response(
            admin
                .request(Method::DELETE, "/api/v1/workflows/delivery?project=alpha")
                .header("if-match", "2")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    for project in ["alpha", "beta"] {
        let config = configuration(&admin, project).await;
        assert_eq!(config["configuration"]["workflow"]["name"], "Changed");
        assert_eq!(config["configuration"]["workflow_version"], 3);
        assert_eq!(
            response(
                admin
                    .request(Method::PUT, &format!("/api/v1/projects/{project}/workflow"))
                    .json(&json!({"version":3,"workflow":definition()}))
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            response(
                admin
                    .request(
                        Method::POST,
                        &format!("/api/v1/projects/{project}/workflow/detach")
                    )
                    .json(&json!({"version":3}))
            )
            .await
            .0,
            StatusCode::OK
        );
        let config = configuration(&admin, project).await;
        assert!(config["shared_workflow"].is_null());
        assert_eq!(config["configuration"]["workflow"]["name"], "Changed");
    }
    let delete = || {
        admin
            .request(Method::DELETE, "/api/v1/workflows/delivery?project=alpha")
            .header("if-match", "2")
            .header("idempotency-key", "workflow-delete")
    };
    assert_eq!(response(delete()).await.0, StatusCode::OK);
    assert!(response(delete()).await.2);
    let (_, list, _) = response(admin.request(Method::GET, "/api/v1/workflows?project=beta")).await;
    assert_eq!(list, json!([]));
    for project in ["alpha", "beta"] {
        assert_eq!(
            response(admin.request(
                Method::GET,
                &format!("/api/v1/events/verify?project={project}")
            ))
            .await
            .0,
            StatusCode::OK
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn versioned_configuration_refuses_bad_workflows_agents_and_cross_account_access() {
    use crate::identity::accounts::api_keys::{NewKey, Scope};
    let admin = or_skip!(configured());
    let config = configuration(&admin, "alpha").await;
    let settings: crate::agenthub::Settings =
        serde_json::from_value(config["configuration"]["settings"].clone()).unwrap();
    let mut edit = SettingsEdit::from(&settings);
    edit.default_priority = 7;
    let save = || {
        admin
            .request(Method::PUT, "/api/v1/projects/alpha/settings")
            .header("idempotency-key", "settings-edit")
            .json(&json!({"version":1,"settings":edit}))
    };
    let (status, saved, _) = response(save()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["settings_version"], 2);
    assert!(response(save()).await.2);
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/projects/alpha/settings")
                .json(&json!({"version":1,"settings":edit}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut bad = definition();
    bad.initial_state = "missing".into();
    assert_eq!(
        response(
            admin
                .request(Method::POST, "/api/v1/workflows?project=alpha")
                .json(&json!({"version":0,"workflow":bad}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/workflows/wrong?project=alpha")
                .json(&json!({"version":1,"workflow":definition()}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (_, secret) = admin
        .hub
        .accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &NewKey {
                name: "Agent".into(),
                scopes: Scope::ALL.to_vec(),
                hours: 24,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        response(
            admin
                .hub
                .http
                .get(format!("{}/api/v1/workflows?project=alpha", admin.hub.url))
                .bearer_auth(secret)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let other = admin
        .hub
        .accounts
        .create_local_user(
            &owner.id,
            "Other",
            "workflow-other@example.test",
            "A good local password",
        )
        .await
        .unwrap()
        .account;
    let session = admin
        .hub
        .accounts
        .open_session(&other.id, None)
        .await
        .unwrap();
    assert_eq!(
        response(
            admin
                .hub
                .http
                .get(format!("{}/api/v1/workflows?project=alpha", admin.hub.url))
                .header("cookie", format!("dg_session={session}"))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
