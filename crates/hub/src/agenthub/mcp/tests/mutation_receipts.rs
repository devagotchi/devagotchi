use crate::tenancy::tests::tenants::{Person, hosted_with, refused};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_keys_survive_mcp_sessions_and_replay_flags_match_saved_results() {
    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut person = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut person, "board_bind", json!({"project":"retries"}))
        .await;
    assert!(!refused(&bound), "{bound}");
    let input = json!({"project":"retries","title":"One task","request_id":"create-1"});
    let first = hub.call(&mut person, "board_create", input.clone()).await;
    assert!(!refused(&first), "{first}");
    let first = first["structuredContent"].clone();
    assert_eq!(first["replayed"], false);
    assert_eq!(first["request_id"], "create-1");
    let id = first["id"].as_str().unwrap();
    let mut second_session = Person {
        cookie: person.cookie.clone(),
        token: person.token.clone(),
        session: None,
    };
    let second = hub.call(&mut second_session, "board_create", input).await;
    assert!(!refused(&second), "{second}");
    let mut second = second["structuredContent"].clone();
    assert_eq!(second["replayed"], true);
    assert_ne!(person.session, second_session.session);
    second["replayed"] = false.into();
    assert_eq!(
        second, first,
        "original session metadata must survive reconnect replay"
    );
    let changed = hub
        .call(
            &mut person,
            "board_create",
            json!({"project":"retries","title":"Changed","request_id":"create-1"}),
        )
        .await;
    assert_eq!(changed["structuredContent"]["code"], "request_conflict");
    let edit =
        json!({"project":"retries","id":id,"version":1,"title":"Edited","request_id":"edit-1"});
    let updated = hub.call(&mut person, "board_update", edit.clone()).await;
    assert!(!refused(&updated), "{updated}");
    let again = hub
        .call(&mut second_session, "board_update", edit.clone())
        .await;
    assert_eq!(again["structuredContent"]["replayed"], true, "{again}");
    assert_eq!(again["structuredContent"]["version"], 2);
    let mut clear_progress = edit;
    clear_progress["progress"] = serde_json::Value::Null;
    let conflict = hub.call(&mut person, "board_update", clear_progress).await;
    assert_eq!(
        conflict["structuredContent"]["code"], "request_conflict",
        "explicit null differs from omission: {conflict}"
    );
    let note = json!({"project":"retries","id":id,"body":"One note","request_id":"note-1"});
    let one = hub.call(&mut person, "board_note", note.clone()).await;
    assert!(!refused(&one), "{one}");
    let two = hub.call(&mut second_session, "board_note", note).await;
    assert_eq!(two["structuredContent"]["replayed"], true, "{two}");
    assert_eq!(
        one["structuredContent"]["seq"],
        two["structuredContent"]["seq"]
    );
    let cross_op = hub
        .call(
            &mut person,
            "board_note",
            json!({"project":"retries","id":id,"body":"Other","request_id":"create-1"}),
        )
        .await;
    assert_eq!(cross_op["structuredContent"]["code"], "request_conflict");
    for key in [String::new(), "x".repeat(201)] {
        let invalid = hub
            .call(
                &mut person,
                "board_create",
                json!({"project":"retries","title":"Invalid","request_id":key}),
            )
            .await;
        assert_eq!(invalid["structuredContent"]["code"], "invalid");
    }
    let view = hub
        .call(
            &mut person,
            "board_get",
            json!({"project":"retries","id":id}),
        )
        .await;
    assert_eq!(
        view["structuredContent"]["notes"].as_array().unwrap().len(),
        1
    );
}
