//! The command line on a project's local board (ADR 0002): every command, its exit codes and
//! `next` hints, the hook pack. A bound project's commands are the same over a hub's board
//! (`crates/hub`, `agenthub/tests.rs`, `mod bound`).

use std::{ffi::OsString, path::Path};

use crate::cli::{
    self, EXIT_CONFLICT, EXIT_NOT_FOUND, EXIT_OK, EXIT_UNAVAILABLE, EXIT_USAGE, Invocation,
};

fn run(cwd: &Path, actor: Option<&str>, terminal: bool, args: &[&str]) -> (u8, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run(
        Invocation {
            args: args.iter().map(OsString::from).collect(),
            cwd: cwd.to_path_buf(),
            actor_env: actor.map(str::to_string),
            session_env: None,
            os_user: Some("jesse".into()),
            stdout_is_terminal: terminal,
            account_file: Some(cwd.join("account.json")),
            plugin_host: None,
            hub: None,
        },
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text.lines().next().unwrap_or("")).expect("one json line")
}

#[test]
fn board_settings_names_the_repo_and_environments_of_a_local_board() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, _) = run(dir.path(), None, false, &["init", "--project", "demo"]);
    assert_eq!(code, EXIT_OK, "{out}");
    let (code, out, err) = run(
        dir.path(),
        None,
        false,
        &[
            "settings",
            "--repo",
            "https://github.com/x/demo",
            "--env",
            "dev,prod",
        ],
    );
    assert_eq!(code, EXIT_OK, "{err}");
    let set = json(&out);
    assert_eq!(set["repo"], "https://github.com/x/demo");
    assert_eq!(set["environments"], serde_json::json!(["dev", "prod"]));
    let (code, out, _) = run(dir.path(), None, true, &["settings"]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(
        out.trim(),
        "repo https://github.com/x/demo · environments dev, prod"
    );
    let (code, out, _) = run(dir.path(), None, false, &["settings", "--env", "a b"]);
    assert_eq!(code, cli::EXIT_REFUSED, "{out}");
    let refused = json(&out);
    assert_eq!(refused["error"]["code"], "invalid_settings");
    assert!(
        refused["error"]["next"]
            .as_str()
            .unwrap()
            .contains("\"a b\" is not a lowercase name"),
        "{refused}"
    );
    let (code, out, _) = run(
        dir.path(),
        None,
        false,
        &["new", "staged", "--env", "staging"],
    );
    assert_eq!(
        code,
        cli::EXIT_REFUSED,
        "the board's environments are the new ones: {out}"
    );
}

#[test]
fn init_creates_the_board_and_appends_the_agents_block_once() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "# Repo\n").unwrap();
    let (code, out, _) = run(
        dir.path(),
        None,
        false,
        &["init", "--project", "demo", "--agents-md"],
    );
    assert_eq!(code, EXIT_OK, "{out}");
    assert!(dir.path().join(".devagotchi/board.sqlite3").is_file());
    assert!(dir.path().join(".devagotchi/config.json").is_file());
    assert!(dir.path().join(".devagotchi/.gitignore").is_file());
    let agents = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(agents.starts_with("# Repo\n"));
    assert_eq!(agents.matches(cli::AGENTS_MD_START).count(), 1);
    let (code, out, _) = run(dir.path(), None, false, &["init", "--agents-md"]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(json(&out)["agents_md"], "already present");
    assert_eq!(
        json(&out)["project"],
        "demo",
        "the project name is kept on re-init"
    );
    let again = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert_eq!(again.matches(cli::AGENTS_MD_START).count(), 1);
}

