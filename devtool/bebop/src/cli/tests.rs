use std::path::Path;

use super::*;
use crate::checks::tests::{repo, write};

fn bebop(args: &[&str], cwd: &Path) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&args, cwd, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn a_violation_exits_1_and_a_clean_tree_exits_0() {
    let dir = repo();
    let (code, out, _) = bebop(&["check", "--format", "xml"], dir.path());
    assert_eq!(code, FAILED);
    assert!(
        out.starts_with("<bebop-report rules=\"2\" violations=\"2\">"),
        "{out}"
    );
    write(dir.path(), "src/long.rs", "1\n");
    write(dir.path(), "src/site/assets_tests.rs", "");
    std::fs::remove_file(dir.path().join("src/site/assets_tests.rs")).unwrap();
    let (code, out, _) = bebop(&["check", "--all"], dir.path());
    assert_eq!(code, CLEAN, "{out}");
    assert_eq!(out, "bebop: 2 rules checked, no violations.\n");
}

#[test]
fn the_repository_is_found_from_a_folder_inside_it() {
    let dir = repo();
    let (code, out, _) = bebop(&["check", "--changed", "long.rs"], &dir.path().join("src"));
    assert_eq!(code, FAILED);
    assert!(out.contains("  - src/long.rs:4\n"), "{out}");
    let (code, _, _) = bebop(&["check", "--changed", "ok.rs"], &dir.path().join("src"));
    assert_eq!(code, CLEAN);
}

#[test]
fn a_malformed_rule_exits_2_and_names_the_file_and_field() {
    let dir = repo();
    write(
        dir.path(),
        ".ai/rules/layout/broken/rule.toml",
        "id = \"layout.broken\"\n",
    );
    let (code, _, err) = bebop(&["check"], dir.path());
    assert_eq!(code, BROKEN);
    assert!(err.contains(".ai/rules/layout/broken/rule.toml"), "{err}");
    assert!(err.contains("missing field"), "{err}");
}

#[test]
fn a_usage_error_exits_2_and_says_what_to_run() {
    let dir = repo();
    for args in [
        &["check", "--all", "--changed", "a"][..],
        &["check", "--format", "json"],
        &["check", "--changed"],
        &["lint"],
        &["baseline", "seed"],
        &["rule", "test", "a", "b"],
    ] {
        let (code, _, err) = bebop(args, dir.path());
        assert_eq!(code, BROKEN, "{args:?}");
        assert!(err.contains("bebop help"), "{args:?}: {err}");
    }
    let outside = tempfile::tempdir().unwrap();
    let (code, _, err) = bebop(&["check"], outside.path());
    assert_eq!(code, BROKEN);
    assert!(err.contains("give --root"), "{err}");
    let (code, out, _) = bebop(&["help"], outside.path());
    assert_eq!(code, CLEAN);
    assert!(out.contains("bebop baseline seed <rule-id>"));
}

#[test]
fn rule_test_reports_each_rule_and_fails_on_a_bad_example() {
    let dir = repo();
    let (code, out, _) = bebop(&["rule", "test"], dir.path());
    assert_eq!(code, CLEAN, "{out}");
    assert!(
        out.contains("ok    layout.size: bad flagged 1, good clean\n"),
        "{out}"
    );
    assert!(
        out.ends_with("bebop rule test: 2 rules, 0 failed\n"),
        "{out}"
    );
    write(
        dir.path(),
        ".ai/rules/layout/size/examples/good/src/big.rs",
        "1\n2\n3\n4\n",
    );
    let (code, out, _) = bebop(&["rule", "test", "layout.size"], dir.path());
    assert_eq!(code, FAILED);
    assert!(out.contains("FAIL  layout.size\n"), "{out}");
    let (code, _, err) = bebop(&["rule", "test", "layout.nothing"], dir.path());
    assert_eq!(code, BROKEN);
    assert!(err.contains("no rule has the id"), "{err}");
}

#[test]
fn baseline_seed_and_tighten_from_the_command_line() {
    let dir = repo();
    let (code, out, _) = bebop(&["baseline", "seed", "layout.size"], dir.path());
    assert_eq!(code, CLEAN);
    assert_eq!(
        out,
        "layout.size: seeded .ai/rules/layout/size/baseline.txt with 1 entry\n"
    );
    let (code, _, err) = bebop(&["baseline", "seed", "layout.size"], dir.path());
    assert_eq!(code, BROKEN);
    assert!(err.contains("bebop baseline tighten layout.size"), "{err}");
    write(dir.path(), "src/long.rs", "1\n2\n3\n4\n");
    let (code, out, _) = bebop(&["baseline", "tighten"], dir.path());
    assert_eq!(code, CLEAN);
    assert_eq!(out, "layout.size: 0 entries removed, 1 lowered, 1 left\n");
}
