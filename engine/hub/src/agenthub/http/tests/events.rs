use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_event_pages_resume_in_order_and_never_cross_board_access() {
    let world = or_skip!(world());
    let before = world.get("/api/v1/tasks?project=demo").await["head_seq"]
        .as_i64()
        .unwrap();
    let first = world
        .write(
            "/api/v1/tasks",
            json!({"project":"demo","title":"First event"}),
        )
        .await;
    let second = world
        .write(
            "/api/v1/tasks",
            json!({"project":"demo","title":"Second event"}),
        )
        .await;
    let page = world
        .get(&format!(
            "/api/v1/events?project=demo&after_seq={before}&limit=1"
        ))
        .await;
    let events = page["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["ticket"], first["id"]);
    let cursor = events[0]["seq"].as_i64().unwrap();
    let next = world
        .get(&format!(
            "/api/v1/events?project=demo&after_seq={cursor}&limit=1"
        ))
        .await;
    assert_eq!(next["events"][0]["ticket"], second["id"]);
    let next_cursor = next["events"][0]["seq"].as_i64().unwrap();
    assert!(next_cursor > cursor);
    let backwards = world
        .get(&format!(
            "/api/v1/events?project=demo&before_seq={next_cursor}&order=desc&limit=1"
        ))
        .await;
    assert_eq!(backwards["events"][0]["seq"], cursor);
    let verify = world.get("/api/v1/events/verify?project=demo").await;
    assert_eq!(verify["ok"], true);
    assert_eq!(verify["entries"], world.entries().await);
    for path in [
        "/api/v1/events?project=demo",
        "/api/v1/events/verify?project=demo",
    ] {
        let (status, _, _) = response(world.request(Method::GET, path, "tok-other")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    for path in [
        "/api/v1/events",
        "/api/v1/events?project=demo&after_seq=-1",
        "/api/v1/events?project=demo&limit=1001",
        "/api/v1/events?project=demo&order=unknown",
    ] {
        let (status, _, _) = response(world.request(Method::GET, path, "tok-member")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
}
