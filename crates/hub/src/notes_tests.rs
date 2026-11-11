//! A note left for an AI, end to end over HTTP, the whole router as it serves: a person leaves
//! it on a ticket's page, in a section, for `claude`; Claude's next tool call of any kind
//! carries it, once; the page shows it in its section with who it is for; marking it read
//! clears it. On SQLite, and on Postgres when the test server is set (CI).

use serde_json::{Value, json};

use crate::tenant_tests::{Hosted, Person, hosted_with};

fn extra_text(answer: &Value) -> String {
    answer["result"]["content"]
        .as_array()
        .or_else(|| answer["content"].as_array())
        .map(|content| {
            content
                .iter()
                .filter_map(|block| block["text"].as_str())
                .filter(|text| text.contains("LEFT FOR YOU") || text.contains("REMINDER"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

async fn list(hub: &Hosted, ai: &mut Person) -> Value {
    hub.call(ai, "board_list", json!({ "project": "site" }))
        .await
}

async fn run(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let mut claude = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut claude, "board_bind", json!({ "project": "site" }))
        .await;
    assert_eq!(bound["structuredContent"]["project"], "site", "{bound}");
    let made = hub
        .call(
            &mut claude,
            "board_create",
            json!({ "project": "site", "title": "Move the database" }),
        )
        .await;
    let id = made["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        extra_text(&list(&hub, &mut claude).await),
        "",
        "nothing yet"
    );

    // The person leaves a question for Claude from the ticket's page.
    let left = hub
        .http
        .post(format!("{}/app/tickets/site/{id}/comment", hub.url))
        .header(axum::http::header::COOKIE, &claude.cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(&[
            ("body", "Which region did you pick?"),
            ("kind", "question"),
            ("to", "claude"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(left.status(), 303);

    // Whatever Claude calls next carries it, complete.
    let answer = list(&hub, &mut claude).await;
    let heard = extra_text(&answer);
    assert!(heard.contains("Which region did you pick?"), "{answer}");
    assert!(heard.contains(&format!("[site {id}]")), "{heard}");
    assert!(heard.contains("question"), "{heard}");
    assert!(
        answer["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Move the database"),
        "the tool's own answer comes first and is unchanged: {answer}"
    );

    // The data moved once.
    assert_eq!(extra_text(&list(&hub, &mut claude).await), "");

    // It stays readable, and marking it read clears it.
    let inbox = hub
        .call(&mut claude, "board_inbox", json!({ "project": "site" }))
        .await;
    let notes = inbox["structuredContent"]["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1, "{inbox}");
    assert_eq!(notes[0]["kind"], "question");
    let seq = notes[0]["seq"].as_i64().unwrap();
    let cleared = hub
        .call(
            &mut claude,
            "board_inbox",
            json!({ "project": "site", "ack_through": seq }),
        )
        .await;
    assert_eq!(
        cleared["structuredContent"]["notes"],
        json!([]),
        "{cleared}"
    );

    // On the phone: its section, and who it is for.
    let (status, page) = Hosted::text(
        hub.get(&format!("/app/tickets/site/{id}"), Some(&claude.cookie))
            .await,
    )
    .await;
    assert_eq!(status, 200);
    assert!(page.contains("id=\"notes-question\""), "{page}");
    assert!(page.contains("for claude"), "{page}");
    assert!(page.contains("id=\"note-to\""), "{page}");

    // A note an AI leaves for a name nobody here answers to reaches nobody here.
    hub.call(
        &mut claude,
        "board_note",
        json!({ "project": "site", "id": id, "body": "for later", "to": "grok" }),
    )
    .await;
    assert_eq!(extra_text(&list(&hub, &mut claude).await), "");

    // Another account's AI hears nothing of this board.
    let mut other = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut other, "board_bind", json!({ "project": "theirs" }))
        .await;
    assert_eq!(bound["structuredContent"]["project"], "theirs", "{bound}");
    assert_eq!(extra_text(&bound), "", "{bound}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_note_left_for_an_ai_reaches_it_on_its_next_call_once() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_tenant_cannot_point_a_webhook_at_the_hubs_network() {
    let hub = hosted_with(None, |_, _| json!({})).await;
    let mut claude = hub.first_connect("Claude").await;
    hub.call(&mut claude, "board_bind", json!({ "project": "site" }))
        .await;
    let answer = hub
        .call(
            &mut claude,
            "board_wake",
            json!({ "project": "site", "recipient": "hermes", "url": "https://127.0.0.1/x" }),
        )
        .await;
    assert!(crate::tenant_tests::refused(&answer), "{answer}");
}
