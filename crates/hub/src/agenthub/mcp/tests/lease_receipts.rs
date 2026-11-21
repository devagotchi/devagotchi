use crate::tenancy::tests::tenants::{hosted_with, refused};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lease_replay_does_not_replace_or_forget_a_newer_cached_fence() {
    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut person = hub.first_connect("Claude").await;
    hub.call(&mut person, "board_bind", json!({"project":"leases"}))
        .await;
    let made = hub
        .call(
            &mut person,
            "board_create",
            json!({"project":"leases","title":"Work"}),
        )
        .await;
    let id = made["structuredContent"]["id"].as_str().unwrap();
    let claim = json!({"project":"leases","id":id,"request_id":"claim-1"});
    let first = hub.call(&mut person, "board_claim", claim.clone()).await;
    assert!(!refused(&first), "{first}");
    let first_fence = first["structuredContent"]["lease"]["fence"]
        .as_i64()
        .unwrap();
    let heartbeat = json!({"project":"leases","id":id,"request_id":"heartbeat-1"});
    let beat = hub
        .call(&mut person, "board_heartbeat", heartbeat.clone())
        .await;
    assert!(!refused(&beat), "{beat}");
    let release = json!({"project":"leases","id":id,"reason":"Pause","request_id":"release-1"});
    let released = hub
        .call(&mut person, "board_release", release.clone())
        .await;
    assert!(!refused(&released), "{released}");
    // Release forgot the implicit fence. Matching retry still reaches the receipt.
    let retry = hub
        .call(&mut person, "board_release", release.clone())
        .await;
    assert_eq!(retry["structuredContent"]["replayed"], true, "{retry}");
    let new = hub
        .call(
            &mut person,
            "board_claim",
            json!({"project":"leases","id":id}),
        )
        .await;
    assert!(!refused(&new), "{new}");
    let new_fence = new["structuredContent"]["lease"]["fence"].as_i64().unwrap();
    assert!(new_fence > first_fence);
    for (tool, input) in [
        ("board_claim", claim),
        ("board_heartbeat", heartbeat),
        ("board_release", release),
    ] {
        let replay = hub.call(&mut person, tool, input).await;
        assert_eq!(replay["structuredContent"]["replayed"], true, "{replay}");
    }
    let current = hub
        .call(
            &mut person,
            "board_heartbeat",
            json!({"project":"leases","id":id}),
        )
        .await;
    assert!(!refused(&current), "{current}");
    assert_eq!(current["structuredContent"]["fence"], new_fence);
    let complete = json!({"project":"leases","id":id,"summary":"Ready","request_id":"complete-1"});
    let completed = hub
        .call(&mut person, "board_complete", complete.clone())
        .await;
    assert!(!refused(&completed), "{completed}");
    let repeated = hub.call(&mut person, "board_complete", complete).await;
    assert_eq!(
        repeated["structuredContent"]["replayed"], true,
        "{repeated}"
    );
    assert_eq!(
        repeated["structuredContent"]["version"],
        completed["structuredContent"]["version"]
    );
}
