use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_uses_current_board_settings_and_the_existing_connector_catalog() {
    let world = or_skip!(world());
    let (_, value, _) = response(
        world
            .request(Method::GET, "/api/v1/protocol?project=demo", "tok-member")
            .header("host", "untrusted.example"),
    )
    .await;
    assert_eq!(value["project"], "demo");
    assert!(value["heartbeat_hint_ms"].as_i64().unwrap() > 0);
    let protocol = value["agents_md"].as_str().unwrap();
    for name in [
        "board_prime",
        "board_ready",
        "board_claim",
        "board_heartbeat",
        "board_update",
        "board_note",
        "board_complete",
        "board_handoff",
        "request_id",
        "--request-id",
    ] {
        assert!(protocol.contains(name), "{name}: {protocol}");
    }
    let url = format!("{}/mcp", world.url);
    assert_eq!(value["connect"]["mcp_url"], url);
    let expected = serde_json::to_value(
        devagotchi_connectors::all()
            .iter()
            .map(|connector| connector.render(&url))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(value["connect"]["connectors"], expected);
    assert!(!value.to_string().contains("tok-member"));
    assert!(!value.to_string().contains("untrusted.example"));
    assert_eq!(
        response(world.request(Method::GET, "/api/v1/protocol?project=demo", "tok-other"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        response(world.request(Method::GET, "/api/v1/protocol", "tok-member"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}
