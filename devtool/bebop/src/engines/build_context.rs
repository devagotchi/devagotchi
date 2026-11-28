//! `members_in_build_context`: every crate the root `Cargo.toml` makes Cargo load is inside the
//! image's build context. Cargo loads each workspace member, and each `path =` crate the
//! workspace's dependencies and patches name, before it builds one; a crate the ignore file
//! keeps out of the context is invisible to `cargo` in the image, which then fails to load the
//! workspace.
//!
//! The ignore file is read as Docker reads `.dockerignore`: lines in order, `#` lines and blank
//! lines skipped, `!` lets back in what an earlier line kept out, and the last line that matches
//! a path decides. A line matches a path when its glob matches the path or a folder above it, so
//! `*` keeps out every top-level folder and all beneath it. A crate is inside the context when its
//! `Cargo.toml` is.

use super::Finding;
use crate::tree::{self, Tree};

pub fn run(tree: &Tree, manifest: &str, ignore_file: &str) -> Result<Vec<Finding>, String> {
    let text = String::from_utf8_lossy(&tree.read(manifest)?).into_owned();
    let root: toml::Table =
        toml::from_str(&text).map_err(|e| format!("{manifest}: not a readable Cargo.toml: {e}"))?;
    let crates = loaded_crates(&root, manifest)?;
    let ignore = String::from_utf8_lossy(&tree.read(ignore_file)?).into_owned();
    let lines = ignore_lines(&ignore, ignore_file)?;
    Ok(crates
        .into_iter()
        .filter(|path| !inside(&lines, &format!("{path}/Cargo.toml")))
        .map(|path| {
            let top = path.split('/').next().unwrap_or(&path).to_owned();
            Finding::new(ignore_file)
                .with("member", path)
                .with("top", top)
                .with("manifest", manifest)
        })
        .collect())
}

/// The workspace members, then each `path =` crate of `[workspace.dependencies]` and of every
/// `[patch.<source>]`, normalized, each once, in that order.
fn loaded_crates(root: &toml::Table, manifest: &str) -> Result<Vec<String>, String> {
    let workspace = root
        .get("workspace")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{manifest}: has no [workspace] members"))?;
    let members = workspace
        .get("members")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("{manifest}: has no [workspace] members"))?;
    let mut crates = Vec::new();
    let mut add = |path: String| {
        if !crates.contains(&path) {
            crates.push(path);
        }
    };
    for member in members {
        let member = member
            .as_str()
            .ok_or_else(|| format!("{manifest}: a workspace member is not a string"))?;
        add(tree::normalize(member));
    }
    let tables = workspace
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .into_iter()
        .chain(
            root.get("patch")
                .and_then(toml::Value::as_table)
                .into_iter()
                .flat_map(|sources| sources.values().filter_map(toml::Value::as_table)),
        );
    for table in tables {
        for dependency in table.values() {
            if let Some(path) = dependency.get("path").and_then(toml::Value::as_str) {
                add(tree::normalize(path));
            }
        }
    }
    Ok(crates)
}

/// One ignore-file line: its glob, and whether it lets in (`!`) or keeps out.
struct Line {
    glob: globset::GlobMatcher,
    lets_in: bool,
}

fn ignore_lines(text: &str, ignore_file: &str) -> Result<Vec<Line>, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (lets_in, pattern) = match line.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, line),
            };
            let pattern = tree::normalize(pattern.trim_start_matches('/'));
            let glob = tree::glob(&pattern)
                .map_err(|e| format!("{ignore_file}: line {line:?}: {e}"))?
                .compile_matcher();
            Ok(Line { glob, lets_in })
        })
        .collect()
}

/// Whether `path` is in the context: the last line matching it or a folder above it decides, and
/// a path no line matches is in.
fn inside(lines: &[Line], path: &str) -> bool {
    let mut candidates: Vec<&str> = path.match_indices('/').map(|(at, _)| &path[..at]).collect();
    candidates.push(path);
    lines
        .iter()
        .rev()
        .find(|line| {
            candidates
                .iter()
                .any(|candidate| line.glob.is_match(candidate))
        })
        .is_none_or(|line| line.lets_in)
}
