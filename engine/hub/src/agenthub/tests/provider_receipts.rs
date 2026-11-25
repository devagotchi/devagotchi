use super::serving::{hub, tool};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn externally_verified_subject_replays_tasks_and_settings_without_a_local_account_row() {
    let dir = tempfile::tempdir().unwrap();
    let url = or_skip!(hub(dir.path()));
    let bound = tool(
        &url,
        "tok-member",
        "board_bind",
        json!({"project":"provider"}),
    )
    .await;
    assert_ne!(bound["isError"], true, "{bound}");
    let create = json!({"project":"provider","title":"One","request_id":"create-one"});
    let first = tool(&url, "tok-member", "board_create", create.clone()).await;
    assert_ne!(first["isError"], true, "{first}");
    let repeat = tool(&url, "tok-member", "board_create", create).await;
    assert_eq!(repeat["structuredContent"]["replayed"], true, "{repeat}");
    assert_eq!(
        repeat["structuredContent"]["id"],
        first["structuredContent"]["id"]
    );
    let settings = json!({"project":"provider","repo":"https://github.com/example/one","request_id":"settings-one"});
    let first = tool(&url, "tok-member", "board_settings", settings.clone()).await;
    assert_ne!(first["isError"], true, "{first}");
    let changed = tool(
        &url,
        "tok-member",
        "board_settings",
        json!({"project":"provider","repo":"https://github.com/example/two"}),
    )
    .await;
    assert_ne!(changed["isError"], true, "{changed}");
    let replay = tool(&url, "tok-member", "board_settings", settings).await;
    assert_eq!(replay["structuredContent"]["replayed"], true, "{replay}");
    assert_eq!(
        replay["structuredContent"]["repo"],
        "https://github.com/example/one"
    );
    let current = tool(
        &url,
        "tok-member",
        "board_settings",
        json!({"project":"provider"}),
    )
    .await;
    assert_eq!(
        current["structuredContent"]["repo"],
        "https://github.com/example/two"
    );
    let read_key = tool(
        &url,
        "tok-member",
        "board_settings",
        json!({"project":"provider","request_id":"read"}),
    )
    .await;
    assert_eq!(read_key["structuredContent"]["code"], "invalid");
}
