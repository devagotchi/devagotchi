//! `folder_case`: every folder in scope is named in the rule's case. B0 knows one case,
//! kebab: lower-case letters and digits in words joined by single hyphens (`machine-broker`).

use super::Finding;
use crate::{
    rules::Case,
    tree::{self, Scope, Tree},
};

pub fn run(tree: &Tree, scope: &Scope, case: Case) -> Vec<Finding> {
    tree.dirs()
        .filter(|path| scope.contains(path))
        .filter_map(|path| {
            let name = tree::name(path);
            match case {
                Case::Kebab if !is_kebab(name) => Some(
                    Finding::new(path)
                        .with("name", name)
                        .with("suggestion", kebab(name)),
                ),
                Case::Kebab => None,
            }
        })
        .collect()
}

pub fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `name` in kebab case: `MachineEngine`, `machine_engine` and `machine engine` all become
/// `machine-engine`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && previous_lower {
                out.push('-');
            }
            previous_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            out.push(c.to_ascii_lowercase());
        } else {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            previous_lower = false;
        }
    }
    out.trim_end_matches('-').to_owned()
}
