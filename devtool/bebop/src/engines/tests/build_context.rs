use super::*;

fn check() -> Check {
    Check::MembersInBuildContext {
        manifest: "Cargo.toml".to_owned(),
        ignore_file: ".dockerignore".to_owned(),
    }
}

const MANIFEST: &str = "[workspace]\nmembers = [\"platform/contracts\", \"devtool/bebop\", \"apps/desktop/src-tauri\"]\n";

#[test]
fn a_member_outside_every_let_in_folder_is_found() {
    let files = [
        ("Cargo.toml", MANIFEST),
        (
            ".dockerignore",
            "# the image builds from the workspace alone\n*\n!Cargo.toml\n!platform/\n!apps/desktop/src-tauri/\n**/target/\n",
        ),
    ];
    assert_eq!(found(&check(), &files), [".dockerignore"]);
    let one = finding(&check(), &files, ".dockerignore");
    assert_eq!(one.var("member"), Some("devtool/bebop"));
    assert_eq!(one.var("top"), Some("devtool"));
}

#[test]
fn every_member_let_in_is_clean() {
    let files = [
        ("Cargo.toml", MANIFEST),
        (
            ".dockerignore",
            "*\n!platform/\n!devtool/\n!apps/desktop/src-tauri/\n",
        ),
    ];
    assert!(found(&check(), &files).is_empty());
}

#[test]
fn an_ignore_file_without_a_star_excludes_nothing() {
    let files = [("Cargo.toml", MANIFEST), (".dockerignore", "**/target/\n")];
    assert!(found(&check(), &files).is_empty());
}

#[test]
fn a_manifest_without_members_is_an_error_not_a_pass() {
    let (_dir, tree) = tree(&[
        ("Cargo.toml", "[package]\nname = \"x\"\n"),
        (".dockerignore", "*\n"),
    ]);
    let error = run(&check(), &tree, &scope(&["Cargo.toml"], &[])).unwrap_err();
    assert!(error.contains("has no [workspace] members"), "{error}");
}

#[test]
fn a_later_line_that_keeps_a_member_out_again_wins() {
    let files = [
        ("Cargo.toml", MANIFEST),
        (
            ".dockerignore",
            "*\n!platform/\n!devtool/\n!apps/desktop/src-tauri/\ndevtool/bebop/\n",
        ),
    ];
    assert_eq!(found(&check(), &files), [".dockerignore"]);
    let one = finding(&check(), &files, ".dockerignore");
    assert_eq!(one.var("member"), Some("devtool/bebop"));
}

#[test]
fn a_line_inside_a_member_does_not_keep_the_member_out() {
    let files = [
        ("Cargo.toml", MANIFEST),
        (
            ".dockerignore",
            "*\n!platform/\n!devtool/\n!apps/desktop/src-tauri/\napps/desktop/src-tauri/gen/\n**/target/\n",
        ),
    ];
    assert!(found(&check(), &files).is_empty());
}

#[test]
fn a_path_dependency_outside_the_context_is_found() {
    let manifest = "[workspace]\nmembers = [\"platform/contracts\"]\n\n\
        [workspace.dependencies]\ndevagotchi-agenthub = { path = \"agenthub\" }\n";
    let files = [
        ("Cargo.toml", manifest),
        (".dockerignore", "*\n!platform/\n"),
    ];
    assert_eq!(found(&check(), &files), [".dockerignore"]);
    let one = finding(&check(), &files, ".dockerignore");
    assert_eq!(one.var("member"), Some("agenthub"));
    assert_eq!(one.var("top"), Some("agenthub"));
}

#[test]
fn a_patched_path_outside_the_context_is_found() {
    let manifest = "[workspace]\nmembers = [\"platform/contracts\"]\n\n\
        [patch.\"https://github.com/devagotchi/agenthub\"]\nagenthub-board = { path = \"vendor/board\" }\n";
    let files = [
        ("Cargo.toml", manifest),
        (".dockerignore", "*\n!platform/\n"),
    ];
    let one = finding(&check(), &files, ".dockerignore");
    assert_eq!(one.var("member"), Some("vendor/board"));
    assert_eq!(one.var("top"), Some("vendor"));
}

#[test]
fn an_ignore_line_that_is_not_a_glob_is_an_error_not_a_pass() {
    let (_dir, tree) = tree(&[
        ("Cargo.toml", MANIFEST),
        (".dockerignore", "*\n!plat[form/\n"),
    ]);
    let error = run(&check(), &tree, &scope(&["Cargo.toml"], &[])).unwrap_err();
    assert!(error.contains("is not a glob"), "{error}");
}
