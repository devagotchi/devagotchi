//! `crate_name_matches_folder`: every crate's package name is the prefix and its folder's
//! name (`engine/hub` is `devagotchi-hub`). The scope picks the `Cargo.toml` files; a
//! manifest with no `[package]` (a workspace root) has no name to check.

use super::Finding;
use crate::tree::{self, Scope, Tree};

pub fn run(tree: &Tree, scope: &Scope, prefix: &str) -> Result<Vec<Finding>, String> {
    let mut found = Vec::new();
    for path in tree.files().filter(|path| scope.contains(path)) {
        if tree::name(path) != "Cargo.toml" {
            continue;
        }
        let text = String::from_utf8_lossy(&tree.read(path)?).into_owned();
        let manifest: toml::Table =
            toml::from_str(&text).map_err(|e| format!("{path}: not a readable Cargo.toml: {e}"))?;
        let Some(package) = manifest.get("package") else {
            continue;
        };
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("{path}: [package] has no name"))?;
        let folder = tree::name(tree::parent(path));
        let expected = format!("{prefix}{folder}");
        if name != expected {
            found.push(
                Finding::new(path)
                    .with("name", name)
                    .with("expected_name", expected)
                    .with("folder", tree::parent(path)),
            );
        }
    }
    Ok(found)
}
