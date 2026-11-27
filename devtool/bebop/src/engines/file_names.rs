//! `forbidden_file_name`: no file in scope has a name one of the patterns matches (a glob on
//! the file's name, such as `*_tests.rs`).

use super::Finding;
use crate::tree::{self, Scope, Tree};

pub fn run(tree: &Tree, scope: &Scope, patterns: &[String]) -> Result<Vec<Finding>, String> {
    let set = tree::globs(patterns)?;
    Ok(tree
        .files()
        .filter(|path| scope.contains(path))
        .filter_map(|path| {
            let name = tree::name(path);
            let first = set.matches(name).into_iter().min()?;
            Some(
                Finding::new(path)
                    .with("name", name)
                    .with("pattern", patterns[first].as_str()),
            )
        })
        .collect())
}
