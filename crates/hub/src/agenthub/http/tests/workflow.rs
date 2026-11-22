use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_dependency_claim_next_note_handoff_force_and_complete_use_the_same_board() {
    let world = or_skip!(world());
    let task = world
        .write(
            "/api/v1/tasks",
            json!({"project":"demo","title":"Workflow","labels":["http-flow"]}),
        )
        .await;
    let blocker = world
        .write("/api/v1/tasks", json!({"project":"demo","title":"Blocker"}))
        .await;
    let id = task["id"].as_str().unwrap();
    let path = |operation: &str| format!("/api/v1/tasks/{id}/{operation}?project=demo");
    let dependency = json!({"depends_on":blocker["id"],"request_id":"dep-once"});
    let deps = world.write(&path("deps"), dependency.clone()).await;
    assert_eq!(deps["deps"].as_array().unwrap().len(), 1);
    assert_eq!(
        world.write(&path("deps"), dependency).await["replayed"],
        true
    );
    let empty = json!({"project":"demo","label":"http-flow","request_id":"empty-claim"});
    let before = world.entries().await;
    assert_eq!(
        world.write("/api/v1/claim-next", empty.clone()).await["claimed"],
        false
    );
    assert_eq!(
        world.entries().await,
        before,
        "empty claim creates no task event"
    );
    world
        .write(
            &path("deps"),
            json!({"depends_on":blocker["id"],"remove":true}),
        )
        .await;
    let replay = world.write("/api/v1/claim-next", empty).await;
    assert_eq!(replay["claimed"], false);
    assert_eq!(
        replay["replayed"], true,
        "new readiness does not rewrite old result"
    );
    let claim = world
        .write(
            "/api/v1/claim-next",
            json!({"project":"demo","label":"http-flow"}),
        )
        .await;
    assert_eq!(claim["lease"]["ticket"]["id"], id);
    let note =
        json!({"body":"What I learned","fence":claim["lease"]["fence"],"request_id":"note-once"});
    let original = world.write(&path("notes"), note.clone()).await;
    let handoff = world
        .write(
            &path("handoff"),
            json!({"fence":claim["lease"]["fence"],"note":"Continue here"}),
        )
        .await;
    assert!(handoff["holder"].is_null());
    assert_eq!(
        world.write(&path("notes"), note).await["seq"],
        original["seq"]
    );
    world.write(&path("claim"), json!({})).await;
    let forced = world
        .write(
            &path("force-release"),
            json!({"reason":"Operator recovery"}),
        )
        .await;
    assert!(forced["holder"].is_null());
    let claim = world.write(&path("claim"), json!({})).await;
    let complete = world.write(&path("complete"), json!({"fence":claim["fence"],"summary":"Implemented and checked","request_id":"complete-once"})).await;
    assert!(complete["holder"].is_null());
    let replay = world.write(&path("complete"), json!({"fence":claim["fence"],"summary":"Implemented and checked","request_id":"complete-once"})).await;
    assert_eq!(replay["version"], complete["version"]);
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        world.get("/api/v1/events/verify?project=demo").await["ok"],
        true
    );
}
