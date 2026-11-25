//! What the tests that read the workspace's own source share: where it is, and its Rust
//! files, walked one way (`layout.rs`, `link_protocol.rs`; G12).

// Each test binary that includes this module uses only part of it.
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Folders under the workspace root that hold the workspace's Rust.
pub const ROOTS: [&str; 6] = ["platform", "engine", "interfaces", "runtime", "agenthub", "apps"];
/// Folder names never walked: build output and other people's code.
pub const SKIPPED: [&str; 2] = ["target", "node_modules"];

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under the roots, keyed by its `/`-separated path from `root`.
pub fn rust_files(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    for name in ROOTS {
        walk(root, &root.join(name), &mut found);
    }
    found
}

/// Every `.rs` file in `dir` and below it.
pub fn rust_files_under(dir: &Path) -> impl Iterator<Item = PathBuf> {
    let mut found = BTreeMap::new();
    walk(dir, dir, &mut found);
    found.into_values()
}

fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            if !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                walk(root, &path, found);
            }
        } else if kind.is_file() && name.ends_with(".rs") {
            found.insert(relative(root, &path), path);
        }
    }
}

/// `path` from `root`, `/`-separated on every OS.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
