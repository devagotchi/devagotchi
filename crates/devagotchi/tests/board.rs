//! The `board` command line of the one binary on a project's standalone board, driven as a
//! person and the AI clients' hooks would: the native hook handler and the plugin's hooks.
//! A standalone board is in Postgres (`DEVAGOTCHI_BOARD_STORAGE_URL`): each test that needs
//! one makes a database of its own on the test Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`) and
//! skips without one (`or_skip!`).

use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use devagotchi_store::{or_skip, testing::empty_postgres};
use serde_json::{Value, json};

/// The binary, its standalone boards in `storage` (none: no Postgres named at all).
fn board_cli(storage: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_devagotchi"));
    match storage {
        Some(url) => command.env("DEVAGOTCHI_BOARD_STORAGE_URL", url),
        None => command.env_remove("DEVAGOTCHI_BOARD_STORAGE_URL"),
    };
    command
}

#[tokio::test]
async fn native_board_hooks_run_without_a_shell_with_spaces_and_never_complete_on_stop() {
    let storage = or_skip!(empty_postgres());
    let devagotchi = || board_cli(Some(&storage));
    let dir = tempfile::Builder::new()
        .prefix("native hook path with spaces ")
        .tempdir()
        .unwrap();
    for args in [
        vec!["board", "init", "--project", "native"],
        vec!["board", "new", "Carry evidence", "--as", "claude"],
    ] {
        let out = devagotchi()
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let listed = devagotchi()
        .args(["board", "ls", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let tickets: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let id = tickets
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "Carry evidence")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    // An agent named by DEVAGOTCHI_ACTOR primes and finds it ready (from one_binary.rs).
    let out = devagotchi()
        .args(["board", "prime", "--json"])
        .current_dir(dir.path())
        .env("DEVAGOTCHI_ACTOR", "claude")
        .output()
        .unwrap();
    let prime: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(prime["actor"], "claude", "{prime}");
    assert_eq!(prime["ready"].as_array().unwrap().len(), 1, "{prime}");
    assert_eq!(prime["ready"][0]["id"], id, "{prime}");
    let claimed = devagotchi()
        .args(["board", "claim", id, "--as", "claude"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        claimed.status.success(),
        "{}",
        String::from_utf8_lossy(&claimed.stderr)
    );
    let env = dir.path().join("client environment");
    let hook = |phase: &str, body: Value| {
        let mut child = devagotchi()
            .args(["board", "hook", phase, "--as", "claude"])
            .current_dir(dir.path())
            .env("CLAUDE_ENV_FILE", &env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&body).unwrap())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let start = hook(
        "session-start",
        json!({
            "hook_event_name":"SessionStart","session_id":"native-session-1",
            "cwd":dir.path(),"source":"resume"
        }),
    );
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let context: Value = serde_json::from_slice(&start.stdout).unwrap();
    assert!(
        context["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains(id)
    );
    assert_eq!(
        std::fs::read_to_string(&env).unwrap(),
        "export DEVAGOTCHI_SESSION=native-session-1\n"
    );
    let stop = hook(
        "stop",
        json!({"hook_event_name":"Stop","stop_hook_active":false}),
    );
    assert_eq!(stop.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&stop.stderr).contains(id));
    let repeated = hook(
        "stop",
        json!({"hook_event_name":"Stop","stop_hook_active":true}),
    );
    assert_eq!(repeated.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&repeated.stderr).contains("No completion or automatic release")
    );
    let ended = hook("session-end", json!({"hook_event_name":"SessionEnd"}));
    assert_eq!(ended.status.code(), Some(5));
    let view = devagotchi()
        .args(["board", "show", id, "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let view: Value = serde_json::from_slice(&view.stdout).unwrap();
    assert_eq!(view["ticket"]["holder"], "claude");
    assert_ne!(view["ticket"]["state"], "done");
    let bad = hook(
        "session-start",
        json!({"hook_event_name":"SessionStart","session_id":"bad;command"}),
    );
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
}

/// The plugin's hook as a client runs it: none of the clients' variables but those in `env`.
fn plugin_hook(
    storage: Option<&str>,
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    body: Value,
) -> Output {
    const HOSTS: [&str; 4] = [
        "CLAUDE_PLUGIN_ROOT",
        "PLUGIN_ROOT",
        "GROK_PLUGIN_ROOT",
        "DEVAGOTCHI_ACTOR",
    ];
    let mut command = board_cli(storage);
    command
        .args(["board", "hook"])
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in HOSTS {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&body).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// What the `devagotchi-agenthub` plugin runs: silent where there is no board.
#[test]
fn plugin_hooks_are_silent_without_a_board() {
    let hook = |dir: &Path, args: &[&str], env: &[(&str, &str)], body: Value| {
        plugin_hook(None, dir, args, env, body)
    };
    let stop = json!({"hook_event_name":"Stop","stop_hook_active":false});

    // No board here or above: the plugin's hooks answer {} and let the client go on.
    let bare = tempfile::tempdir().unwrap();
    let quiet = hook(
        bare.path(),
        &["stop", "--as", "auto", "--if-board"],
        &[("CLAUDE_PLUGIN_ROOT", "/plugins/devagotchi-agenthub")],
        stop.clone(),
    );
    assert!(
        quiet.status.success(),
        "{}",
        String::from_utf8_lossy(&quiet.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&quiet.stdout).trim(), "{}");
    let start = hook(
        bare.path(),
        &["session-start", "--as", "auto", "--if-board"],
        &[("CLAUDE_PLUGIN_ROOT", "/plugins/devagotchi-agenthub")],
        json!({"hook_event_name":"SessionStart","session_id":"plugin-1","cwd":bare.path()}),
    );
    assert!(start.status.success());
    assert_eq!(String::from_utf8_lossy(&start.stdout).trim(), "{}");
    // Without --if-board, no board is still an error, as before.
    let loud = hook(bare.path(), &["stop", "--as", "claude"], &[], stop.clone());
    assert!(!loud.status.success());
    assert!(String::from_utf8_lossy(&loud.stderr).contains("no_board"));
    // --as auto with no plugin host and no DEVAGOTCHI_ACTOR says so instead of guessing.
    let nobody = hook(
        bare.path(),
        &["stop", "--as", "auto", "--if-board"],
        &[],
        stop.clone(),
    );
    assert_eq!(nobody.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&nobody.stderr).contains("--as auto"));
}

/// The agent of a plugin's hook is whichever client's plugin started it.
#[tokio::test]
async fn plugin_hooks_name_the_client_that_runs_them() {
    let storage = or_skip!(empty_postgres());
    let devagotchi = || board_cli(Some(&storage));
    let hook = |dir: &Path, args: &[&str], env: &[(&str, &str)], body: Value| {
        plugin_hook(Some(&storage), dir, args, env, body)
    };
    let stop = json!({"hook_event_name":"Stop","stop_hook_active":false});
    // A board where Grok holds a ticket: Grok's Stop is held, Claude's is not.
    let dir = tempfile::tempdir().unwrap();
    let board = |args: &[&str]| {
        let out = devagotchi()
            .arg("board")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    };
    board(&["init", "--project", "plugin"]);
    board(&["new", "Held by Grok", "--as", "grok"]);
    let listed: Value = serde_json::from_slice(&board(&["ls", "--json"]).stdout).unwrap();
    let id = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "Held by Grok")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    board(&["claim", &id, "--as", "grok"]);
    let auto = ["stop", "--as", "auto", "--if-board"];
    let grok = hook(
        dir.path(),
        &auto,
        &[("GROK_PLUGIN_ROOT", "/plugins/devagotchi-agenthub")],
        stop.clone(),
    );
    assert_eq!(grok.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&grok.stderr).contains(&id));
    let claude = hook(
        dir.path(),
        &auto,
        &[("CLAUDE_PLUGIN_ROOT", "/plugins/devagotchi-agenthub")],
        stop.clone(),
    );
    assert!(
        claude.status.success(),
        "{}",
        String::from_utf8_lossy(&claude.stderr)
    );
    // DEVAGOTCHI_ACTOR still wins over the plugin's client.
    let named = hook(
        dir.path(),
        &auto,
        &[
            ("CLAUDE_PLUGIN_ROOT", "/plugins/devagotchi-agenthub"),
            ("DEVAGOTCHI_ACTOR", "grok"),
        ],
        stop,
    );
    assert_eq!(named.status.code(), Some(2));
}
