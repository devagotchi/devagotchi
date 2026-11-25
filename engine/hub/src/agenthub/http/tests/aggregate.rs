use super::{principals::Admin, *};

async fn configured() -> Option<Admin> {
    let admin = Admin::new().await?;
    let (status, value, _) = response(
        admin
            .request(Method::POST, "/api/v1/projects")
            .json(&json!({"id":"demo","name":"Demo","version":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    Some(admin)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aggregate_configuration_reads_the_same_owners_and_fails_instead_of_partial_success() {
    let admin = or_skip!(configured());
    let (status, config, _) =
        response(admin.request(Method::GET, "/api/v1/config?project=demo")).await;
    assert_eq!(status, StatusCode::OK, "{config}");
    let (_, policy, _) =
        response(admin.request(Method::GET, "/api/v1/principals/configuration")).await;
    assert_eq!(config["roles"], policy["roles"]);
    assert_eq!(config["policies"], policy["rules"]);
    assert_eq!(config["settings"]["principals"], policy["settings"]);
    let (_, board, _) =
        response(admin.request(Method::GET, "/api/v1/projects/demo/configuration")).await;
    assert_eq!(config["settings"]["board"], board);
    let (_, projects, _) = response(admin.request(Method::GET, "/api/v1/projects")).await;
    assert_eq!(config["projects"], projects["projects"]);
    assert_eq!(config["next"], json!({"projects":null,"principals":null}));
    assert_eq!(config["workflows"], json!([]));
    assert_eq!(config["principals"], json!([]));
    assert_eq!(
        config["state_kinds"],
        json!(["queue", "active", "review", "blocked", "done", "attention"])
    );
    for path in [
        "/api/v1/config",
        "/api/v1/config?project=demo&principals_after=bad",
        "/api/v1/config?project=demo&projects_after=bad",
        "/api/v1/config?project=demo&unrecognized=1",
    ] {
        let (status, value, _) = response(admin.request(Method::GET, path)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {value}");
        assert!(value.get("settings").is_none());
    }
    assert_eq!(
        response(admin.request(Method::GET, "/api/v1/config?project=absent"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        response(
            admin
                .request(Method::GET, "/api/v1/config?project=demo")
                .header("idempotency-key", "read")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aggregate_configuration_requires_live_environment_owner_and_human_board_authority() {
    use crate::identity::accounts::api_keys::{NewKey, Scope};
    let admin = or_skip!(configured());
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
                .get(format!("{}/api/v1/config?project=demo", admin.hub.url))
                .bearer_auth(secret)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(
            admin
                .request(Method::GET, "/api/v1/config?project=demo")
                .header(crate::agenthub::mcp::SEAT_HEADER, "seat")
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
            "aggregate-other@example.test",
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
        response(
            request(Method::POST, "/api/v1/projects")
                .json(&json!({"id":"demo","name":"Own","version":0}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, value, _) = response(request(Method::GET, "/api/v1/config?project=demo")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{value}");
    assert!(value.get("principals").is_none());
}
