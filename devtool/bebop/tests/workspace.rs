//! bebop on this repository, in CI: every enforced rule holds on the tree (with its baseline),
//! and every rule's examples pass. These replace the layout tests that lived in
//! `runtime/devagotchi/tests/layout.rs` (the 500-line ratchet and one home per module).

use std::path::{Path, PathBuf};

use devagotchi_bebop::checks::{self, Selection, Workspace, examples};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn workspace() -> Workspace {
    Workspace::load(&repository())
        .unwrap_or_else(|broken| panic!("the rules do not load:\n  {}", broken.0.join("\n  ")))
}

#[test]
fn the_workspace_keeps_every_enforced_rule() {
    let workspace = workspace();
    assert!(
        workspace.tree.is_file("devtool/bebop/tests/workspace.rs"),
        "the walk did not find this test: the repository root is wrong"
    );
    let enforced = workspace.rules.iter().filter(|r| r.is_enforced()).count();
    assert!(enforced >= 2, "only {enforced} enforced rules were found");
    let report = checks::check(&workspace, &Selection::All)
        .unwrap_or_else(|broken| panic!("bebop stopped:\n  {}", broken.0.join("\n  ")));
    assert!(report.is_clean(), "{}", report.text());
}

#[test]
fn every_rule_passes_its_examples() {
    let workspace = workspace();
    let outcomes = examples::test_examples(&workspace, None)
        .unwrap_or_else(|broken| panic!("bebop stopped:\n  {}", broken.0.join("\n  ")));
    assert_eq!(outcomes.len(), workspace.rules.len());
    let failed: Vec<String> = outcomes
        .iter()
        .filter(|o| !o.passed())
        .map(|o| format!("{}:\n    {}", o.rule, o.problems.join("\n    ")))
        .collect();
    assert!(
        failed.is_empty(),
        "rule examples failed:\n  {}",
        failed.join("\n  ")
    );
}

#[test]
fn the_binary_exits_as_documented() {
    let bebop = env!("CARGO_BIN_EXE_bebop");
    let run = |args: &[&str]| {
        std::process::Command::new(bebop)
            .args(args)
            .arg("--root")
            .arg(repository())
            .output()
            .unwrap()
    };
    let clean = run(&["check", "--all", "--format", "xml"]);
    let xml = String::from_utf8_lossy(&clean.stdout);
    assert_eq!(clean.status.code(), Some(0), "{xml}");
    assert!(xml.starts_with("<bebop-report rules=\""), "{xml}");
    assert!(xml.contains("violations=\"0\""), "{xml}");
    let usage = run(&["check", "--format", "json"]);
    assert_eq!(usage.status.code(), Some(2));
}
