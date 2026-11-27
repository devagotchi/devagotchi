//! `forbidden_folder_name`: no folder in scope has one of the names (`utils`, `common`, …):
//! a folder names what it holds.

use super::Finding;
use crate::tree::{self, Scope, Tree};

pub fn run(tree: &Tree, scope: &Scope, names: &[String]) -> Vec<Finding> {
    tree.dirs()
        .filter(|path| scope.contains(path))
        .filter(|path| names.iter().any(|name| name == tree::name(path)))
        .map(|path| Finding::new(path).with("name", tree::name(path)))
        .collect()
}