#[test]
fn the_loop_works_from_a_subdirectory_with_a_cached_fence() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(dir.path(), None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    let sub = dir.path().join("crates").join("x");
    std::fs::create_dir_all(&sub).unwrap();
    let (code, out, _) = run(
        &sub,
        Some("claude"),
        false,
        &["new", "port it", "--label", "Hub"],
    );
    assert_eq!(code, EXIT_OK, "{out}");
    let id = json(&out)["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("demo-"));
    let (code, out, _) = run(&sub, Some("claude"), false, &["claim"]);
    assert_eq!(code, EXIT_OK, "{out}");
    assert_eq!(json(&out)["ticket"]["id"], id);
    assert!(dir.path().join(".devagotchi/claims/claude.json").is_file());
    // No --fence: the cache supplies it.
    let (code, _, _) = run(&sub, Some("claude"), false, &["beat", &id]);
    assert_eq!(code, EXIT_OK);
    let (code, _, _) = run(
        &sub,
        Some("claude"),
        false,
        &["note", &id, "learned a thing", "--to", "codex"],
    );
    assert_eq!(code, EXIT_OK);
    let (code, out, _) = run(&sub, Some("claude"), false, &["done", &id, "shipped"]);
    assert_eq!(code, EXIT_OK, "{out}");
    assert_eq!(json(&out)["state"], "review");
    assert!(
        !dir.path().join(".devagotchi/claims/claude.json").exists(),
        "cache cleared on done"
    );
    // The author may not review; codex may, and prime tells codex about the note.
    let (code, out, _) = run(&sub, Some("claude"), false, &["claim", &id]);
    assert_eq!(code, EXIT_CONFLICT);
    assert_eq!(json(&out)["error"]["code"], "distinct_required");
    let (code, out, _) = run(&sub, Some("codex"), false, &["prime"]);
    assert_eq!(code, EXIT_OK);
    let prime = json(&out);
    assert_eq!(prime["notes_to_me"][0]["ticket"], id);
    assert_eq!(prime["ready"][0]["id"], id);
    let (code, _, _) = run(&sub, Some("codex"), false, &["claim", &id]);
    assert_eq!(code, EXIT_OK);
    let (code, out, _) = run(&sub, Some("codex"), false, &["done", &id, "looks right"]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(json(&out)["state"], "done");
}

#[test]
fn held_beat_all_and_the_hook_pack() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(dir.path(), None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    // Nothing held: `held` is an empty list, and empty text (the stop hook keys on that).
    let (code, out, _) = run(dir.path(), Some("claude"), false, &["held"]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(json(&out), serde_json::json!([]));
    let (code, out, _) = run(dir.path(), Some("claude"), true, &["held"]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(out.trim(), "");
    let (code, out, _) = run(dir.path(), Some("claude"), false, &["beat"]);
    assert_eq!(code, EXIT_OK, "{out}");
    assert_eq!(json(&out), serde_json::json!([]));

    for title in ["first", "second"] {
        assert_eq!(run(dir.path(), None, false, &["new", title]).0, EXIT_OK);
    }
    for _ in 0..2 {
        assert_eq!(
            run(dir.path(), Some("claude"), false, &["claim"]).0,
            EXIT_OK
        );
    }
    let (code, out, _) = run(dir.path(), Some("claude"), false, &["held"]);
    assert_eq!(code, EXIT_OK);
    let held = json(&out);
    assert_eq!(held.as_array().unwrap().len(), 2);
    let before: Vec<i64> = held
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["lease_expires_at"].as_i64().unwrap())
        .collect();
    // `beat` with no id extends every held lease, with no cached fence needed.
    std::fs::remove_dir_all(dir.path().join(".devagotchi/claims")).unwrap();
    let (code, out, _) = run(dir.path(), Some("claude"), false, &["beat"]);
    assert_eq!(code, EXIT_OK, "{out}");
    let beaten = json(&out);
    assert_eq!(beaten.as_array().unwrap().len(), 2);
    for (lease, earlier) in beaten.as_array().unwrap().iter().zip(before) {
        assert!(lease["lease_expires_at"].as_i64().unwrap() >= earlier);
    }
    let (_, out, _) = run(dir.path(), Some("claude"), true, &["held"]);
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(out.contains("fence"), "{out}");

    // The hook pack: three executable scripts, the ignore line, both snippets.
    let (code, out, _) = run(dir.path(), None, false, &["hooks"]);
    assert_eq!(code, EXIT_OK, "{out}");
    let hooks = json(&out);
    for script in ["session-start.sh", "beat.sh", "stop.sh"] {
        let path = dir.path().join(".devagotchi/hooks").join(script);
        assert!(path.is_file(), "{script}");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.starts_with("#!/bin/sh\n"), "{script}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
                0o111
            );
        }
    }
    assert!(
        std::fs::read_to_string(dir.path().join(".devagotchi/hooks/stop.sh"))
            .unwrap()
            .contains("board hook stop")
    );
    assert!(
        std::fs::read_to_string(dir.path().join(".devagotchi/.gitignore"))
            .unwrap()
            .contains(cli::HOOK_MARKER_IGNORE)
    );
    assert!(
        hooks["claude_code"]["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook stop --as claude")
    );
    assert!(
        hooks["codex"]["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook session-start --as codex")
    );
    let (code, out, _) = run(dir.path(), None, true, &["hooks", "--for", "codex"]);
    assert_eq!(code, EXIT_OK);
    assert!(
        out.contains("Codex") && !out.contains("Claude Code"),
        "{out}"
    );
    let (code, _, err) = run(dir.path(), None, false, &["hooks", "--for", "vim"]);
    assert_eq!(code, EXIT_USAGE, "{err}");
}

#[test]
fn exit_codes_and_next_hints_by_failure_class() {
    let dir = tempfile::tempdir().unwrap();
    let (code, _, err) = run(dir.path(), None, false, &["prime"]);
    assert_eq!(code, EXIT_UNAVAILABLE, "no board here");
    assert!(err.is_empty(), "json mode puts the error on stdout");
    assert_eq!(run(dir.path(), None, false, &["init"]).0, EXIT_OK);
    let (code, out, _) = run(dir.path(), None, false, &["show", "nope"]);
    assert_eq!(code, EXIT_NOT_FOUND);
    assert!(
        json(&out)["error"]["next"]
            .as_str()
            .unwrap()
            .contains("board ls")
    );
    let (code, _, err) = run(dir.path(), None, true, &["bogus"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("next: board --help"));
    let (code, _, err) = run(dir.path(), None, true, &["new", "x", "--limit", "many"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("--limit must be an integer"));
    let (code, out, _) = run(dir.path(), Some("claude"), false, &["new", "held"]);
    assert_eq!(code, EXIT_OK);
    let id = json(&out)["id"].as_str().unwrap().to_string();
    assert_eq!(
        run(dir.path(), Some("claude"), false, &["claim", &id]).0,
        EXIT_OK
    );
    let (code, out, _) = run(
        dir.path(),
        Some("claude"),
        false,
        &["beat", &id, "--fence", "99"],
    );
    assert_eq!(code, EXIT_CONFLICT);
    assert_eq!(json(&out)["error"]["code"], "stale_fence");
    // An agent may not force-release; the bare OS user (a person) may.
    let (code, out, _) = run(
        dir.path(),
        Some("codex"),
        false,
        &["force", &id, "stuck session"],
    );
    assert_eq!(code, 3);
    assert_eq!(json(&out)["error"]["code"], "humans_only");
    let (code, out, _) = run(dir.path(), None, false, &["force", &id, "stuck session"]);
    assert_eq!(code, EXIT_OK, "{out}");
    assert_eq!(json(&out)["holder"], serde_json::Value::Null);
}

mod output;
