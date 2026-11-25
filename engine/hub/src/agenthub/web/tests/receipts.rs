use crate::web::app::tests::{hosted, page, things_of};
use serde_json::json;

fn key(html: &str) -> &str {
    html.split_once("name=\"request_id\" value=\"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_creation_and_ticket_actions_retry_once_with_original_close_redirect() {
    let hub = or_skip!(hosted());
    let mut person = hub.first_connect("Browser retries").await;
    things_of(&hub, &mut person).await;
    let (_, html) = page(&hub, "/app/boards/alpha/new", &person).await;
    let fields = [("title", "Browser retry"), ("request_id", key(&html))];
    let create_key = fields[1].1.to_owned();
    let mut location = String::new();
    for attempt in 0..2 {
        let response = hub
            .form("/app/boards/alpha/new", Some(&person.cookie), &fields)
            .await;
        assert_eq!(response.status(), 303);
        assert_eq!(
            response.headers().contains_key("idempotent-replayed"),
            attempt == 1
        );
        let next = response.headers()["location"].to_str().unwrap();
        if !location.is_empty() {
            assert_eq!(location, next);
        }
        location = next.into();
    }
    let id = location.rsplit('/').next().unwrap();
    let (_, html) = page(&hub, &location, &person).await;
    let form = html
        .split("<form ")
        .find(|form| form.contains(&format!("action=\"{location}/comment\"")))
        .unwrap();
    let comment = [("body", "One comment"), ("request_id", key(form))];
    let comment_key = comment[1].1.to_owned();
    for _ in 0..2 {
        assert_eq!(
            hub.form(
                &format!("{location}/comment"),
                Some(&person.cookie),
                &comment
            )
            .await
            .status(),
            303
        );
    }
    let view = hub
        .call(&mut person, "board_get", json!({"project":"alpha","id":id}))
        .await;
    assert_eq!(
        view["structuredContent"]["notes"].as_array().unwrap().len(),
        1
    );
    let version = view["structuredContent"]["ticket"]["version"]
        .as_i64()
        .unwrap()
        .to_string();
    let from = view["structuredContent"]["ticket"]["state"]
        .as_str()
        .unwrap();
    let close = [
        ("version", version.as_str()),
        ("from", from),
        ("request_id", "close-retry"),
    ];
    let mut original = String::new();
    for _ in 0..2 {
        let response = hub
            .form(&format!("{location}/close"), Some(&person.cookie), &close)
            .await;
        let next = response.headers()["location"].to_str().unwrap();
        assert!(next.contains("done=close"), "{next}");
        if !original.is_empty() {
            assert_eq!(next, original);
        }
        original = next.into();
    }
    let view = hub
        .call(&mut person, "board_get", json!({"project":"alpha","id":id}))
        .await;
    let version = view["structuredContent"]["ticket"]["version"]
        .as_i64()
        .unwrap()
        .to_string();
    let undo = [
        ("version", version.as_str()),
        ("from", from),
        ("request_id", "undo-retry"),
    ];
    for _ in 0..2 {
        let response = hub
            .form(&format!("{location}/undo"), Some(&person.cookie), &undo)
            .await;
        assert!(
            response.headers()["location"]
                .to_str()
                .unwrap()
                .contains("done=undo")
        );
    }
    // Retrying the old close after undo must not close it a second time.
    let response = hub
        .form(&format!("{location}/close"), Some(&person.cookie), &close)
        .await;
    assert_eq!(response.headers()["location"], original);
    let view = hub
        .call(&mut person, "board_get", json!({"project":"alpha","id":id}))
        .await;
    assert_eq!(view["structuredContent"]["ticket"]["state"], from);
    let history = hub
        .call(
            &mut person,
            "board_events",
            json!({"project":"alpha","ticket":id}),
        )
        .await;
    let requests: Vec<_> = history["structuredContent"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["request_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        requests,
        [
            create_key.as_str(),
            comment_key.as_str(),
            "close-retry",
            "undo-retry"
        ]
    );
}
