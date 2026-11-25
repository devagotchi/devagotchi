//! What a ticket carries after a run (machine, seat, repo), and every ticket result against its schema.

use super::*;

/// P2c: a ticket carries routing context, not proof that a command executed: the one
/// machine this session addressed, the token's seat, this MCP session and the board's
/// repo. An unregistered AI is refused execution. After addressing two machines the
/// machine is left empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ticket_after_a_refused_run_keeps_routing_context_without_claiming_execution() {
    use serde_json::json;

    use crate::tenancy::tests::tenants::{hosted_with, refused};

    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut person = hub.first_connect("Claude").await;
    // Two machines that joined (enrolled; neither is online).
    for name in ["dabo", "wisconsin"] {
        let added = hub
            .call(&mut person, "machine_add", json!({ "name": name }))
            .await;
        let added = &added["structuredContent"];
        let stored = added["manual"]
            .as_str()
            .unwrap()
            .split("--device ")
            .nth(1)
            .unwrap();
        let stored = stored.split(' ').next().unwrap();
        hub.enroll(
            &stored,
            added["code"].as_str().unwrap(),
            &format!("{name}-key"),
        )
        .await
        .unwrap();
    }
    let bound = hub
        .call(&mut person, "board_bind", json!({ "project": "fills" }))
        .await;
    assert!(!refused(&bound), "{bound}");
    let who = hub.call(&mut person, "whoami", json!({})).await["structuredContent"].clone();
    let subject = who["subject"].as_str().unwrap();
    let board = hub
        .hub
        .boards
        .open_account(subject, "fills")
        .unwrap()
        .expect("bound board");
    let mut settings = board.settings().unwrap();
    settings.repo = Some("https://github.com/x/y".into());
    board
        .put_settings(&Actor::human("jesse"), &settings)
        .unwrap();

    // Routing is known, but this provisional client's unbound identity may not run.
    let ran = hub
        .call(
            &mut person,
            "run",
            json!({ "machine": "dabo", "command": "true" }),
        )
        .await;
    assert!(refused(&ran), "{ran}");
    assert_eq!(ran["structuredContent"]["code"], "not_granted", "{ran}");
    let first = hub
        .call(
            &mut person,
            "board_create",
            json!({ "project": "fills", "title": "after dabo" }),
        )
        .await["structuredContent"]
        .clone();
    assert_eq!(first["machine"], "dabo", "{first}");
    assert_eq!(first["repo"], "https://github.com/x/y");
    // A hub-connected AI's token names no seat of its own: it is the account's "ai".
    assert!(who.get("agent").is_none(), "{who}");
    assert_eq!(first["agent"], "ai", "the seat: {first}");
    assert_eq!(
        first["session"].as_str(),
        person.session.as_deref(),
        "{first}"
    );

    let given = hub
        .call(
            &mut person,
            "board_create",
            json!({
                "project": "fills", "title": "what the caller gives wins", "machine": "other",
                "repo": "https://github.com/a/b", "agent": "codex", "session": "s-9",
            }),
        )
        .await["structuredContent"]
        .clone();
    assert_eq!(given["machine"], "other");
    assert_eq!(given["repo"], "https://github.com/a/b");
    assert_eq!(given["agent"], "codex");
    assert_eq!(given["session"], "s-9");

    let ran = hub
        .call(
            &mut person,
            "run",
            json!({ "machine": "wisconsin", "command": "true" }),
        )
        .await;
    assert!(refused(&ran), "{ran}");
    assert_eq!(ran["structuredContent"]["code"], "not_granted", "{ran}");
    let second = hub
        .call(
            &mut person,
            "board_create",
            json!({ "project": "fills", "title": "after both" }),
        )
        .await["structuredContent"]
        .clone();
    assert!(
        second["machine"].is_null(),
        "two machines: none is guessed: {second}"
    );
    assert_eq!(second["repo"], "https://github.com/x/y");
}

/// Every board tool that answers with tickets answers within its declared output schema
/// with the anchors present and absent (`Hosted::call` holds each result to it).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ticket_results_match_their_schemas_with_anchors_present_and_absent() {
    use serde_json::json;

    use crate::tenancy::tests::tenants::{hosted_with, refused};

    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut person = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut person, "board_bind", json!({ "project": "anchors" }))
        .await;
    assert!(!refused(&bound), "{bound}");
    let anchored = hub
        .call(
            &mut person,
            "board_create",
            json!({
                "project": "anchors", "title": "dabo is down", "machine": "dabo",
                "repo": "https://github.com/x/y", "environment": "prod", "agent": "claude",
                "session": "s-1", "fingerprint": "fp-1",
            }),
        )
        .await["structuredContent"]
        .clone();
    assert_eq!(anchored["machine"], "dabo", "{anchored}");
    assert_eq!(anchored["environment"], "prod");
    assert_eq!(anchored["fingerprint"], "fp-1");
    let id = anchored["id"].as_str().unwrap().to_string();
    let plain = hub
        .call(
            &mut person,
            "board_create",
            json!({ "project": "anchors", "title": "plain" }),
        )
        .await["structuredContent"]
        .clone();
    assert!(plain["machine"].is_null(), "{plain}");
    for project_id in [&id, plain["id"].as_str().unwrap()] {
        let got = hub
            .call(
                &mut person,
                "board_get",
                json!({ "project": "anchors", "id": project_id }),
            )
            .await;
        assert!(!refused(&got), "{got}");
    }
    let all = hub
        .call(&mut person, "board_list", json!({ "project": "anchors" }))
        .await;
    assert!(!refused(&all), "{all}");
    let filtered = hub
        .call(
            &mut person,
            "board_list",
            json!({ "project": "anchors", "machine": "dabo", "environment": "prod" }),
        )
        .await;
    let text = filtered.to_string();
    assert!(
        text.contains(&id) && !text.contains(plain["id"].as_str().unwrap()),
        "{text}"
    );
    let updated = hub
        .call(
            &mut person,
            "board_update",
            json!({ "project": "anchors", "id": id, "version": 1, "machine": "", "environment": "dev" }),
        )
        .await["structuredContent"]
        .clone();
    assert!(
        updated["machine"].is_null(),
        "an empty string clears: {updated}"
    );
    assert_eq!(updated["environment"], "dev");
    let refused_env = hub
        .call(
            &mut person,
            "board_create",
            json!({ "project": "anchors", "title": "qa", "environment": "qa" }),
        )
        .await;
    assert!(refused(&refused_env), "{refused_env}");
    assert!(
        refused_env.to_string().contains("dev, staging, prod"),
        "the refusal names the list: {refused_env}"
    );
}
