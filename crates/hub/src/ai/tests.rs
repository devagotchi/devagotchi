use super::*;

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, Journal) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("checkout");
    fs::create_dir(&repo).unwrap();
    let repo = repo.canonicalize().unwrap();
    let state = temp.path().join("private");
    let journal = Journal::open(&repo, &state).unwrap();
    (temp, repo, state, journal)
}

fn enable(store: &mut Journal, capture: Capture) {
    let previous = store.policy().unwrap();
    store
        .set_policy(
            &previous,
            &Policy {
                capture,
                content_acknowledged: capture == Capture::Content,
                operator: Some("test-operator".into()),
                ..Policy::default()
            },
        )
        .unwrap();
}

fn event(id: &str) -> AiCaptureEvent {
    AiCaptureEvent {
        schema_version: 1,
        event_id: id.into(),
        session_id: "session-1".into(),
        harness: AiHarness::Manual,
        kind: AiEventKind::Checkpoint,
        model: None,
        issue: Some("DG-42".into()),
        content: None,
    }
}

#[test]
fn managed_defaults_and_advanced_acknowledgment_are_explicit() {
    let default = Policy::default();
    assert_eq!(default.capture, Capture::Off);
    assert!(default.permits("push", true, false).is_ok());
    assert!(default.permits("push", false, true).is_err());
    assert!(default.permits("merge", true, false).is_err());
    assert!(default.permits("merge", true, true).is_ok());
    assert!(default.permits("deploy", true, true).is_err());
    let mut advanced = Policy {
        mode: Mode::Advanced,
        merge: Permission::Validated,
        ..default
    };
    assert!(advanced.validate().is_err());
    advanced.advanced_acknowledged = true;
    assert!(advanced.permits("merge", true, false).is_ok());
    advanced.mode = Mode::Managed;
    assert!(advanced.validate().is_err());
    advanced.mode = Mode::Advanced;
    advanced.push = Permission::Disabled;
    assert!(advanced.permits("push", true, true).is_err());
    advanced.capture = Capture::Content;
    assert!(advanced.validate().is_err());
}

#[test]
fn no_capture_before_opt_in_and_no_content_in_metadata() {
    let (_temp, repo, state, mut store) = setup();
    assert!(store.append(&event("one")).is_err());
    assert_eq!(store.verify().unwrap(), 0);
    enable(&mut store, Capture::Metadata);
    let raw = json!({
        "session_id":"abc","hook_event_name":"PostToolUse","cwd":repo,
        "model":"test-model","tool_input":{"password":"NOT-TO-BE-STORED"},
        "tool_response":"private output","transcript_path":"private-location"
    });
    let normalized = normalize(
        AiHarness::ClaudeCode,
        "PostToolUse",
        raw,
        &repo,
        Capture::Metadata,
    )
    .unwrap();
    store.append(&normalized).unwrap();
    let export = serde_json::to_string(&store.page(0, 100).unwrap()).unwrap();
    for private in [
        "NOT-TO-BE-STORED",
        "private output",
        "private-location",
        "tool_input",
        "cwd",
    ] {
        assert!(!export.contains(private), "{export}");
    }
    let mut with_content = event("content");
    with_content.content = Some(json!({"secret":"do not admit"}));
    assert!(store.append(&with_content).is_err());
    assert_eq!(store.verify().unwrap(), 1);
    drop(store);
    let reopened = Journal::open(&repo, &state).unwrap();
    assert_eq!(reopened.verify().unwrap(), 1);
    assert_eq!(reopened.policy().unwrap().capture, Capture::Metadata);
}

