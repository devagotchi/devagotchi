use super::principals::Admin;
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_default_http_and_browser_require_typed_confirmation_and_preserve_drafts() {
    let admin = or_skip!(Admin::new());
    let id = "experimental-full-access";
    let path = format!("/api/v1/grant-presets/{id}/default");
    let (status, failure, _) = response(
        admin
            .request(Method::POST, &path)
            .json(&json!({"version":1})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{failure}");
    assert_eq!(failure["error"]["code"], "confirm_required");
    assert_eq!(
        response(
            admin
                .request(Method::POST, &path)
                .json(&json!({"version":1,"confirm_default":true}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let browser = format!("/app/catalog/grant-presets/records/{id}");
    let fields = [
        ("id", id),
        ("version", "1"),
        ("action", "default"),
        ("request_id", "browser-default"),
        ("confirm_default", "<wrong>"),
    ];
    let refused = admin
        .request(Method::POST, &browser)
        .form(&fields)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = refused.text().await.unwrap();
    assert!(
        body.contains("name=\"confirm_default\" value=\"&lt;wrong&gt;\""),
        "{body}"
    );
    assert!(body.contains("environment-wide unsandboxed policy"));
    assert_eq!(
        response(
            admin
                .request(Method::POST, &path)
                .json(&json!({"version":1,"confirm_default":id}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let access = crate::machines::access::Access::new(admin.hub.accounts.clone());
    let settings = access.settings(&admin.owner).await.unwrap();
    let (_, risks, _) = response(admin.request(Method::GET, "/api/v1/risks")).await;
    let acknowledged: Vec<Value> = settings
        .unsandboxed_risk_ids
        .iter()
        .map(|id| {
            let risk = risks["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == *id)
                .unwrap();
            json!({"id":id,"version":risk["version"]})
        })
        .collect();
    let edit = serde_json::from_value(
        json!({"version":0,"allowed":true,"acknowledged":acknowledged,"expires_at":null}),
    )
    .unwrap();
    access
        .set_policy(&admin.owner, None, &edit, "enable-default-policy")
        .await
        .unwrap();
    let send = || {
        admin
            .request(Method::POST, &path)
            .header("idempotency-key", "http-default")
            .json(&json!({"version":1,"confirm_default":id}))
    };
    let (status, selected, _) = response(send()).await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    assert_eq!(selected["result"]["is_default"], true);
    assert!(response(send()).await.2);

    // Editing the selected preset uses the same typed confirmation control.
    let record = format!("/api/v1/grant-presets/{id}");
    assert_eq!(
        response(
            admin
                .request(Method::PATCH, &record)
                .json(&json!({"version":2,"label":"Confirmed default"}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, edited, _) = response(
        admin
            .request(Method::PATCH, &record)
            .json(&json!({"version":2,"label":"Confirmed default","confirm_default":id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let fields = [
        ("id", id),
        ("version", "3"),
        ("action", "default"),
        ("request_id", "browser-default"),
        ("confirm_default", id),
    ];
    assert_eq!(
        admin
            .request(Method::POST, &browser)
            .form(&fields)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let page = admin
        .request(Method::GET, &browser)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("name=\"confirm_default\" value=\"\""),
        "confirmation is never prefilled"
    );
}
