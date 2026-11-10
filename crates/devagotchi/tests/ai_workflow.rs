//! Native CLI integration: no board, account, shell JSON parsing or network required.
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn run(repo: &Path, state: &Path, args: &[&str], input: Option<Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_devagotchi"))
        .args(["ai"])
        .args(args)
        .current_dir(repo)
        .env("DEVAGOTCHI_STATE_DIR", state)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&input).unwrap())
            .unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn operator_controls_and_native_provider_capture_work_without_a_board() {
    let temp = tempfile::Builder::new()
        .prefix("AI native workflow spaces ")
        .tempdir()
        .unwrap();
    let repo = temp.path().join("checkout");
    let state = temp.path().join("private archive");
    std::fs::create_dir_all(repo.join(".ai")).unwrap();
    std::fs::write(repo.join(".ai/project.json"), b"{}").unwrap();
    let shown = ok(run(&repo, &state, &["policy", "show"], None));
    assert_eq!(shown["policy"]["mode"], "managed");
    assert_eq!(shown["policy"]["merge"], "approval");
    assert_eq!(shown["policy"]["capture"], "off");
    let disabled = run(
        &repo,
        &state,
        &["hook", "copilot", "sessionStart"],
        Some(json!({"sessionId":"copilot-1","cwd":repo,"initialPrompt":"sensitive"})),
    );
    assert!(disabled.status.success());
    assert!(String::from_utf8_lossy(&disabled.stderr).contains("capture OFF"));
    assert_eq!(
        ok(run(&repo, &state, &["capture", "status"], None))["events"],
        0
    );
    for args in [
        vec!["policy", "set", "--operator", "owner", "--mode", "advanced"],
        vec![
            "policy",
            "set",
            "--operator",
            "owner",
            "--merge",
            "validated",
        ],
        vec![
            "policy",
            "set",
            "--operator",
            "owner",
            "--capture",
            "content",
        ],
        vec!["policy", "check", "--action", "push"],
        vec!["policy", "check", "--action", "merge", "--validated"],
        vec![
            "policy",
            "set",
            "--operator",
            "owner",
            "--capture",
            "metadata",
            "--capture",
            "content",
        ],
    ] {
        assert!(
            !run(&repo, &state, &args, None).status.success(),
            "{args:?}"
        );
    }
    for args in [
        vec!["policy", "check", "--action", "push", "--validated"],
        vec![
            "policy",
            "check",
            "--action",
            "merge",
            "--validated",
            "--approved",
        ],
    ] {
        assert!(run(&repo, &state, &args, None).status.success());
    }
    ok(run(
        &repo,
        &state,
        &[
            "policy",
            "set",
            "--operator",
            "owner",
            "--capture",
            "metadata",
        ],
        None,
    ));
    for (harness, name, key) in [
        ("claude-code", "SessionStart", "session_id"),
        ("codex", "Stop", "session_id"),
        ("copilot", "postToolUse", "sessionId"),
    ] {
        let mut raw = json!({"cwd":repo,"secret":"NEVER-CAPTURE-IN-METADATA"});
        raw[key] = json!(format!("{harness}-session"));
        let output = run(&repo, &state, &["hook", harness, name], Some(raw));
        assert_eq!(ok(output), json!({}));
    }
    let exported = run(&repo, &state, &["capture", "export"], None);
    let text = String::from_utf8(exported.stdout).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(!text.contains("NEVER-CAPTURE"));
    for line in text.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        assert!(row["event"]["content"].is_null());
        assert_eq!(row["training_eligible"], false);
    }
    ok(run(
        &repo,
        &state,
        &[
            "policy",
            "set",
            "--operator",
            "owner",
            "--capture",
            "content",
            "--acknowledge",
            "content-risk",
        ],
        None,
    ));
    let source_dir = temp.path().join("authorized provider transcript");
    std::fs::create_dir(&source_dir).unwrap();
    let source = source_dir.join("native.jsonl");
    std::fs::write(
        &source,
        b"{\"role\":\"user\",\"text\":\"authorized visible prompt\"}\n",
    )
    .unwrap();
    let configured = ok(run(
        &repo,
        &state,
        &[
            "policy",
            "set",
            "--operator",
            "owner",
            "--transcript-root",
            source_dir.to_str().unwrap(),
        ],
        None,
    ));
    assert_eq!(configured["transcript_roots"].as_array().unwrap().len(), 1);
    let stop = || {
        run(
            &repo,
            &state,
            &["hook", "codex", "Stop"],
            Some(json!({"session_id":"codex-session","transcript_path":source,"cwd":repo})),
        )
    };
    ok(stop());
    ok(stop());
    assert_eq!(
        ok(run(&repo, &state, &["capture", "verify"], None))["verified_events"],
        6
    );
    ok(run(
        &repo,
        &state,
        &["hook", "copilot", "agentStop"],
        Some(json!({
            "sessionId":"copilot-1", "cwd":repo,
            "transcript_path":temp.path().join("not-a-copilot-contract.jsonl")
        })),
    ));
    assert!(
        !run(
            &repo,
            &state,
            &["hook", "codex", "Stop"],
            Some(json!({"session_id":"codex-session","transcript_path":true})),
        )
        .status
        .success()
    );
    let exported = run(
        &repo,
        &state,
        &["capture", "export", "--after", "3", "--limit", "1"],
        None,
    );
    let first: Value = serde_json::from_slice(&exported.stdout).unwrap();
    assert_eq!(first["sequence"], 4);
    ok(run(
        &repo,
        &state,
        &[
            "policy",
            "set",
            "--operator",
            "owner",
            "--clear-transcript-roots",
        ],
        None,
    ));
    assert_eq!(
        ok(run(&repo, &state, &["policy", "show"], None))["policy"]["transcript_roots"],
        json!([])
    );
    ok(run(
        &repo,
        &state,
        &[
            "policy",
            "set",
            "--operator",
            "owner",
            "--mode",
            "advanced",
            "--merge",
            "validated",
            "--acknowledge",
            "advanced",
        ],
        None,
    ));
    assert!(
        run(
            &repo,
            &state,
            &["policy", "check", "--action", "merge", "--validated"],
            None
        )
        .status
        .success()
    );
    let managed = ok(run(
        &repo,
        &state,
        &[
            "policy",
            "set",
            "--operator",
            "owner",
            "--mode",
            "managed",
            "--capture",
            "metadata",
        ],
        None,
    ));
    assert_eq!(managed["merge"], "approval");
    assert_eq!(managed["advanced_acknowledged"], false);
    assert_eq!(managed["content_acknowledged"], false);
    assert!(
        !run(
            &repo,
            &state,
            &[
                "policy",
                "set",
                "--operator",
                "owner",
                "--capture",
                "content"
            ],
            None
        )
        .status
        .success()
    );
    assert!(!repo.join(".devagotchi").exists());
    assert!(!repo.join("journal.sqlite3").exists());
}