#[test]
fn durable_replay_conflict_paging_and_revocation() {
    let (_temp, repo, state, mut store) = setup();
    enable(&mut store, Capture::Content);
    let mut first = event("one");
    first.content = Some(json!({"rationale":"visible decision", "exit_code":0}));
    assert_eq!(store.append(&first).unwrap()["sequence"], 1);
    assert_eq!(store.append(&first).unwrap()["replayed"], true);
    let mut changed = first.clone();
    changed.content = Some(json!({"changed":true}));
    assert!(
        store
            .append(&changed)
            .unwrap_err()
            .to_string()
            .contains("different content")
    );
    store.append(&event("two")).unwrap();
    assert_eq!(store.page(0, 1).unwrap().len(), 1);
    assert_eq!(store.page(1, 1).unwrap()[0]["sequence"], 2);
    for row in store.page(0, 100).unwrap() {
        assert_eq!(row["coverage"], "partial");
        assert_eq!(row["training_eligible"], false);
        assert_eq!(row["redaction"], "not_performed");
    }
    let mut another = Journal::open(&repo, &state).unwrap();
    enable(&mut store, Capture::Off);
    assert!(another.append(&event("three")).is_err());
    assert_eq!(another.verify().unwrap(), 2);
    let audit: i64 = store
        .db
        .query_row("SELECT count(*) FROM policy_audit", [], |r| r.get(0))
        .unwrap();
    assert_eq!(audit, 2);
}

#[test]
fn all_configured_provider_events_translate_without_model_vendor_guessing() {
    let (_temp, repo, _, _) = setup();
    let expected = [
        AiEventKind::SessionStart,
        AiEventKind::UserPrompt,
        AiEventKind::ToolBefore,
        AiEventKind::ToolAfter,
        AiEventKind::TurnEnd,
        AiEventKind::SessionEnd,
    ];
    for harness in [AiHarness::ClaudeCode, AiHarness::Codex, AiHarness::Copilot] {
        let names = if harness == AiHarness::Copilot {
            [
                "sessionStart",
                "userPromptSubmitted",
                "preToolUse",
                "postToolUse",
                "agentStop",
                "sessionEnd",
            ]
        } else {
            [
                "SessionStart",
                "UserPromptSubmit",
                "PreToolUse",
                "PostToolUse",
                "Stop",
                "SessionEnd",
            ]
        };
        for (name, kind) in names.into_iter().zip(expected) {
            let raw = if harness == AiHarness::Copilot {
                json!({"sessionId":"copilot-1","cwd":repo,"model":"claude-in-copilot","timestamp":1794300000000u64})
            } else {
                json!({"session_id":"native-1","hook_event_name":name,"cwd":repo,"model":"native-model"})
            };
            let normalized =
                normalize(harness, name, raw.clone(), &repo, Capture::Content).unwrap();
            assert_eq!(normalized.kind, kind);
            assert_eq!(normalized.harness, harness);
            assert_eq!(normalized.content, Some(raw));
        }
    }
    assert_eq!(
        normalize(
            AiHarness::Copilot,
            "errorOccurred",
            json!({"sessionId":"c"}),
            &repo,
            Capture::Metadata
        )
        .unwrap()
        .kind,
        AiEventKind::Error
    );
}

#[test]
fn provider_payload_mismatches_and_outside_cwd_are_explicit_errors() {
    let (temp, repo, _, _) = setup();
    for (harness, name, raw) in [
        (
            AiHarness::Copilot,
            "sessionStart",
            json!({"session_id":"wrong-schema"}),
        ),
        (
            AiHarness::Copilot,
            "SessionStart",
            json!({"sessionId":"wrong-event"}),
        ),
        (
            AiHarness::ClaudeCode,
            "SessionStart",
            json!({"session_id":"x","hook_event_name":"Stop"}),
        ),
        (
            AiHarness::Codex,
            "SessionStart",
            json!({"session_id":"x","cwd":temp.path()}),
        ),
        (AiHarness::Manual, "SessionStart", json!({"session_id":"x"})),
        (
            AiHarness::Codex,
            "Stop",
            json!({"session_id":"x","cwd":"relative"}),
        ),
        (
            AiHarness::Codex,
            "Stop",
            json!({"session_id":"x","model":false}),
        ),
    ] {
        assert!(normalize(harness, name, raw, &repo, Capture::Metadata).is_err());
    }
}

