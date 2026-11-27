//! The engines' tests, a file per engine, over small trees written to a temporary folder.

mod crate_names;
mod domain_roles;
mod max_lines;
mod module_home;
mod names;

use std::fs;

use super::*;
use crate::tree::{Scope, Tree};

/// A tree of `files` (each with its text) in a temporary folder.
pub(super) fn tree(files: &[(&str, &str)]) -> (tempfile::TempDir, Tree) {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let tree = Tree::walk(dir.path(), true).unwrap();
    (dir, tree)
}

pub(super) fn scope(include: &[&str], exclude: &[&str]) -> Scope {
    let own = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    Scope::new(&own(include), &own(exclude)).unwrap()
}

/// The files `check` finds in `files`, with everything in scope.
pub(super) fn found(check: &Check, files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, tree) = tree(files);
    match run(check, &tree, &scope(&["**"], &[])).unwrap() {
        Outcome::Found(found) => found.into_iter().map(|f| f.file).collect(),
        Outcome::Measured(_) => panic!("{} measures, it does not find", check.engine()),
    }
}

/// The finding for `file`.
pub(super) fn finding(check: &Check, files: &[(&str, &str)], file: &str) -> Finding {
    let (_dir, tree) = tree(files);
    match run(check, &tree, &scope(&["**"], &[])).unwrap() {
        Outcome::Found(found) => found.into_iter().find(|f| f.file == file).unwrap(),
        Outcome::Measured(_) => panic!("{} measures, it does not find", check.engine()),
    }
}

#[test]
fn every_value_an_engine_gives_is_one_it_lists() {
    let check = Check::LayerFolders {
        allowed: vec!["engine".to_owned()],
    };
    let one = finding(&check, &[("engines/a.rs", "")], "engines");
    for (name, _) in &one.vars {
        assert!(vars(&check).contains(name), "{name}");
    }
}
