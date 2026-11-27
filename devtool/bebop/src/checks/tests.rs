use std::{fs, path::Path};

use super::*;

pub(crate) fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn rule_toml(category: &str, name: &str, check: &str, include: &str, example: &str) -> String {
    format!(
        "id = \"{category}.{name}\"\ncategory = \"{category}\"\nstatement = \"The statement.\"\n\
         why = \"The reason.\"\nsources = [\"https://example.org/practice\"]\nseverity = \"error\"\n\
         status = \"enforced\"\nversion = 1\n\n[scope]\ninclude = [{include}]\n\n[check]\n{check}\n\n\
         [guidance]\nexpected = \"{{file}} keeps the rule\"\nfix = \"Correct {{file}}\"\n\
         example = \"{example}\"\n\n[approval]\nby = \"operator\"\ndate = \"2026-11-26\"\n"
    )
}

/// A small repository with two enforced rules: a 3-line cap on `src/**/*.rs` and no
/// `*_tests.rs`, each with examples, and a tree that breaks both.
pub(crate) fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "src/ok.rs", "1\n2\n");
    write(root, "src/long.rs", "1\n2\n3\n4\n5\n");
    write(root, "src/site/assets_tests.rs", "");
    let size = ".ai/rules/layout/size";
    write(
        root,
        &format!("{size}/rule.toml"),
        &rule_toml(
            "layout",
            "size",
            "engine = \"max_lines\"\nmax = 3",
            "\"src/**/*.rs\"",
            "src/ok.rs",
        ),
    );
    write(
        root,
        &format!("{size}/examples/bad/src/big.rs"),
        "1\n2\n3\n4\n",
    );
    write(root, &format!("{size}/examples/good/src/small.rs"), "1\n");
    let names = ".ai/rules/layout/test-names";
    write(
        root,
        &format!("{names}/rule.toml"),
        &rule_toml(
            "layout",
            "test-names",
            "engine = \"forbidden_file_name\"\npatterns = [\"*_tests.rs\"]",
            "\"**/*.rs\"",
            "src/ok.rs",
        ),
    );
    write(root, &format!("{names}/examples/bad/a_tests.rs"), "");
    write(root, &format!("{names}/examples/good/tests.rs"), "");
    dir
}

fn report(root: &Path, selection: &Selection) -> Report {
    check(&Workspace::load(root).unwrap(), selection).unwrap()
}

fn files(report: &Report) -> Vec<(&str, &str)> {
    report
        .violations
        .iter()
        .map(|v| (v.rule.as_str(), v.file.as_str()))
        .collect()
}

#[test]
fn every_enforced_rule_reports_its_violations_with_its_guidance() {
    let dir = repo();
    let report = report(dir.path(), &Selection::All);
    assert_eq!(report.rules, 2);
    assert_eq!(
        files(&report),
        [
            ("layout.size", "src/long.rs"),
            ("layout.test-names", "src/site/assets_tests.rs"),
        ]
    );
    let long = &report.violations[0];
    assert_eq!(long.line, Some(4));
    assert_eq!(long.expected, "src/long.rs keeps the rule");
    assert_eq!(long.fix, "Correct src/long.rs");
    assert_eq!(long.statement, "The statement.");
    assert_eq!(long.example, "src/ok.rs");
}

#[test]
fn a_baseline_holds_todays_violations_and_only_shrinks() {
    let dir = repo();
    let root = dir.path();
    write(
        root,
        ".ai/rules/layout/size/baseline.txt",
        "# h\n5\tsrc/long.rs\n",
    );
    write(
        root,
        ".ai/rules/layout/test-names/baseline.txt",
        "src/gone_tests.rs\nsrc/site/assets_tests.rs\n",
    );
    let found = report(root, &Selection::All);
    assert_eq!(
        files(&found),
        [(
            "layout.test-names",
            ".ai/rules/layout/test-names/baseline.txt"
        )]
    );
    let stale = &found.violations[0];
    assert_eq!(stale.line, Some(1));
    assert_eq!(stale.subject, "src/gone_tests.rs");
    assert!(
        stale.fix.contains("remove its line on line 1"),
        "{}",
        stale.fix
    );

    write(root, "src/long.rs", "1\n2\n3\n4\n5\n6\n");
    let grown = report(root, &Selection::All);
    assert!(
        grown
            .violations
            .iter()
            .any(|v| v.file == "src/long.rs" && v.line == Some(6)),
        "{grown:?}"
    );
}

