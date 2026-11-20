//! Actual CLI creation preserves dependency flags in the PostgreSQL transaction.
use devagotchi_store::{or_skip, testing::empty_postgres};
use serde_json::{Value, json};
use std::process::Command;

#[tokio::test]
async fn cli_creates_dependencies_atomically() {
    let storage = or_skip!(empty_postgres());
    let directory = tempfile::tempdir().unwrap();
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_devagotchi"))
            .arg("board")
            .args(args)
            .arg("--json")
            .env("DEVAGOTCHI_BOARD_STORAGE_URL", &storage)
            .current_dir(directory.path())
            .output()
            .unwrap()
    };
    assert!(call(&["init", "--project", "creation"]).status.success());
    let first = call(&["new", "First"]);
    assert!(first.status.success(), "{first:?}");
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    let id = first["id"].as_str().unwrap();
    let second = call(&["new", "Second", "--depends-on", id, "--depends-on", id]);
    assert!(second.status.success(), "{second:?}");
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["blocked_by"], json!([id]));
    let before = call(&["ls"]);
    let failed = call(&[
        "new",
        "Must not exist",
        "--depends-on",
        id,
        "--depends-on",
        "missing",
    ]);
    assert_eq!(failed.status.code(), Some(4), "{failed:?}");
    assert_eq!(call(&["ls"]).stdout, before.stdout);
    let verified: Value = serde_json::from_slice(&call(&["verify"]).stdout).unwrap();
    assert_eq!(verified["ok"], true);
}
