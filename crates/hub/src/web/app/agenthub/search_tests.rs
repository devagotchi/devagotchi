//! Browser search and MCP search read the same scoped board, including its notes.
use crate::tenancy::tests::tenants::refused;
use crate::web::app::tests::{hosted, page};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_and_mcp_find_task_ids_notes_and_words_without_crossing_accounts() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Codex").await;
    for person in [&mut alice, &mut bob] {
        let bound = hub
            .call(person, "board_bind", json!({"project":"search"}))
            .await;
        assert!(!refused(&bound), "{bound}");
    }
    let made = hub
        .call(
            &mut alice,
            "board_create",
            json!({
                "project":"search", "title":"Alpha implementation", "description":"Beta acceptance",
                "labels":["release"]
            }),
        )
        .await;
    assert!(!refused(&made), "{made}");
    let id = made["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = hub
        .form(
            &format!("/app/tickets/search/{id}/comment"),
            Some(&alice.cookie),
            &[("body", "Migration evidence recorded")],
        )
        .await;
    assert_eq!(response.status(), 303);
    for query in [&id, "migration evidence", "alpha beta", "plement"] {
        let path = format!("/app/boards/search?q={}", crate::web::encode(query));
        let (status, html) = page(&hub, &path, &alice).await;
        assert_eq!(status, 200, "{html}");
        assert!(
            html.contains(&format!("id=\"ticket-{id}\"")),
            "{query}: {html}"
        );
        assert_eq!(html.matches("<li id=\"ticket-").count(), 1);
        let found = hub
            .call(
                &mut alice,
                "board_list",
                json!({"project":"search","text":query}),
            )
            .await;
        assert!(!refused(&found), "{found}");
        let tickets = found["structuredContent"]["tickets"].as_array().unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0]["id"], id);
        let (_, foreign) = page(&hub, &path, &bob).await;
        assert!(!foreign.contains("Alpha implementation"));
        let absent = hub
            .call(
                &mut bob,
                "board_list",
                json!({"project":"search","text":query}),
            )
            .await;
        assert!(!refused(&absent), "{absent}");
        assert_eq!(absent["structuredContent"]["tickets"], json!([]));
    }
    let (_, filtered) = page(&hub, "/app/boards/search?q=migration&label=missing", &alice).await;
    assert!(filtered.contains("No tasks match."));
}