#[test]
fn replay_scope_is_harness_and_session_and_native_invocations_are_not_falsely_deduplicated() {
    let (_temp, repo, _, mut store) = setup();
    enable(&mut store, Capture::Metadata);
    for (harness, session) in [
        (AiHarness::Codex, "s"),
        (AiHarness::Copilot, "s"),
        (AiHarness::Codex, "other"),
    ] {
        let mut item = event("same-id");
        item.harness = harness;
        item.session_id = session.into();
        store.append(&item).unwrap();
    }
    for _ in 0..2 {
        let item = normalize(
            AiHarness::Codex,
            "Stop",
            json!({"session_id":"s"}),
            &repo,
            Capture::Metadata,
        )
        .unwrap();
        store.append(&item).unwrap();
    }
    assert_eq!(store.verify().unwrap(), 5);
}

#[test]
fn bounded_inputs_schema_and_invalid_event_do_not_create_receipts() {
    let (_temp, _, _, mut store) = setup();
    enable(&mut store, Capture::Content);
    for mut item in [event(""), event("bad\nid"), event("big")] {
        if item.event_id == "big" {
            item.content = Some(json!("x".repeat(LIMIT)));
        }
        assert!(store.append(&item).is_err());
    }
    let mut bad = event("version");
    bad.schema_version = 2;
    assert!(store.append(&bad).is_err());
    assert!(read_json(vec![b' '; LIMIT + 1].as_slice()).is_err());
    assert!(read_json(b"{oops".as_slice()).is_err());
    assert!(serde_json::from_value::<AiCaptureEvent>(json!({"surprise":true})).is_err());
    assert_eq!(store.verify().unwrap(), 0);
    let schema: Value = serde_json::from_str(&schema().unwrap()).unwrap();
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn incremental_import_retries_partial_tail_rejects_rewrites_and_preserves_prior_records() {
    let (temp, _, _, mut store) = setup();
    enable(&mut store, Capture::Content);
    let source = temp.path().join("chosen-provider-export.jsonl");
    fs::write(&source, b"{\"visible\":\"one\"}\n{\"visible\":").unwrap();
    assert_eq!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .unwrap(),
        1
    );
    assert_eq!(store.verify().unwrap(), 1);
    let imported = store.page(0, 1).unwrap();
    assert_eq!(imported[0]["event"]["content"]["record"]["visible"], "one");
    assert_eq!(
        imported[0]["event"]["content"]["source_sha256"],
        hash(b"{\"visible\":\"one\"}\n")
    );
    fs::write(&source, b"{ \"visible\":\"one\"}\n").unwrap();
    assert!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .is_err()
    );
    fs::write(&source, b"{\"visible\":\"one\"}\n{\"visible\":\"two\"}\n").unwrap();
    assert_eq!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .unwrap(),
        2
    );
    assert_eq!(store.verify().unwrap(), 2);
    fs::write(
        &source,
        b"{\"visible\":\"one\"}\n{\"visible\":\"two\"}\nnot-json\n",
    )
    .unwrap();
    assert!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .is_err()
    );
    assert_eq!(store.verify().unwrap(), 2);
    fs::write(&source, b"{\"visible\":\"changed\"}\n").unwrap();
    assert!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .unwrap_err()
            .to_string()
            .contains("different content")
    );
    enable(&mut store, Capture::Metadata);
    assert!(
        store
            .import(AiHarness::Codex, "native", &source, false)
            .is_err()
    );
    assert_eq!(store.verify().unwrap(), 2);
}

