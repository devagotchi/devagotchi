//! Browser creation and MCP observe the same PostgreSQL board and permissions.
use crate::tenancy::tests::tenants::{Hosted, Person, refused};
use crate::web::app::tests::{hosted, page};
use serde_json::json;

async fn task(hub: &Hosted, person: &Person, fields: &[(&str, &str)]) -> String {
    let response = hub
        .form("/app/boards/new/new", Some(&person.cookie), fields)
        .await;
    let status = response.status();
    let location = response
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    assert_eq!(status, 303, "{}", response.text().await.unwrap());
    location.unwrap().rsplit('/').next().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_creation_is_atomic_and_visible_through_mcp() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    let bob = hub.first_connect("Codex").await;
    assert_eq!(hub.get("/app/new-board", None).await.status(), 303);
    let (_, boards) = page(&hub, "/app/boards", &alice).await;
    assert!(boards.contains("/app/new-board"));
    assert_eq!(page(&hub, "/app/new-board", &alice).await.0, 200);
    // "new" is a valid existing project name, not a reserved form route.
    let response = hub
        .form("/app/new-board", Some(&alice.cookie), &[("project", "new")])
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers()["location"], "/app/boards/new");
    let (_, board) = page(&hub, "/app/boards/new", &alice).await;
    assert!(board.contains("/app/boards/new/new"));
    assert_eq!(page(&hub, "/app/boards/new/new", &alice).await.0, 200);
    let first = task(&hub, &alice, &[("title", "First")]).await;
    let second = task(
        &hub,
        &alice,
        &[
            ("title", "Second <task>"),
            ("description", "Keep <details>"),
            ("priority", "1"),
            ("labels", "ops, release"),
            ("depends_on", &first),
        ],
    )
    .await;
    let got = hub
        .call(
            &mut alice,
            "board_get",
            json!({"project":"new", "id":second}),
        )
        .await;
    assert!(!refused(&got), "{got}");
    let ticket = &got["structuredContent"]["ticket"];
    assert_eq!(ticket["title"], "Second <task>");
    assert_eq!(ticket["description"], "Keep <details>");
    assert_eq!(ticket["priority"], 1);
    assert_eq!(ticket["labels"], json!(["ops", "release"]));
    assert_eq!(ticket["blocked_by"], json!([first]));
    let (_, shown) = page(&hub, &format!("/app/tickets/new/{second}"), &alice).await;
    assert!(shown.contains("Second &lt;task&gt;") && shown.contains("Keep &lt;details&gt;"));
    assert!(
        shown.contains(&format!("href=\"/app/tickets/new/{first}\"")) && shown.contains("Waiting")
    );
    let third = hub
        .call(
            &mut alice,
            "board_create",
            json!({"project":"new", "title":"From MCP", "depends_on":[second]}),
        )
        .await;
    assert!(!refused(&third), "{third}");
    assert_eq!(third["structuredContent"]["blocked_by"], json!([second]));
    let before = hub
        .call(&mut alice, "board_events", json!({"project":"new"}))
        .await;
    let response = hub
        .form(
            "/app/boards/new/new",
            Some(&alice.cookie),
            &[
                ("title", "Retain my draft"),
                ("description", "Keep this too"),
                ("depends_on", "missing-ticket"),
            ],
        )
        .await;
    let (status, html) = Hosted::text(response).await;
    assert_eq!(status, 404, "{html}");
    assert!(html.contains("Retain my draft") && html.contains("Keep this too"));
    let after = hub
        .call(&mut alice, "board_events", json!({"project":"new"}))
        .await;
    assert_eq!(before["structuredContent"], after["structuredContent"]);
    assert_eq!(page(&hub, "/app/boards/new/new", &bob).await.0, 404);
    assert_eq!(
        hub.form(
            "/app/boards/new/new",
            Some(&bob.cookie),
            &[("title", "Forbidden")]
        )
        .await
        .status(),
        404
    );
    // Real storage refusal must not look like a created ticket or an empty board.
    sqlx::query("REVOKE INSERT ON devagotchi.board_tickets FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    let denied = hub
        .form(
            "/app/boards/new/new",
            Some(&alice.cookie),
            &[("title", "Storage denied")],
        )
        .await;
    assert_eq!(denied.status(), 503, "{}", denied.text().await.unwrap());
    sqlx::query("GRANT INSERT ON devagotchi.board_tickets TO devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    let after = hub
        .call(&mut alice, "board_events", json!({"project":"new"}))
        .await;
    assert_eq!(before["structuredContent"], after["structuredContent"]);
    assert_eq!(
        hub.form(
            &format!("/app/tickets/new/{first}/close"),
            Some(&alice.cookie),
            &[("version", "1")]
        )
        .await
        .status(),
        303
    );
    let (_, unblocked) = page(&hub, &format!("/app/tickets/new/{second}"), &alice).await;
    assert!(unblocked.contains("No longer blocking") && !unblocked.contains(">Waiting<"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creation_requires_the_authenticated_hubs_origin() {
    let hub = or_skip!(hosted());
    let alice = hub.first_connect("Claude").await;
    assert_eq!(
        hub.form("/app/new-board", None, &[("project", "unsigned")])
            .await
            .status(),
        303
    );
    for (origin, fetch) in [
        (None, None),
        (Some("https://elsewhere.example"), None),
        (Some("https://elsewhere.example"), Some("same-origin")),
        (Some(hub.url.as_str()), Some("cross-site")),
    ] {
        for path in ["/app/new-board", "/app/boards/absent/new"] {
            let mut request = hub
                .http
                .post(format!("{}{path}", hub.url))
                .header("cookie", &alice.cookie)
                .form(&[("project", "forbidden"), ("title", "forbidden")]);
            if let Some(value) = origin {
                request = request.header("origin", value);
            }
            if let Some(value) = fetch {
                request = request.header("sec-fetch-site", value);
            }
            assert_eq!(request.send().await.unwrap().status(), 403);
        }
    }
    assert_eq!(page(&hub, "/app/boards/forbidden", &alice).await.0, 404);
}
