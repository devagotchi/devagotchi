//! The size ratchet from `.ai/patterns/layout.md`: no new Rust file over the cap, and no
//! file already over it may grow. `layout/oversize.txt` lists the files that were over the
//! cap when the rule arrived; the list only gets shorter and its numbers only get smaller.

mod support;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use support::workspace_root;

/// Most lines a Rust file may hold unless `layout/oversize.txt` lists it.
const CAP: usize = 500;
const HEADER: &str = "\
# Rust files over the 500-line cap when the cap arrived (main 332bf25, 2026-11-13).
# <lines><TAB><path>, sorted by path. A listed file may shrink, never grow; a file that is
# not listed may not exceed the cap. Never add a line or raise a number to make the test
# pass: see .ai/patterns/layout.md.
";

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/layout/oversize.txt")
}

/// Lines as `wc -l` counts them, plus a last line that has no newline.
fn line_count(bytes: &[u8]) -> usize {
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    match bytes.last() {
        Some(last) if *last != b'\n' => newlines + 1,
        _ => newlines,
    }
}

/// Every `.rs` file under the roots, as a `/`-separated path from the workspace root, with
/// its line count.
fn rust_files(root: &Path) -> BTreeMap<String, usize> {
    support::rust_files(root)
        .into_iter()
        .map(|(relative, path)| {
            let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{relative}: {e}"));
            (relative, line_count(&bytes))
        })
        .collect()
}

fn parse_baseline(text: &str, cap: usize) -> Result<BTreeMap<String, usize>, String> {
    let mut listed = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let at = index + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (lines, path) = line
            .split_once('\t')
            .ok_or_else(|| format!("line {at}: expected <lines><TAB><path>, got {line:?}"))?;
        let lines: usize = lines
            .parse()
            .map_err(|_| format!("line {at}: {lines:?} is not a line count"))?;
        if lines <= cap {
            return Err(format!(
                "line {at}: {path} is listed at {lines} lines, within the cap of {cap}: remove the line"
            ));
        }
        if listed.insert(path.to_owned(), lines).is_some() {
            return Err(format!("line {at}: {path} is listed twice"));
        }
    }
    Ok(listed)
}

/// What breaks the ratchet, one message per file; empty when it holds.
fn violations(
    cap: usize,
    listed: &BTreeMap<String, usize>,
    actual: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut found = Vec::new();
    for (path, lines) in actual {
        match listed.get(path) {
            Some(allowed) if lines > allowed => found.push(format!(
                "{path}: grew from {allowed} to {lines} lines. It is over the cap of {cap} and may only shrink: split it"
            )),
            None if *lines > cap => found.push(format!(
                "{path}: {lines} lines, over the cap of {cap} for a file not in the list: split it by subject"
            )),
            _ => {}
        }
    }
    for path in listed.keys() {
        if !actual.contains_key(path) {
            found.push(format!(
                "{path}: listed but not found. If it moved, change its path in the list; if it is gone, remove its line"
            ));
        }
    }
    found
}

/// The list after a shrink: every number lowered to today's, entries at or under the cap or
/// without a file dropped. Never a new entry and never a higher number.
fn tightened(
    cap: usize,
    listed: &BTreeMap<String, usize>,
    actual: &BTreeMap<String, usize>,
) -> BTreeMap<String, usize> {
    listed
        .iter()
        .filter_map(|(path, allowed)| {
            let lines = (*actual.get(path)?).min(*allowed);
            (lines > cap).then(|| (path.clone(), lines))
        })
        .collect()
}

fn render(listed: &BTreeMap<String, usize>) -> String {
    let mut text = HEADER.to_owned();
    for (path, lines) in listed {
        text.push_str(&format!("{lines}\t{path}\n"));
    }
    text
}

#[test]
fn no_rust_file_outgrows_the_layout_ratchet() {
    let path = baseline_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    let mut listed = parse_baseline(&text, CAP).unwrap_or_else(|e| panic!("oversize.txt {e}"));
    let actual = rust_files(&workspace_root());
    assert!(
        actual.contains_key("runtime/devagotchi/tests/layout.rs"),
        "the walk did not find this test: the workspace root is wrong"
    );
    if std::env::var_os("DEVAGOTCHI_WRITE_LAYOUT").is_some() {
        listed = tightened(CAP, &listed, &actual);
        fs::write(&path, render(&listed)).unwrap();
    }
    let found = violations(CAP, &listed, &actual);
    assert!(
        found.is_empty(),
        "the layout ratchet (.ai/patterns/layout.md) is broken by {} file(s):\n  {}\n",
        found.len(),
        found.join("\n  ")
    );
}

fn files(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
    entries
        .iter()
        .map(|(path, lines)| ((*path).to_owned(), *lines))
        .collect()
}

