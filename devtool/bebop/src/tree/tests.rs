use std::fs;

use super::*;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn the_walk_honours_gitignore_and_skips_git_but_not_hidden_folders() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, ".gitignore", "/target/\nnode_modules/\n");
    write(root, "target/debug/x.rs", "");
    write(root, "web/node_modules/a/b.js", "");
    write(root, ".git/config", "");
    write(root, ".ai/README.md", "");
    write(root, "engine/hub/src/lib.rs", "");
    let tree = Tree::walk(root, true).unwrap();
    let files: Vec<&str> = tree.files().collect();
    assert_eq!(
        files,
        [".ai/README.md", ".gitignore", "engine/hub/src/lib.rs"]
    );
    assert!(tree.is_dir("engine/hub"));
    assert!(!tree.is_dir("target"));
    assert_eq!(tree.child_dirs(""), [".ai", "engine", "web"]);
    assert_eq!(
        tree.child_files("engine/hub/src"),
        ["engine/hub/src/lib.rs"]
    );
}

#[test]
fn a_repository_walk_leaves_out_the_rules_own_examples() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, ".ai/rules/layout/size/rule.toml", "");
    write(root, ".ai/rules/layout/size/examples/bad/a.rs", "");
    write(root, "docs/examples/kept.md", "");
    let skipped = Tree::walk(root, true).unwrap();
    assert!(!skipped.is_file(".ai/rules/layout/size/examples/bad/a.rs"));
    assert!(skipped.is_file("docs/examples/kept.md"));
    let kept = Tree::walk(root, false).unwrap();
    assert!(kept.is_file(".ai/rules/layout/size/examples/bad/a.rs"));
}

#[test]
fn a_star_stays_in_its_folder_and_an_excluded_folder_takes_its_contents() {
    let scope = Scope::new(
        &["engine/*".to_owned(), "platform/**/*.rs".to_owned()],
        &["**/target".to_owned()],
    )
    .unwrap();
    assert!(scope.contains("engine/hub"));
    assert!(!scope.contains("engine/hub/src"));
    assert!(scope.contains("platform/store/src/lib.rs"));
    assert!(!scope.contains("platform/store/target/debug/build.rs"));
    assert!(!scope.contains("platform/store/src/lib.txt"));
}

#[test]
fn paths_from_a_command_line_are_normalized() {
    assert_eq!(normalize(".\\engine\\hub\\src\\"), "engine/hub/src");
    assert_eq!(normalize("./a/b.rs"), "a/b.rs");
    assert_eq!(name("a/b/c.rs"), "c.rs");
    assert_eq!(parent("a/b/c.rs"), "a/b");
    assert_eq!(parent("top"), "");
}

#[test]
fn a_bad_glob_says_which() {
    let error = Scope::new(&["a/[".to_owned()], &[]).err().unwrap();
    assert!(error.contains("\"a/[\""), "{error}");
}