#[test]
fn transcript_paths_require_specific_canonical_authorization() {
    let (temp, _, _, mut store) = setup();
    let root = temp.path().join("provider-project");
    fs::create_dir(&root).unwrap();
    let source = root.join("session.jsonl");
    fs::write(&source, b"{}\n").unwrap();
    let roots = vec![root.canonicalize().unwrap()];
    assert!(authorize_transcript(&source, &roots).is_ok());
    assert!(authorize_transcript(&source, &[]).is_err());
    let outside = temp.path().join("unrelated.jsonl");
    fs::write(&outside, b"{}\n").unwrap();
    assert!(authorize_transcript(&outside, &roots).is_err());
    assert!(authorize_transcript(Path::new("relative.jsonl"), &roots).is_err());
    enable(&mut store, Capture::Content);
    assert!(store.import(AiHarness::Codex, "s", &root, false).is_err());

    let mut policy = store.policy().unwrap();
    let previous = policy.clone();
    policy.transcript_roots = roots;
    store.set_policy(&previous, &policy).unwrap();
    assert_eq!(
        store
            .import(AiHarness::Codex, "automatic", &source, true)
            .unwrap(),
        1
    );
    let previous = policy.clone();
    policy.transcript_roots.clear();
    store.set_policy(&previous, &policy).unwrap();
    assert!(
        store
            .import(AiHarness::Codex, "automatic", &source, true)
            .is_err()
    );
    assert!(
        store
            .append_authorized(&event("after-revocation"), Some(&source))
            .is_err()
    );
}

#[test]
fn stale_operator_updates_cannot_overwrite_revocation() {
    let (_temp, repo, state, mut store) = setup();
    enable(&mut store, Capture::Content);
    let stale = store.policy().unwrap();
    let mut another = Journal::open(&repo, &state).unwrap();
    enable(&mut store, Capture::Off);
    assert!(another.set_policy(&stale, &stale).is_err());
    assert_eq!(another.policy().unwrap().capture, Capture::Off);
}

