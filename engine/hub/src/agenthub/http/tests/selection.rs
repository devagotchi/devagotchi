use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claim_label_intersection_and_empty_field_retries_agree_between_http_and_mcp() {
    let world = or_skip!(world());
    let partial = world
        .write(
            "/api/v1/tasks",
            json!({"project":"demo","title":"Partial","labels":["api"]}),
        )
        .await;
    let target = world
        .write(
            "/api/v1/tasks",
            json!({"project":"demo","title":"Both","labels":["api","release"]}),
        )
        .await;
    let input =
        json!({"project":"demo","label":"release","labels":["api"],"request_id":"labels-once"});
    let claimed = world.write("/api/v1/claim-next", input.clone()).await;
    assert_eq!(claimed["lease"]["ticket"]["id"], target["id"]);
    let mcp = tool(&world.url, "tok-member", "board_claim", input).await;
    assert_ne!(mcp["isError"], true, "{mcp}");
    assert_eq!(mcp["structuredContent"]["replayed"], true);
    assert_eq!(
        mcp["structuredContent"]["lease"]["fence"],
        claimed["lease"]["fence"]
    );
    let input = json!({"project":"demo","label":"api","request_id":"original-shape"});
    let first = world.write("/api/v1/claim-next", input.clone()).await;
    assert_eq!(first["lease"]["ticket"]["id"], partial["id"]);
    let mut explicit = input;
    explicit["labels"] = json!([]);
    let again = world.write("/api/v1/claim-next", explicit).await;
    assert_eq!(again["replayed"], true);
    assert_eq!(again["lease"], first["lease"]);
}
