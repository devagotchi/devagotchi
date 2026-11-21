//! Project lifecycle through the real server, including stale board handles and forms.
use super::{principals::Admin, *};
use crate::identity::accounts::api_keys::{NewKey, Scope};

fn draft(id: &str) -> Value {
    json!({"id":id,"name":"<Delivery>","version":0})
}
async fn create(admin: &Admin, id: &str) -> Value {
    let (status, value, _) = response(
        admin
            .request(Method::POST, "/api/v1/projects")
            .json(&draft(id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value["result"].clone()
}
async fn remove(admin: &Admin, id: &str, version: i64, key: &str) -> (StatusCode, Value, bool) {
    response(
        admin
            .request(Method::DELETE, &format!("/api/v1/projects/{id}"))
            .header("if-match", version.to_string())
            .header("idempotency-key", key),
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_lifecycle_replays_retires_and_recreates_without_reviving_old_handles() {
    let admin = or_skip!(Admin::new());
    let request = || {
        admin
            .request(Method::POST, "/api/v1/projects")
            .header("idempotency-key", "create-project")
            .json(&draft("delivery"))
    };
    let (one, two) = tokio::join!(response(request()), response(request()));
    assert_eq!(one.0, StatusCode::OK, "{:?}", one.1);
    assert_eq!(two.0, StatusCode::OK, "{:?}", two.1);
    assert_ne!(one.2, two.2);
    assert_eq!(one.1["result"], two.1["result"]);
    let first = one.1["result"].clone();
    assert_eq!(first["version"], 1);
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let subject = admin.hub.accounts.subject(&owner, "browser");
    let (old, _, _) = admin.hub.hub.board_access(&subject, "delivery").unwrap();
    let (_, events, _) =
        response(admin.request(Method::GET, "/api/v1/events?project=delivery")).await;
    assert_eq!(events["events"].as_array().unwrap().len(), 1);
    assert_eq!(events["events"][0]["action"], "project.saved");
    let edit = json!({"id":"delivery","name":"Renamed","version":1});
    let (status, saved, _) = response(
        admin
            .request(Method::PUT, "/api/v1/projects/delivery")
            .header("idempotency-key", "rename")
            .json(&edit),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["result"]["version"], 2);
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/projects/delivery")
                .json(&edit)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        remove(&admin, "delivery", 1, "stale-delete").await.0,
        StatusCode::CONFLICT
    );
    let (status, removed, replayed) = remove(&admin, "delivery", 2, "delete-project").await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert!(!replayed);
    assert!(removed["result"]["retired_at"].is_number());
    assert!(remove(&admin, "delivery", 2, "delete-project").await.2);
    assert_eq!(
        old.hosted().database_now().await.unwrap_err().code,
        "not_found"
    );
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/projects/delivery"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, active, _) = response(admin.request(Method::GET, "/api/v1/projects")).await;
    assert_eq!(active["projects"], json!([]));
    let (_, retired, _) =
        response(admin.request(Method::GET, "/api/v1/projects?retired=true")).await;
    assert_eq!(retired["projects"].as_array().unwrap().len(), 1);
    let history_path = format!(
        "/api/v1/projects/archive/{}/events",
        first["board_id"].as_str().unwrap()
    );
    let (status, history, _) = response(admin.request(Method::GET, &history_path)).await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(history["events"].as_array().unwrap().len(), 3);
    assert_eq!(history["events"][2]["action"], "project.retired");
    // Old creation identity cannot allocate a new generation, and rolls back its tentative insert.
    assert_eq!(response(request()).await.0, StatusCode::CONFLICT);
    let second = create(&admin, "delivery").await;
    assert_ne!(second["board_id"], first["board_id"]);
    assert_eq!(second["version"], 1);
    assert_eq!(
        remove(&admin, "delivery", 2, "delete-project").await.0,
        StatusCode::CONFLICT
    );
    let (new, _, _) = admin.hub.hub.board_access(&subject, "delivery").unwrap();
    assert_ne!(
        new.hosted().project_record().await.unwrap().board_id,
        first["board_id"].as_str().unwrap()
    );
    assert_eq!(old.hosted().events().await.unwrap_err().code, "not_found");
    let (_, preserved, _) = response(admin.request(Method::GET, &history_path)).await;
    assert_eq!(preserved, history);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_creation_is_atomic_with_workflow_and_shared_edits_invalidate_metadata_version() {
    let admin = or_skip!(Admin::new());
    create(&admin, "source").await;
    let mut workflow = crate::agenthub::Workflow::default_workflow();
    workflow.id = "delivery".into();
    workflow.name = "Delivery".into();
    let (status, value, _) = response(
        admin
            .request(Method::POST, "/api/v1/workflows?project=source")
            .json(&json!({"version":0,"workflow":workflow})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    let mut input = draft("bound");
    input["workflow_id"] = "delivery".into();
    input["workflow_version"] = 2.into();
    assert_eq!(
        response(admin.request(Method::POST, "/api/v1/projects").json(&input))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/projects/bound"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    input["workflow_version"] = 1.into();
    let (status, created, _) =
        response(admin.request(Method::POST, "/api/v1/projects").json(&input)).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["result"]["workflow_id"], "delivery");
    assert_eq!(created["result"]["version"], 1);
    assert_eq!(
        remove(&admin, "source", 1, "anchor").await.0,
        StatusCode::CONFLICT
    );
    workflow.name = "Updated".into();
    let (status, value, _) = response(
        admin
            .request(Method::PUT, "/api/v1/workflows/delivery?project=source")
            .json(&json!({"version":1,"workflow":workflow})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    input["version"] = 1.into();
    assert_eq!(
        response(
            admin
                .request(Method::PUT, "/api/v1/projects/bound")
                .json(&input)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, current, _) = response(admin.request(Method::GET, "/api/v1/projects/bound")).await;
    assert_eq!(current["version"], 2);
    let (status, value, _) = remove(&admin, "bound", 2, "bound-remove").await;
    assert_eq!(status, StatusCode::OK, "{value}");
    let (_, catalog, _) =
        response(admin.request(Method::GET, "/api/v1/workflows?project=source")).await;
    assert_eq!(catalog[0]["used_by"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_removal_refuses_completed_tasks_and_live_project_keys() {
    let admin = or_skip!(Admin::new());
    create(&admin, "work").await;
    let (_, task, _) = response(
        admin
            .request(Method::POST, "/api/v1/tasks")
            .json(&json!({"project":"work","title":"Finish"})),
    )
    .await;
    let id = task["id"].as_str().unwrap();
    let (_, claim, _) = response(
        admin
            .request(
                Method::POST,
                &format!("/api/v1/tasks/{id}/claim?project=work"),
            )
            .json(&json!({})),
    )
    .await;
    let (status, value, _) = response(
        admin
            .request(
                Method::POST,
                &format!("/api/v1/tasks/{id}/complete?project=work"),
            )
            .json(&json!({"fence":claim["fence"],"summary":"Done"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    let (status, value, _) = remove(&admin, "work", 1, "not-empty").await;
    assert_eq!(status, StatusCode::CONFLICT, "{value}");
    create(&admin, "restricted").await;
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (key, secret) = admin
        .hub
        .accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &NewKey {
                name: "Scoped".into(),
                scopes: Scope::ALL.to_vec(),
                projects: vec!["restricted".into()],
                hours: 24,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let (status, value, _) = remove(&admin, "restricted", 1, "key-guard").await;
    assert_eq!(status, StatusCode::CONFLICT, "{value}");
    assert_eq!(
        response(
            admin
                .hub
                .http
                .get(format!("{}/api/v1/projects", admin.hub.url))
                .bearer_auth(&secret)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    admin
        .hub
        .accounts
        .revoke_key(&owner.tenant, &owner.id, &key.id)
        .await
        .unwrap();
    let (status, value, _) = remove(&admin, "restricted", 1, "key-guard").await;
    assert_eq!(status, StatusCode::OK, "{value}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_browser_forms_share_receipts_preserve_drafts_and_enforce_origin() {
    let admin = or_skip!(Admin::new());
    let page = admin
        .request(Method::GET, "/app/projects/new")
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let body = page.text().await.unwrap();
    assert!(body.contains("name=\"request_id\""));
    assert!(body.contains("Project name"));
    let fields = [
        ("request_id", "browser-project"),
        ("id", "browser"),
        ("name", "<Delivery>"),
        ("version", "0"),
        ("workflow", ""),
    ];
    let made = admin
        .request(Method::POST, "/app/projects/new")
        .form(&fields)
        .send()
        .await
        .unwrap();
    assert_eq!(made.status(), StatusCode::SEE_OTHER);
    let (status, again, replayed) = response(
        admin
            .request(Method::POST, "/api/v1/projects")
            .header("idempotency-key", "browser-project")
            .json(&draft("browser")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert!(replayed);
    let page = admin
        .request(Method::GET, "/app/projects/browser")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("&lt;Delivery&gt;"));
    assert!(page.contains("/app/projects/browser/remove"));
    let edit = [
        ("request_id", "browser-edit"),
        ("id", "browser"),
        ("name", "Kept draft"),
        ("version", "9"),
        ("workflow", ""),
    ];
    let stale = admin
        .request(Method::POST, "/app/projects/browser")
        .form(&edit)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let body = stale.text().await.unwrap();
    assert!(body.contains("Kept draft"));
    assert!(body.contains("browser-edit"));
    let foreign = admin
        .hub
        .http
        .post(format!("{}/app/projects/browser/remove", admin.hub.url))
        .header("cookie", &admin.cookie)
        .header("origin", "https://foreign.example")
        .form(&[("request_id", "remove"), ("version", "1")])
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    let removed = admin
        .request(Method::POST, "/app/projects/browser/remove")
        .form(&[("request_id", "browser-remove"), ("version", "1")])
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::SEE_OTHER);
    let location = removed.headers().get("location").unwrap().to_str().unwrap();
    let history = admin.request(Method::GET, location).send().await.unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    assert!(history.text().await.unwrap().contains("project.retired"));
    assert!(remove(&admin, "browser", 1, "browser-remove").await.2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_owner_namespaces_and_retired_history_are_not_shared_between_people() {
    let admin = or_skip!(Admin::new());
    let first = create(&admin, "private").await;
    let other = admin
        .hub
        .accounts
        .create_local_user(
            &admin.owner,
            "Other",
            "project-other@example.test",
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
    let request = |method, path: &str| {
        admin
            .hub
            .http
            .request(method, format!("{}{path}", admin.hub.url))
            .header("cookie", format!("dg_session={session}"))
            .header("origin", &admin.hub.url)
    };
    assert_eq!(
        response(request(Method::GET, "/api/v1/projects/private"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, other_project, _) =
        response(request(Method::POST, "/api/v1/projects").json(&draft("private"))).await;
    assert_eq!(status, StatusCode::OK, "{other_project}");
    assert_ne!(other_project["result"]["board_id"], first["board_id"]);
    assert_eq!(
        remove(&admin, "private", 1, "retire-private").await.0,
        StatusCode::OK
    );
    let path = format!(
        "/api/v1/projects/archive/{}/events",
        first["board_id"].as_str().unwrap()
    );
    assert_eq!(
        response(request(Method::GET, &path)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        response(
            admin
                .request(Method::GET, "/api/v1/projects")
                .header(crate::agenthub::mcp::SEAT_HEADER, "seat")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