#[test]
fn a_listed_file_may_shrink_and_never_grow() {
    let listed = files(&[("a.rs", 900)]);
    assert!(violations(500, &listed, &files(&[("a.rs", 900)])).is_empty());
    assert!(violations(500, &listed, &files(&[("a.rs", 899)])).is_empty());
    assert!(violations(500, &listed, &files(&[("a.rs", 120)])).is_empty());
    let grown = violations(500, &listed, &files(&[("a.rs", 901)]));
    assert_eq!(grown.len(), 1);
    assert!(
        grown[0].starts_with("a.rs: grew from 900 to 901"),
        "{grown:?}"
    );
}

#[test]
fn a_file_that_is_not_listed_stays_within_the_cap() {
    let listed = files(&[("a.rs", 900)]);
    let at_the_cap = files(&[("a.rs", 900), ("new.rs", 500)]);
    assert!(violations(500, &listed, &at_the_cap).is_empty());
    let over = violations(500, &listed, &files(&[("a.rs", 900), ("new.rs", 501)]));
    assert_eq!(over.len(), 1);
    assert!(over[0].starts_with("new.rs: 501 lines"), "{over:?}");
}

#[test]
fn a_moved_file_takes_its_entry_along() {
    let listed = files(&[("old/a.rs", 900)]);
    let moved = violations(500, &listed, &files(&[("new/a.rs", 900)]));
    assert_eq!(moved.len(), 2, "{moved:?}");
    assert!(moved.iter().any(|m| m.starts_with("new/a.rs: 900 lines")));
    assert!(
        moved
            .iter()
            .any(|m| m.starts_with("old/a.rs: listed but not found"))
    );
}

#[test]
fn tightening_only_lowers_and_removes() {
    let listed = files(&[
        ("a.rs", 900),
        ("b.rs", 800),
        ("c.rs", 700),
        ("gone.rs", 600),
    ]);
    let actual = files(&[
        ("a.rs", 950),
        ("b.rs", 640),
        ("c.rs", 500),
        ("new.rs", 2000),
    ]);
    assert_eq!(
        tightened(500, &listed, &actual),
        files(&[("a.rs", 900), ("b.rs", 640)])
    );
}

#[test]
fn the_list_is_read_strictly() {
    let listed = parse_baseline("# note\n\n900\ta.rs\n501\tb c.rs\n", 500).unwrap();
    assert_eq!(listed, files(&[("a.rs", 900), ("b c.rs", 501)]));
    assert_eq!(parse_baseline(&render(&listed), 500).unwrap(), listed);
    for bad in [
        "900 a.rs\n",
        "many\ta.rs\n",
        "500\ta.rs\n",
        "900\ta.rs\n901\ta.rs\n",
    ] {
        assert!(parse_baseline(bad, 500).is_err(), "{bad:?}");
    }
}

#[test]
fn lines_are_counted_as_wc_counts_them() {
    assert_eq!(line_count(b""), 0);
    assert_eq!(line_count(b"a\nb\n"), 2);
    assert_eq!(line_count(b"a\r\nb\r\n"), 2);
    assert_eq!(line_count(b"a\nb"), 2);
}

/// Modules that were both `<x>.rs` and a folder `<x>/` of Rust when layout rule 2 got its
/// check (B3, 2026-11-19). The list only gets shorter.
fn beside_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/layout/beside.txt")
}

/// Every `<x>.rs` with a folder `<x>/` beside it that holds Rust, except `main`, `lib` and
/// `build`: one module in two places (layout rule 2). A folder holds its module as `mod.rs`.
fn beside(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = support::rust_files(root)
        .into_iter()
        .filter_map(|(relative, path)| {
            let stem = path.file_stem()?.to_str()?.to_string();
            let folder = path.with_extension("");
            let holds_rust = folder.is_dir() && support::rust_files_under(&folder).next().is_some();
            (holds_rust && !["main", "lib", "build", "mod"].contains(&stem.as_str()))
                .then_some(relative)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn no_module_is_both_a_file_and_a_folder_beyond_those_listed() {
    let path = beside_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    let listed: Vec<&str> = text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let found = beside(&workspace_root());
    let new: Vec<&String> = found
        .iter()
        .filter(|f| !listed.contains(&f.as_str()))
        .collect();
    assert!(
        new.is_empty(),
        "layout rule 2: make each a folder with mod.rs, never list it:\n  {}",
        new.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    let gone: Vec<&&str> = listed
        .iter()
        .filter(|l| !found.iter().any(|f| f == *l))
        .collect();
    assert!(
        gone.is_empty(),
        "fixed, so take them out of layout/beside.txt:\n  {}",
        gone.iter().map(|s| **s).collect::<Vec<_>>().join("\n  ")
    );
}
