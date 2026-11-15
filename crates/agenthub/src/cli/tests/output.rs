//! What the command line prints and records: `prime`, `show`, the export, anchors, a session.

use super::*;

#[test]
fn prime_text_stays_short_and_names_the_protocol() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(dir.path(), None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    for n in 0..30 {
        let title = format!("ticket {n}");
        assert_eq!(run(dir.path(), None, false, &["new", &title]).0, EXIT_OK);
    }
    let (code, out, _) = run(dir.path(), Some("claude"), true, &["prime"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.lines().count() <= 50, "{} lines", out.lines().count());
    assert!(out.contains("protocol: claim → beat → note → done"));
    assert!(out.starts_with("board demo · you are claude"));
    let (code, out, _) = run(dir.path(), Some("claude"), true, &["prime", "--quiet"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.is_empty());
}

#[test]
fn export_writes_one_json_line_per_ticket_and_verify_reads_the_chain() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(dir.path(), None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    assert_eq!(run(dir.path(), None, false, &["new", "a"]).0, EXIT_OK);
    assert_eq!(run(dir.path(), None, false, &["new", "b"]).0, EXIT_OK);
    let (code, out, _) = run(dir.path(), None, false, &["export"]);
    assert_eq!(code, EXIT_OK, "{out}");
    let text = std::fs::read_to_string(dir.path().join(".devagotchi/board.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 2);
    let (code, out, _) = run(dir.path(), None, false, &["verify"]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(json(&out)["entries"], 2);
    assert_eq!(json(&out)["ok"], true);
}

#[test]
fn prime_and_show_print_the_anchors_as_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    assert_eq!(
        run(d, None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    let (_, out, _) = run(
        d,
        Some("claude"),
        false,
        &[
            "new",
            "dabo is down",
            "--machine",
            "dabo",
            "--env",
            "prod",
            "--repo",
            "https://github.com/x/y",
            "--agent",
            "claude",
            "--session",
            "4f1c9a2b-77e0",
        ],
    );
    let id = json(&out)["id"].as_str().unwrap().to_string();
    let line = "dabo · prod · github.com/x/y · claude (session 4f1c9a2b…)";
    let (code, shown, _) = run(d, Some("claude"), true, &["show", &id]);
    assert_eq!(code, EXIT_OK, "{shown}");
    assert!(shown.lines().any(|l| l.trim() == line), "{shown}");
    let (code, primed, _) = run(d, Some("claude"), true, &["prime"]);
    assert_eq!(code, EXIT_OK, "{primed}");
    let row = primed.lines().find(|l| l.contains(&id)).expect("ready row");
    assert!(row.ends_with(&format!("· {line}")), "{row}");
    let (_, primed, _) = run(d, Some("claude"), false, &["prime"]);
    assert_eq!(json(&primed)["ready"][0]["anchors"], line);
    let (_, plain, _) = run(d, Some("codex"), false, &["new", "plain"]);
    let plain_id = json(&plain)["id"].as_str().unwrap().to_string();
    let (_, shown, _) = run(d, Some("claude"), true, &["show", &plain_id]);
    assert_eq!(shown.lines().count(), 2, "nothing known, no line: {shown}");
}

#[test]
fn a_new_ticket_carries_the_session_the_hook_pack_exported() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(dir.path(), None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    let new = |session: Option<&str>, args: &[&str]| {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = cli::run(
            Invocation {
                args: args.iter().map(OsString::from).collect(),
                cwd: dir.path().to_path_buf(),
                actor_env: Some("claude".into()),
                session_env: session.map(str::to_string),
                os_user: Some("jesse".into()),
                stdout_is_terminal: false,
                account_file: None,
                plugin_host: None,
                hub: None,
            },
            &mut out,
            &mut err,
        );
        assert_eq!(code, EXIT_OK, "{}", String::from_utf8_lossy(&err));
        json(&String::from_utf8(out).unwrap())
    };
    assert_eq!(
        new(Some("s-env"), &["new", "from the hook"])["session"],
        "s-env"
    );
    assert_eq!(
        new(Some("s-env"), &["new", "flag wins", "--session", "s-flag"])["session"],
        "s-flag"
    );
    assert!(new(None, &["new", "none"])["session"].is_null());
}

#[test]
fn anchors_are_flags_on_new_set_and_ls_and_in_the_json() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    assert_eq!(
        run(d, None, false, &["init", "--project", "demo"]).0,
        EXIT_OK
    );
    let (code, out, _) = run(
        d,
        None,
        false,
        &[
            "new",
            "dabo is down",
            "--machine",
            "dabo",
            "--repo",
            "https://github.com/x/y",
            "--env",
            "prod",
            "--agent",
            "claude",
            "--session",
            "s-1",
            "--fingerprint",
            "fp-1",
        ],
    );
    assert_eq!(code, EXIT_OK, "{out}");
    let ticket = json(&out);
    for (key, value) in [
        ("machine", "dabo"),
        ("repo", "https://github.com/x/y"),
        ("environment", "prod"),
        ("agent", "claude"),
        ("session", "s-1"),
        ("fingerprint", "fp-1"),
    ] {
        assert_eq!(ticket[key], value, "{key}: {ticket}");
    }
    assert_eq!(ticket["occurrences"], 1);
    let id = ticket["id"].as_str().unwrap().to_string();
    let (_, plain, _) = run(d, None, false, &["new", "plain"]);
    let plain = json(&plain);
    assert!(
        plain["machine"].is_null() && plain.get("machine").is_some(),
        "{plain}"
    );

    let ids = |args: &[&str]| -> Vec<String> {
        let (code, out, _) = run(d, None, false, args);
        assert_eq!(code, EXIT_OK, "{out}");
        json(&out)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        ids(&["ls", "--machine", "dabo", "--env", "prod"]),
        [id.clone()]
    );
    assert_eq!(ids(&["ls", "--fingerprint", "fp-1"]), [id.clone()]);
    assert!(ids(&["ls", "--machine", "other"]).is_empty());
    assert_eq!(ids(&["ls"]).len(), 2);

    let (code, out, _) = run(
        d,
        None,
        false,
        &[
            "set",
            &id,
            "--version",
            "1",
            "--env",
            "staging",
            "--session",
            "",
        ],
    );
    assert_eq!(code, EXIT_OK, "{out}");
    assert_eq!(json(&out)["environment"], "staging");
    assert!(json(&out)["session"].is_null(), "an empty anchor clears it");
    assert_eq!(ids(&["ls", "--env", "staging"]), [id.clone()]);

    let (code, out, _) = run(d, None, false, &["new", "qa only", "--env", "qa"]);
    assert_ne!(code, EXIT_OK);
    let error = &json(&out)["error"];
    assert_eq!(error["code"], "unknown_environment");
    assert!(
        error["next"]
            .as_str()
            .unwrap()
            .contains("dev, staging, prod"),
        "{error}"
    );
}
