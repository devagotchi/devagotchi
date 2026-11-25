//! `board scan --import` on a project bound to the hub: the findings land on the account's
//! board in Postgres, and a second scan finds them there by fingerprint and adds nothing.

use std::{ffi::OsString, path::Path};

use serde_json::Value;

use super::serving::{hub, signed_in};
use crate::agenthub::cli::{self, EXIT_OK, Invocation};

fn run(cwd: &Path, account: &Path, args: &[&str]) -> (u8, Value) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run(
        Invocation {
            args: args.iter().map(OsString::from).collect(),
            cwd: cwd.to_path_buf(),
            actor_env: Some("claude".into()),
            session_env: None,
            os_user: Some("jesse".into()),
            stdout_is_terminal: false,
            account_file: Some(account.to_path_buf()),
            plugin_host: None,
            storage_env: None,
            hub: Some(std::sync::Arc::new(crate::login::client::AccountHub)),
        },
        &mut out,
        &mut err,
    );
    let out = String::from_utf8(out).unwrap();
    let value = serde_json::from_str(out.trim()).unwrap_or(Value::Null);
    assert_eq!(code, EXIT_OK, "{out}{}", String::from_utf8_lossy(&err));
    (code, value)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_imports_once_onto_the_hubs_board() {
    let dir = tempfile::tempdir().unwrap();
    let url = devagotchi_store::or_skip!(hub(dir.path()));
    let project = dir.path().join("demo");
    std::fs::create_dir_all(project.join(".kiro/specs/push")).unwrap();
    std::fs::write(
        project.join(".kiro/specs/push/tasks.md"),
        "# Push\n- [ ] Subscribe the phone\n",
    )
    .unwrap();
    std::fs::write(project.join("main.py"), "# TODO: retry the send\n").unwrap();
    let account = dir.path().join("account.json");
    signed_in(&account, "tok-member");

    tokio::task::spawn_blocking(move || {
        run(&project, &account, &["bind", "--hub", &url]);
        let (_, imported) = run(&project, &account, &["scan", "--import"]);
        assert_eq!(imported["imported"], 2, "{imported}");
        let id = imported["items"][0]["ticket"].as_str().unwrap().to_string();
        let (_, again) = run(&project, &account, &["scan", "--import"]);
        assert_eq!(
            (again["imported"].clone(), again["on_board"].clone()),
            (0.into(), 2.into())
        );
        assert_eq!(again["items"][0]["ticket"], id.as_str());
        let (_, listed) = run(&project, &account, &["ls", "--label", "scanned"]);
        assert_eq!(listed.as_array().map(Vec::len), Some(2), "{listed}");
    })
    .await
    .unwrap();
}
