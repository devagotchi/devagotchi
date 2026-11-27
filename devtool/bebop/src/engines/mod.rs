//! The engines: what each kind of check finds in a tree. B0's engines are native and look at
//! structure only: files, folders, their names, their line counts and crate names.
//!
//! # Owns
//!
//! One file per engine. An engine is given a rule's check, the tree and the rule's scope, and
//! returns its findings, each with the values its guidance may name (`{file}`, `{lines}`, …).
//! `max_lines` returns every measured file instead, so its baseline can hold numbers.
//!
//! # Serves
//!
//! [`run`] and [`vars`] to the checks and to rule loading.
//!
//! # Settings
//!
//! None: an engine's parameters are part of its rule's `[check]`.
//!
//! # Depends on
//!
//! `crate::rules` (the check), `crate::tree` (the tree and the scope), and `toml` to read a
//! crate's `Cargo.toml`.

mod crate_names;
mod domain_roles;
mod file_names;
mod folder_case;
mod folder_names;
mod layer_folders;
mod max_lines;
mod module_home;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

pub use folder_case::is_kebab;
pub use max_lines::line_count;

use crate::{
    rules::Check,
    tree::{Scope, Tree},
};

/// One thing a rule found: where, and the values its guidance names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: Option<usize>,
    pub vars: Vec<(&'static str, String)>,
}

impl Finding {
    pub fn new(file: &str) -> Finding {
        Finding {
            file: file.to_owned(),
            line: None,
            vars: vec![("file", file.to_owned())],
        }
    }

    pub fn with(mut self, name: &'static str, value: impl Into<String>) -> Finding {
        self.vars.push((name, value.into()));
        self
    }

    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What an engine returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every file in scope with its line count (`max_lines`).
    Measured(BTreeMap<String, usize>),
    /// The findings, sorted by file.
    Found(Vec<Finding>),
}

impl Outcome {
    /// How many findings there are with no baseline.
    pub fn count(&self, check: &Check) -> usize {
        match (self, check) {
            (Outcome::Measured(files), Check::MaxLines { max }) => {
                files.values().filter(|lines| *lines > max).count()
            }
            (Outcome::Measured(_), _) => 0,
            (Outcome::Found(found), _) => found.len(),
        }
    }
}

/// Runs `check` over the files and folders of `tree` that `scope` holds.
pub fn run(check: &Check, tree: &Tree, scope: &Scope) -> Result<Outcome, String> {
    let mut found = match check {
        Check::MaxLines { .. } => return max_lines::measure(tree, scope).map(Outcome::Measured),
        Check::ModuleHome => module_home::run(tree, scope),
        Check::ForbiddenFileName { patterns } => file_names::run(tree, scope, patterns)?,
        Check::ForbiddenFolderName { names } => folder_names::run(tree, scope, names),
        Check::FolderCase { case } => folder_case::run(tree, scope, *case),
        Check::CrateNameMatchesFolder { prefix } => crate_names::run(tree, scope, prefix)?,
        Check::LayerFolders { allowed } => layer_folders::run(tree, scope, allowed),
        Check::DomainRoleFiles {
            roots,
            allowed_files,
            allowed_dirs,
        } => domain_roles::run(tree, scope, roots, allowed_files, allowed_dirs),
    };
    found.sort_by(|a, b| a.file.cmp(&b.file));
    found.dedup_by(|a, b| a.file == b.file);
    Ok(Outcome::Found(found))
}

/// The values an engine's findings carry, which a rule's guidance may name as `{value}`.
pub fn vars(check: &Check) -> &'static [&'static str] {
    match check {
        Check::MaxLines { .. } => &["file", "lines", "max", "allowed"],
        Check::ModuleHome => &["file", "module", "folder", "home"],
        Check::ForbiddenFileName { .. } => &["file", "name", "pattern"],
        Check::ForbiddenFolderName { .. } => &["file", "name"],
        Check::FolderCase { .. } => &["file", "name", "suggestion"],
        Check::CrateNameMatchesFolder { .. } => &["file", "name", "expected_name", "folder"],
        Check::LayerFolders { .. } => &["file", "name", "allowed", "suggestion"],
        Check::DomainRoleFiles { .. } => &["file", "name", "domain", "roles", "detail", "remedy"],
    }
}
