//! CLI omission survives the real MCP adapter; explicit null remains a clear operation.
use super::serving::{hub, run, signed_in};
use crate::agenthub::cli::EXIT_OK;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bound_cli_heartbeat_preserves_omitted_progress_and_clears_explicit_null() {
    let dir = tempfile::tempdir().unwrap();
    let url = or_skip!(hub(dir.path()));
    let project = dir.path().join("demo");
    std::fs::create_dir_all(&project).unwrap();
    let account = dir.path().join("account.json");
    signed_in(&account, "tok-member");
    tokio::task::spawn_blocking(move || {
        let ok = |args: &[&str]| {
            let (code, result, error) = run(&project, &account, None, args);
            assert_eq!(code, EXIT_OK, "{result} {error}");
            result
        };
        ok(&["bind", "--hub", &url]);
        let made = ok(&["new", "Keep progress"]);
        let id = made["id"].as_str().unwrap();
        ok(&["claim", id]);
        let set = ok(&["beat", id, "--progress", "{\"step\":4}"]);
        assert_eq!(set["ticket"]["progress"]["step"], 4);
        let kept = ok(&["beat", id]);
        assert_eq!(kept["ticket"]["progress"]["step"], 4);
        let cleared = ok(&["beat", id, "--progress", "null"]);
        assert!(cleared["ticket"]["progress"].is_null());
    })
    .await
    .unwrap();
}
