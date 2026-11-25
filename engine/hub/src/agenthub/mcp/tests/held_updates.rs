//! The real MCP handler must forward both remembered and explicit update fences.

use crate::tenancy::tests::tenants::{hosted_with, refused};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_update_mcp_preserves_fences_and_progress() {
    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut person = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut person, "board_bind", json!({"project":"edits"}))
        .await;
    assert!(!refused(&bound), "{bound}");
    let made = hub
        .call(
            &mut person,
            "board_create",
            json!({"project":"edits", "title":"work"}),
        )
        .await;
    let id = made["structuredContent"]["id"].as_str().unwrap();
    let claimed = hub
        .call(
            &mut person,
            "board_claim",
            json!({"project":"edits", "id":id}),
        )
        .await;
    assert!(!refused(&claimed), "{claimed}");
    assert_eq!(claimed["structuredContent"]["claimed"], true, "{claimed}");
    let lease = &claimed["structuredContent"]["lease"];
    let version = &lease["ticket"]["version"];
    let fence = lease["fence"].as_i64().unwrap();
    let rejected = hub
        .call(
            &mut person,
            "board_update",
            json!({
                "project":"edits", "id":id, "version":version,
                "fence":fence - 1, "title":"late overwrite"
            }),
        )
        .await;
    assert!(refused(&rejected), "{rejected}");
    assert_eq!(rejected["structuredContent"]["code"], "stale_fence");
    let changed = hub
        .call(
            &mut person,
            "board_update",
            json!({
                "project":"edits", "id":id, "version":version,
                "title":"current worker", "progress":{"step":3, "detail":"<script>untrusted</script>"}
            }),
        )
        .await;
    assert!(!refused(&changed), "{changed}");
    let ticket = &changed["structuredContent"];
    assert_eq!(ticket["title"], "current worker");
    assert_eq!(ticket["progress"]["step"], 3);
    let page = hub
        .get(&format!("/app/tickets/edits/{id}"), Some(&person.cookie))
        .await;
    assert_eq!(page.status(), 200);
    let html = page.text().await.unwrap();
    assert!(html.contains("id=\"progress\"") && html.contains("Latest progress"));
    assert!(html.contains("&lt;script&gt;untrusted&lt;/script&gt;"));
    assert!(!html.contains("<script>untrusted</script>"));
    assert_eq!(ticket["fence"], fence);
    assert_eq!(ticket["lease_expires_at"], lease["lease_expires_at"]);
    assert_eq!(
        ticket["version"].as_i64(),
        Some(version.as_i64().unwrap() + 1)
    );
    let unchanged = hub
        .call(
            &mut person,
            "board_update",
            json!({
                "project":"edits", "id":id, "version":ticket["version"], "title":"keep progress"
            }),
        )
        .await;
    assert!(!refused(&unchanged), "{unchanged}");
    assert_eq!(unchanged["structuredContent"]["progress"]["step"], 3);
    let cleared = hub
        .call(
            &mut person,
            "board_update",
            json!({
                "project":"edits", "id":id,
                "version":unchanged["structuredContent"]["version"], "progress":null
            }),
        )
        .await;
    assert!(!refused(&cleared), "{cleared}");
    assert!(
        cleared["structuredContent"]["progress"].is_null(),
        "{cleared}"
    );
}