#[test]
fn committed_contract_and_all_hook_stubs_match_the_binary_adapter() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let checked = run(repo, temp.path(), &["check"], None);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    for (file, harness, grouped) in [
        (".claude/settings.json", "claude-code", true),
        (".codex/hooks.json", "codex", true),
        (".github/hooks/session-capture.json", "copilot", false),
    ] {
        let config: Value =
            serde_json::from_slice(&std::fs::read(repo.join(file)).unwrap()).unwrap();
        if !grouped {
            assert_eq!(config["version"], 1);
        }
        for (native, groups) in config["hooks"].as_object().unwrap() {
            for group in groups.as_array().unwrap() {
                let handlers = if grouped {
                    group["hooks"].as_array().unwrap().clone()
                } else {
                    vec![group.clone()]
                };
                for handler in handlers {
                    let expected = format!("devagotchi ai hook {harness} {native}");
                    assert_eq!(handler["command"], expected);
                    assert_eq!(handler["type"], "command");
                    let key = if harness == "copilot" {
                        "sessionId"
                    } else {
                        "session_id"
                    };
                    let mut raw = json!({"cwd":repo});
                    raw[key] = json!("config-fixture");
                    assert_eq!(
                        ok(run(
                            repo,
                            temp.path(),
                            &["hook", harness, native],
                            Some(raw)
                        )),
                        json!({})
                    );
                }
            }
        }
    }
}

#[test]
fn delivery_reports_only_local_git_facts_and_cannot_claim_a_remote_merge() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("checkout");
    std::fs::create_dir_all(repo.join(".ai")).unwrap();
    std::fs::write(repo.join(".ai/project.json"), b"{}").unwrap();
    for args in [
        vec!["init", "--initial-branch=main"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(&args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = run(
        &repo,
        &temp.path().join("private"),
        &["delivery", "status"],
        None,
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("DELIVERY INCOMPLETE"));
    let status = ok(output);
    assert_eq!(status["branch"], "main");
    assert_eq!(status["uncommitted"], true);
    assert_eq!(status["remote_checked"], false);
    assert_eq!(status["main"], "not_verified");
    assert_eq!(status["pr"], "not_verified");
    assert!(!temp.path().join("private").exists());
}
