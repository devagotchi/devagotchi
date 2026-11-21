use super::principals::Admin;
use super::*;

pub(super) async fn create(admin: &Admin, kind: &str, id: &str, mut fields: Value) -> Value {
    fields["id"] = id.into();
    fields["version"] = 0.into();
    let (status, value, _) = response(
        admin
            .request(Method::POST, &format!("/api/v1/{kind}"))
            .json(&fields),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{kind}: {value}");
    value["result"].clone()
}
async fn patch(
    admin: &Admin,
    kind: &str,
    id: &str,
    version: i64,
    mut fields: Value,
) -> (StatusCode, Value, bool) {
    fields["version"] = version.into();
    response(
        admin
            .request(Method::PATCH, &format!("/api/v1/{kind}/{id}"))
            .json(&fields),
    )
    .await
}
async fn remove(admin: &Admin, kind: &str, id: &str, version: i64) -> StatusCode {
    response(
        admin
            .request(Method::DELETE, &format!("/api/v1/{kind}/{id}"))
            .header("if-match", version.to_string()),
    )
    .await
    .0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_all_eight_types_seed_edit_replay_and_remove_through_http() {
    let admin = or_skip!(Admin::new());
    let examples = [
        (
            "capabilities",
            json!({"label":"Custom","sandbox_levels":["read-only"]}),
        ),
        (
            "risks",
            json!({"title":"Custom","plain_text":"Risk explained","severity":"low"}),
        ),
        (
            "capability-risks",
            json!({"capability_id":"fs.read","risk_id":"own.risk"}),
        ),
        (
            "shell-profiles",
            json!({"os":"any","label":"Custom","argv_prefix":["sh","-c"]}),
        ),
        (
            "grant-presets",
            json!({"label":"Custom","items":[{"capability":"fs.read","selector":{},"sandbox_level":"read-only"}]}),
        ),
        (
            "docker-runtime-presets",
            json!({"os":"linux","label":"Custom"}),
        ),
        (
            "router-endpoints",
            json!({"label":"Custom","url":"https://models.example.test/v1","kind":"direct"}),
        ),
        (
            "chat-apps",
            json!({"label":"Custom","provider_ids":["custom.provider"]}),
        ),
    ];
    create(
        &admin,
        "risks",
        "own.risk",
        json!({"title":"Own","plain_text":"Detail","severity":"low"}),
    )
    .await;
    for (kind, fields) in examples {
        let (status, seed, _) =
            response(admin.request(Method::GET, &format!("/api/v1/{kind}"))).await;
        assert_eq!(status, StatusCode::OK, "{seed}");
        assert!(!seed["rows"].as_array().unwrap().is_empty());
        let made = create(&admin, kind, "own.entry", fields.clone()).await;
        assert_eq!(made["version"], 1);
        let body = json!({"version":1, fields.as_object().unwrap().keys().next().unwrap(): fields.as_object().unwrap().values().next().unwrap()});
        let update = || {
            admin
                .request(Method::PATCH, &format!("/api/v1/{kind}/own.entry"))
                .header("idempotency-key", format!("update-{kind}"))
                .json(&body)
        };
        let (status, updated, replayed) = response(update()).await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert!(!replayed);
        assert_eq!(updated["result"]["version"], 2);
        let (_, again, replayed) = response(update()).await;
        assert!(replayed);
        assert_eq!(again["result"], updated["result"]);
        assert_eq!(
            response(
                admin
                    .request(Method::PATCH, &format!("/api/v1/{kind}/own.entry"))
                    .json(&body)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let (_, detail, _) =
            response(admin.request(Method::GET, &format!("/api/v1/{kind}/own.entry"))).await;
        assert_eq!(detail, updated["result"]);
        let page = admin
            .request(
                Method::GET,
                &format!("/app/catalog/{kind}/records/own.entry"),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        assert!(page.text().await.unwrap().contains("value=\"2\""));
        assert_eq!(remove(&admin, kind, "own.entry", 2).await, StatusCode::OK);
        assert_eq!(
            response(admin.request(Method::GET, &format!("/api/v1/{kind}/own.entry")))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        // A permanent receipt describes the committed operation even after removal.
        assert!(response(update()).await.2);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_merged_validation_and_reference_guards_preserve_existing_rows() {
    let admin = or_skip!(Admin::new());
    let path = "/api/v1/grant-presets/experimental-full-access";
    let (_, before, _) = response(admin.request(Method::GET, path)).await;
    let denied = patch(
        &admin,
        "grant-presets",
        "experimental-full-access",
        1,
        json!({"requires_policy":null}),
    )
    .await;
    assert_eq!(denied.0, StatusCode::BAD_REQUEST, "{}", denied.1);
    assert_eq!(response(admin.request(Method::GET, path)).await.1, before);
    assert_eq!(
        response(
            admin
                .request(Method::POST, &format!("{path}/default"))
                .json(&json!({"version":1}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        patch(
            &admin,
            "grant-presets",
            "task-scoped",
            1,
            json!({"full_access":true,"requires_policy":"unsandboxed"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        remove(&admin, "capabilities", "fs.read", 1).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        remove(&admin, "grant-presets", "task-scoped", 1).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        patch(
            &admin,
            "capabilities",
            "fs.read",
            1,
            json!({"sandbox_levels":["unsandboxed"]})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    for invalid in [
        json!({"is_default":true}),
        json!({"tenant_id":"other"}),
        json!({"label":3}),
        json!({"default_duration":"0h"}),
        json!({"sandbox_levels":["desktop"]}),
    ] {
        assert_eq!(
            patch(&admin, "capabilities", "fs.read", 1, invalid).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    for schema in [
        json!({"type":"wrong"}),
        json!({"$ref":"https://127.0.0.1:9/schema"}),
        json!({"$ref":"file:///private/schema.json"}),
    ] {
        assert_eq!(
            patch(
                &admin,
                "capabilities",
                "fs.read",
                1,
                json!({"selector_schema":schema})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let invalid=patch(&admin,"grant-presets","task-scoped",1,json!({"items":[{"capability":"fs.read","selector":{"paths":4},"sandbox_level":"read-only"}]})).await;
    assert_eq!(invalid.0, StatusCode::BAD_REQUEST, "{}", invalid.1);
    create(
        &admin,
        "shell-profiles",
        "own.shell",
        json!({"label":"Own","os":"linux"}),
    )
    .await;
    create(&admin,"grant-presets","own.preset",json!({"label":"Own","items":[{"capability":"exec","selector":{"shell_profile":"own.shell"},"sandbox_level":"read-only"}]})).await;
    assert_eq!(
        remove(&admin, "shell-profiles", "own.shell", 1).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        remove(&admin, "grant-presets", "own.preset", 1).await,
        StatusCode::OK
    );
    assert_eq!(
        remove(&admin, "shell-profiles", "own.shell", 1).await,
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_concurrent_writers_defaults_and_seed_reopen_preserve_operator_changes() {
    let admin = or_skip!(Admin::new());
    let (left, right) = tokio::join!(
        patch(
            &admin,
            "capabilities",
            "fs.read",
            1,
            json!({"label":"First"})
        ),
        patch(
            &admin,
            "capabilities",
            "fs.read",
            1,
            json!({"label":"Second"})
        )
    );
    assert_eq!(
        [left.0, right.0]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count(),
        1,
        "{left:?} {right:?}"
    );
    assert_eq!(
        [left.0, right.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    let choose = |id: &str| {
        admin
            .request(Method::POST, &format!("/api/v1/grant-presets/{id}/default"))
            .json(&json!({"version":1}))
    };
    let (a, b) = tokio::join!(
        response(choose("read-only-look")),
        response(choose("docker-setup"))
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(b.0, StatusCode::OK, "{}", b.1);
    let (_, list, _) = response(admin.request(Method::GET, "/api/v1/grant-presets")).await;
    assert_eq!(
        list["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["is_default"] == true)
            .count(),
        1
    );
    assert_eq!(
        remove(&admin, "grant-presets", "task-scoped", 2).await,
        StatusCode::OK
    );
    // A new owner instance uses the same persisted per-file seed marks.
    let catalog = crate::catalog::Catalog::new(admin.hub.accounts.clone());
    assert_eq!(
        catalog
            .get(
                &admin.owner,
                crate::catalog::Kind::GrantPresets,
                "task-scoped"
            )
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    let edited = catalog
        .get(&admin.owner, crate::catalog::Kind::Capabilities, "fs.read")
        .await
        .unwrap();
    assert_eq!(edited.version, 2);
    assert!(matches!(
        edited.data["label"].as_str(),
        Some("First" | "Second")
    ));
}