#[test]
fn changed_keeps_the_violations_about_the_changed_paths() {
    let dir = repo();
    let root = dir.path();
    let only = |paths: &[&str]| {
        let selection = Selection::Changed(paths.iter().map(|p| (*p).to_owned()).collect());
        files(&report(root, &selection))
            .into_iter()
            .map(|(rule, file)| format!("{rule} {file}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(only(&["src/long.rs"]), ["layout.size src/long.rs"]);
    assert_eq!(
        only(&["src/site"]),
        ["layout.test-names src/site/assets_tests.rs"]
    );
    assert!(only(&["src/ok.rs"]).is_empty());
    assert_eq!(
        only(&[".ai/rules/layout/size/rule.toml"]),
        ["layout.size src/long.rs"]
    );
}

#[test]
fn a_malformed_baseline_stops_the_check_and_names_its_line() {
    let dir = repo();
    write(
        dir.path(),
        ".ai/rules/layout/size/baseline.txt",
        "5 src/long.rs\n",
    );
    let workspace = Workspace::load(dir.path()).unwrap();
    let Broken(errors) = check(&workspace, &Selection::All).unwrap_err();
    assert!(
        errors[0].starts_with(".ai/rules/layout/size/baseline.txt: line 1"),
        "{errors:?}"
    );
}

#[test]
fn tighten_lowers_and_drops_and_never_adds() {
    let dir = repo();
    let root = dir.path();
    write(
        root,
        ".ai/rules/layout/size/baseline.txt",
        "# h\n9\tsrc/gone.rs\n9\tsrc/long.rs\n",
    );
    write(
        root,
        ".ai/rules/layout/test-names/baseline.txt",
        "src/fixed_tests.rs\n",
    );
    let said = upkeep::tighten(&Workspace::load(root).unwrap(), None).unwrap();
    assert_eq!(
        said,
        [
            "layout.size: 1 entry removed, 1 lowered, 1 left",
            "layout.test-names: 1 entry removed, 0 lowered, 0 left",
        ]
    );
    assert_eq!(
        fs::read_to_string(root.join(".ai/rules/layout/size/baseline.txt")).unwrap(),
        "# h\n5\tsrc/long.rs\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".ai/rules/layout/test-names/baseline.txt")).unwrap(),
        ""
    );
    let again = upkeep::tighten(&Workspace::load(root).unwrap(), Some("layout.size")).unwrap();
    assert_eq!(again, ["layout.size: unchanged, 1 entry"]);
}

#[test]
fn seed_writes_todays_violations_once() {
    let dir = repo();
    let root = dir.path();
    let workspace = Workspace::load(root).unwrap();
    let said = upkeep::seed(&workspace, "layout.test-names").unwrap();
    assert_eq!(
        said,
        "layout.test-names: seeded .ai/rules/layout/test-names/baseline.txt with 1 entry"
    );
    let text = fs::read_to_string(root.join(".ai/rules/layout/test-names/baseline.txt")).unwrap();
    assert!(text.ends_with("\nsrc/site/assets_tests.rs\n"), "{text}");
    assert!(
        report(root, &Selection::All)
            .violations
            .iter()
            .all(|v| v.rule == "layout.size")
    );
    let Broken(refused) =
        upkeep::seed(&Workspace::load(root).unwrap(), "layout.test-names").unwrap_err();
    assert!(refused[0].contains("already exists"), "{refused:?}");
    let Broken(unknown) = upkeep::seed(&workspace, "layout.nothing").unwrap_err();
    assert!(unknown[0].contains("no rule has the id"), "{unknown:?}");
}

#[test]
fn examples_must_break_and_keep_their_rule() {
    let dir = repo();
    let root = dir.path();
    let outcomes = examples::test_examples(&Workspace::load(root).unwrap(), None).unwrap();
    assert!(outcomes.iter().all(|o| o.passed()), "{outcomes:?}");
    assert_eq!(outcomes[0].bad, 1);

    write(
        root,
        ".ai/rules/layout/size/examples/good/src/big.rs",
        "1\n2\n3\n4\n",
    );
    fs::remove_file(root.join(".ai/rules/layout/test-names/examples/bad/a_tests.rs")).unwrap();
    write(
        root,
        ".ai/rules/layout/test-names/examples/bad/tests.rs",
        "",
    );
    let outcomes = examples::test_examples(&Workspace::load(root).unwrap(), None).unwrap();
    assert!(outcomes[0].problems[0].contains("examples/good/ breaks the rule at src/big.rs"));
    assert!(outcomes[1].problems[0].contains("examples/bad/ breaks nothing"));

    fs::remove_dir_all(root.join(".ai/rules/layout/size/examples/good")).unwrap();
    let outcomes =
        examples::test_examples(&Workspace::load(root).unwrap(), Some("layout.size")).unwrap();
    assert!(outcomes[0].problems[0].contains("examples/good/ is missing"));
}

#[test]
fn the_rules_own_examples_are_not_checked_as_the_repository() {
    let dir = repo();
    let report = report(dir.path(), &Selection::All);
    assert!(
        report
            .violations
            .iter()
            .all(|v| !v.file.contains("examples/")),
        "{report:?}"
    );
}
