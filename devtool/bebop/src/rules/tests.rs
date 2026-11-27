use std::{fs, path::Path};

use super::*;
use crate::tree::Tree;

const GOOD: &str = r#"
id = "layout.file-size-cap"
category = "layout"
statement = "A new file holds at most 500 lines."
why = "Small files are read whole."
sources = ["https://eslint.org/docs/latest/rules/max-lines"]
severity = "error"
status = "enforced"
version = 1

[scope]
include = ["**/*.rs"]
exclude = ["**/target"]

[check]
engine = "max_lines"
max = 500

[guidance]
expected = "{file} holds at most {allowed} lines"
fix = "Split {file} by subject"
example = "src/small.rs"

[approval]
by = "operator"
date = "2026-11-26"
"#;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A repository holding one rule, `rule.toml` as given.
fn repo(rule: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/small.rs", "fn main() {}\n");
    write(dir.path(), ".ai/rules/layout/file-size-cap/rule.toml", rule);
    dir
}

fn errors(rule: &str) -> Vec<String> {
    let dir = repo(rule);
    let tree = Tree::walk(dir.path(), true).unwrap();
    match load(&tree) {
        Ok(rules) => panic!("loaded {} rule(s), expected an error", rules.len()),
        Err(errors) => errors.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn a_well_formed_rule_loads_with_its_folder_and_engine() {
    let dir = repo(GOOD);
    let tree = Tree::walk(dir.path(), true).unwrap();
    let rules = load(&tree).unwrap();
    assert_eq!(rules.len(), 1);
    let rule = &rules[0];
    assert_eq!(rule.id, "layout.file-size-cap");
    assert_eq!(rule.check, Check::MaxLines { max: 500 });
    assert_eq!(rule.status, Status::Enforced);
    assert_eq!(rule.folder_path, ".ai/rules/layout/file-size-cap");
    assert_eq!(
        rule.baseline_file(),
        ".ai/rules/layout/file-size-cap/baseline.txt"
    );
    assert!(rule.in_folder(".ai/rules/layout/file-size-cap/baseline.txt"));
    assert!(!rule.in_folder(".ai/rules/layout/file-size-cap-two/rule.toml"));
}

#[test]
fn an_unknown_field_is_refused_and_named() {
    let found = errors(&GOOD.replace("version = 1", "version = 1\nowner = \"me\""));
    assert!(found[0].contains("rule.toml"), "{found:?}");
    assert!(found[0].contains("owner"), "{found:?}");
    let found = errors(&GOOD.replace("max = 500", "max = 500\nmin = 1"));
    assert!(found[0].contains("min"), "{found:?}");
    let found = errors(&GOOD.replace("example = ", "picture = \"x\"\nexample = "));
    assert!(found[0].contains("picture"), "{found:?}");
}

#[test]
fn a_missing_field_is_named() {
    let found = errors(&GOOD.replace("why = \"Small files are read whole.\"\n", ""));
    assert!(found[0].contains("why"), "{found:?}");
    let found = errors(&GOOD.replace("engine = \"max_lines\"\n", ""));
    assert!(found[0].contains("check.engine: missing"), "{found:?}");
}

#[test]
fn the_id_and_category_follow_the_folder() {
    let found = errors(&GOOD.replace("layout.file-size-cap", "layout.size"));
    assert!(
        found.iter().any(|e| e.contains("id: \"layout.size\"")),
        "{found:?}"
    );
    let found = errors(&GOOD.replace("category = \"layout\"", "category = \"naming\""));
    assert!(found.iter().any(|e| e.contains("category:")), "{found:?}");
}

#[test]
fn an_enforced_rule_records_its_approval() {
    let without = GOOD.replace("[approval]\nby = \"operator\"\ndate = \"2026-11-26\"\n", "");
    let found = errors(&without);
    assert!(found.iter().any(|e| e.contains("approval:")), "{found:?}");
    let proposed = without.replace("status = \"enforced\"", "status = \"proposed\"");
    let dir = repo(&proposed);
    let tree = Tree::walk(dir.path(), true).unwrap();
    assert!(load(&tree).is_ok());
    let found = errors(&GOOD.replace("2026-11-26", "26/11/2026"));
    assert!(
        found.iter().any(|e| e.contains("approval.date")),
        "{found:?}"
    );
}

#[test]
fn values_out_of_range_are_named() {
    let found = errors(&GOOD.replace("status = \"enforced\"", "status = \"live\""));
    assert!(found[0].contains("live"), "{found:?}");
    let found = errors(&GOOD.replace("severity = \"error\"", "severity = \"warning\""));
    assert!(found[0].contains("warning"), "{found:?}");
    let found = errors(&GOOD.replace("max = 500", "max = 0"));
    assert!(found.iter().any(|e| e.contains("check.max")), "{found:?}");
    let found = errors(&GOOD.replace("engine = \"max_lines\"", "engine = \"lines\""));
    assert!(found[0].contains("\"lines\" is not an engine"), "{found:?}");
    let found = errors(&GOOD.replace("[\"**/*.rs\"]", "[\"a/[\"]"));
    assert!(
        found.iter().any(|e| e.contains("scope.include")),
        "{found:?}"
    );
}

#[test]
fn the_example_must_exist() {
    let found = errors(&GOOD.replace("src/small.rs", "src/gone.rs"));
    assert!(
        found
            .iter()
            .any(|e| e.contains("guidance.example: \"src/gone.rs\" does not exist")),
        "{found:?}"
    );
    let found = errors(&GOOD.replace("src/small.rs", "../outside.rs"));
    assert!(
        found.iter().any(|e| e.contains("guidance.example")),
        "{found:?}"
    );
}

#[test]
fn a_guidance_value_must_be_one_the_engine_gives() {
    let found = errors(&GOOD.replace("{allowed}", "{allowance}"));
    assert!(
        found
            .iter()
            .any(|e| e.contains("{allowance} is not a value")),
        "{found:?}"
    );
    assert_eq!(placeholders("a {file} b {x y} {lines}"), ["file", "lines"]);
}

#[test]
fn empty_sources_need_a_reason() {
    let found = errors(&GOOD.replace("[\"https://eslint.org/docs/latest/rules/max-lines\"]", "[]"));
    assert!(
        found.iter().any(|e| e.contains("sources: empty")),
        "{found:?}"
    );
    let reasoned = GOOD
        .replace("[\"https://eslint.org/docs/latest/rules/max-lines\"]", "[]")
        .replace(
            "read whole.",
            "read whole. No upstream source: our own rule.",
        );
    let dir = repo(&reasoned);
    assert!(load(&Tree::walk(dir.path(), true).unwrap()).is_ok());
}

#[test]
fn a_rule_folder_without_rule_toml_is_an_error() {
    let dir = repo(GOOD);
    write(dir.path(), ".ai/rules/layout/stray/notes.md", "");
    let tree = Tree::walk(dir.path(), true).unwrap();
    let found = load(&tree).err().unwrap();
    assert_eq!(found[0].file, ".ai/rules/layout/stray");
}