#[test]
fn lock_contention_fails_explicitly_with_a_bounded_wait_and_no_mutation() {
    let (_temp, repo, state, mut store) = setup();
    enable(&mut store, Capture::Metadata);
    let mut another = Journal::open(&repo, &state).unwrap();
    let tx = store
        .db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let start = std::time::Instant::now();
    let error = another.append(&event("blocked")).unwrap_err();
    assert!(error.to_string().contains("locked"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(4));
    tx.rollback().unwrap();
    assert_eq!(another.verify().unwrap(), 0);
    another.append(&event("blocked")).unwrap();
    assert_eq!(another.verify().unwrap(), 1);
}

#[test]
fn concurrent_writers_and_replays_survive_reopen() {
    let (_temp, repo, state, mut store) = setup();
    enable(&mut store, Capture::Metadata);
    let mut threads = Vec::new();
    for worker in 0..4 {
        let repo = repo.clone();
        let state = state.clone();
        threads.push(std::thread::spawn(move || {
            let mut store = Journal::open(&repo, &state).unwrap();
            for n in 0..10 {
                store
                    .append(&event(&format!("worker-{worker}-{n}")))
                    .unwrap();
                store.append(&event("shared-retry")).unwrap();
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    drop(store);
    let reopened = Journal::open(&repo, &state).unwrap();
    assert_eq!(reopened.verify().unwrap(), 41);
}

#[test]
fn archive_detects_mutation_and_refuses_in_repo_storage() {
    let (_temp, repo, _, mut store) = setup();
    assert!(Journal::open(&repo, &repo.join("private")).is_err());
    enable(&mut store, Capture::Metadata);
    store.append(&event("one")).unwrap();
    store.append(&event("two")).unwrap();
    store
        .db
        .execute(
            "UPDATE events SET recorded_ms=recorded_ms+1 WHERE seq=1",
            [],
        )
        .unwrap();
    assert!(store.verify().is_err());
}

#[test]
fn integrity_binds_dedupe_keys_and_export_sequence_to_the_event() {
    for sql in [
        "UPDATE events SET harness='\"copilot\"'",
        "UPDATE events SET session='other-session'",
        "UPDATE events SET event_id='other-id'",
        "UPDATE events SET seq=10",
    ] {
        let (_temp, _, _, mut store) = setup();
        enable(&mut store, Capture::Metadata);
        store.append(&event("one")).unwrap();
        store.db.execute(sql, []).unwrap();
        assert!(store.verify().is_err(), "{sql}");
    }
}

#[test]
fn archive_and_import_limits_refuse_foreign_checkouts_and_oversized_sources() {
    let (temp, repo, _, mut store) = setup();
    let other = temp.path().join("other-checkout");
    fs::create_dir_all(other.join(".git")).unwrap();
    assert!(Journal::open(&repo, &other.join("private")).is_err());
    enable(&mut store, Capture::Content);
    let oversized = temp.path().join("oversized.jsonl");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(IMPORT_LIMIT + 1)
        .unwrap();
    assert!(
        store
            .import(AiHarness::Manual, "large", &oversized, false)
            .is_err()
    );
    let large_line = temp.path().join("oversized-record.jsonl");
    fs::write(&large_line, vec![b'x'; LIMIT + 1]).unwrap();
    assert!(
        store
            .import(AiHarness::Manual, "large", &large_line, false)
            .is_err()
    );
    assert_eq!(store.verify().unwrap(), 0);
}

#[test]
fn lifecycle_promotion_and_policy_schema_drift_require_explicit_reconciliation() {
    let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    for file in [
        ".ai/project.json",
        ".ai/docs/capture-event.schema.json",
        "AGENTS.md",
        "CLAUDE.md",
        ".github/copilot-instructions.md",
        ".ai/docs/lifecycle.md",
        ".ai/docs/capture.md",
        ".ai/docs/adapters.md",
        ".ai/docs/evaluations.json",
        ".ai/dev/workflow.md",
        ".ai/dev/plans/TEMPLATE.md",
        ".ai/patterns/README.md",
    ] {
        let destination = repo.join(file);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(actual.join(file), destination).unwrap();
    }
    check_contracts(repo).unwrap();
    let file = repo.join(".ai/project.json");
    let original: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let mut project = original.clone();
    project["stage"] = json!("production");
    fs::write(&file, serde_json::to_vec(&project).unwrap()).unwrap();
    assert!(check_contracts(repo).is_err());
    project["production_promotion"] = json!({
        "approval":"https://example.invalid/review/1","candidate":"sha256:candidate",
        "evidence":"https://example.invalid/evidence/1","date":"2026-11-10"
    });
    fs::write(&file, serde_json::to_vec(&project).unwrap()).unwrap();
    check_contracts(repo).unwrap();
    project = original.clone();
    project["policy_defaults"]["merge"] = json!("validated");
    fs::write(&file, serde_json::to_vec(&project).unwrap()).unwrap();
    assert!(
        check_contracts(repo)
            .unwrap_err()
            .to_string()
            .contains("default drift")
    );
    fs::write(&file, serde_json::to_vec(&original).unwrap()).unwrap();
    fs::write(repo.join(".ai/docs/capture-event.schema.json"), b"{}").unwrap();
    assert!(
        check_contracts(repo)
            .unwrap_err()
            .to_string()
            .contains("schema drift")
    );
}

#[test]
fn option_parser_rejects_duplicates_unknowns_and_missing_values() {
    for args in [
        vec!["--mode"],
        vec!["--mode", "--approved"],
        vec!["--mode", "managed", "--mode", "advanced"],
        vec!["--other", "x"],
        vec!["bare"],
    ] {
        assert!(options(&args, &["mode"], &["approved"]).is_err());
    }
    assert!(
        options(
            &["--mode", "managed", "--approved"],
            &["mode"],
            &["approved"]
        )
        .is_ok()
    );
}

#[cfg(unix)]
#[test]
fn unix_archive_is_private_and_symlink_paths_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (temp, repo, _, store) = setup();
    assert_eq!(
        fs::metadata(&store.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(store.path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let outside = temp.path().join("actual.jsonl");
    fs::write(&outside, b"{}\n").unwrap();
    let linked = repo.join("linked.jsonl");
    symlink(&outside, &linked).unwrap();
    assert!(authorize_transcript(&linked, &[repo]).is_err());
}
