//! Baselines: the violations a rule found on the tree when it was enforced, which may only
//! shrink.
//!
//! # Owns
//!
//! The format of `baseline.txt` beside a rule, read strictly: comment and blank lines only in
//! the header, then one entry per line, sorted by path, none twice. A `max_lines` rule's entry
//! is `<lines><TAB><path>` and every other rule's is `<path>`. The ratchet: a listed file may
//! shrink and never grow, a listed violation that is gone fails until its line is removed, and
//! an unlisted violation fails. Tightening lowers numbers and drops resolved entries; it never
//! adds an entry or raises a number.
//!
//! # Serves
//!
//! [`parse`], [`render`], [`counted_breaches`], [`tighten_counted`] and [`tighten_listed`] to
//! the checks.
//!
//! # Settings
//!
//! None.
//!
//! # Depends on
//!
//! Nothing else in the crate.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// The two kinds of baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `<lines><TAB><path>`, every number over the cap.
    Counted { cap: usize },
    /// `<path>`.
    Listed,
}

/// A baseline's entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entries {
    Counted(BTreeMap<String, usize>),
    Listed(BTreeSet<String>),
}

impl Entries {
    pub fn len(&self) -> usize {
        match self {
            Entries::Counted(entries) => entries.len(),
            Entries::Listed(entries) => entries.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A baseline as read: its header (comment and blank lines, kept as they are), its entries,
/// and the line each entry is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub header: Vec<String>,
    pub entries: Entries,
    pub lines: BTreeMap<String, usize>,
}

/// The header a seeded baseline starts with.
pub fn seed_header(rule: &str, kind: Kind) -> Vec<String> {
    let format = match kind {
        Kind::Counted { .. } => "<lines><TAB><path>",
        Kind::Listed => "<path>",
    };
    vec![
        format!("# {rule}: the violations on the tree when the rule was enforced."),
        format!("# {format}, sorted by path. The list only shrinks: fix the code, then run"),
        "# `bebop baseline tighten`. Never add a line or raise a number to make a check pass."
            .to_owned(),
    ]
}

/// Reads a baseline strictly. Errors name the line.
pub fn parse(text: &str, kind: Kind) -> Result<Baseline, String> {
    let text = text.replace("\r\n", "\n");
    let mut header = Vec::new();
    let mut counted = BTreeMap::new();
    let mut listed = BTreeSet::new();
    let mut lines = BTreeMap::new();
    let mut previous: Option<String> = None;
    for (index, line) in text.lines().enumerate() {
        let at = index + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            if previous.is_some() {
                return Err(format!(
                    "line {at}: comments and blank lines belong in the header, above the first entry"
                ));
            }
            header.push(line.to_owned());
            continue;
        }
        let path = match kind {
            Kind::Counted { cap } => {
                let (count, path) = line.split_once('\t').ok_or_else(|| {
                    format!("line {at}: expected <lines><TAB><path>, got {line:?}")
                })?;
                let count: usize = count
                    .parse()
                    .map_err(|_| format!("line {at}: {count:?} is not a line count"))?;
                if count <= cap {
                    return Err(format!(
                        "line {at}: {path} is listed at {count} lines, within the cap of {cap}: remove the line"
                    ));
                }
                check_path(path, at)?;
                counted.insert(path.to_owned(), count);
                path
            }
            Kind::Listed => {
                check_path(line, at)?;
                listed.insert(line.to_owned());
                line
            }
        };
        if let Some(previous) = &previous {
            if previous.as_str() == path {
                return Err(format!("line {at}: {path} is listed twice"));
            }
            if previous.as_str() > path {
                return Err(format!(
                    "line {at}: {path} is out of order; entries are sorted by path, so it goes \
                     above {previous}"
                ));
            }
        }
        lines.insert(path.to_owned(), at);
        previous = Some(path.to_owned());
    }
    let entries = match kind {
        Kind::Counted { .. } => Entries::Counted(counted),
        Kind::Listed => Entries::Listed(listed),
    };
    Ok(Baseline {
        header,
        entries,
        lines,
    })
}

fn check_path(path: &str, at: usize) -> Result<(), String> {
    if path.is_empty()
        || path != path.trim()
        || path.contains('\t')
        || path.contains('\\')
        || path.starts_with('/')
        || path.starts_with("./")
    {
        return Err(format!(
            "line {at}: {path:?} is not a path as bebop writes one (from the repository root, \
             `/`-separated, no surrounding space)"
        ));
    }
    Ok(())
}

/// The baseline as its file holds it: the header, then the entries in path order.
pub fn render(header: &[String], entries: &Entries) -> String {
    let mut text = String::new();
    for line in header {
        text.push_str(line);
        text.push('\n');
    }
    match entries {
        Entries::Counted(entries) => {
            for (path, lines) in entries {
                text.push_str(&format!("{lines}\t{path}\n"));
            }
        }
        Entries::Listed(entries) => {
            for path in entries {
                text.push_str(path);
                text.push('\n');
            }
        }
    }
    text
}

/// What breaks a counted ratchet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Breach {
    /// A listed file holds more lines than its entry.
    Grew {
        path: String,
        allowed: usize,
        lines: usize,
    },
    /// A file that is not listed is over the cap.
    OverCap { path: String, lines: usize },
    /// A listed file is not in the tree.
    Missing { path: String },
}

impl Breach {
    pub fn path(&self) -> &str {
        match self {
            Breach::Grew { path, .. } | Breach::OverCap { path, .. } | Breach::Missing { path } => {
                path
            }
        }
    }
}

impl fmt::Display for Breach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Breach::Grew {
                path,
                allowed,
                lines,
            } => write!(
                f,
                "{path}: grew from {allowed} to {lines} lines. It is over the cap and may only shrink: split it"
            ),
            Breach::OverCap { path, lines } => write!(
                f,
                "{path}: {lines} lines, over the cap for a file not in the list: split it by subject"
            ),
            Breach::Missing { path } => write!(
                f,
                "{path}: listed but not found. If it moved, change its path in the list; if it is gone, remove its line"
            ),
        }
    }
}

/// What breaks the ratchet, one breach per file; empty when it holds. `actual` is every file
/// in scope with its line count.
pub fn counted_breaches(
    cap: usize,
    listed: &BTreeMap<String, usize>,
    actual: &BTreeMap<String, usize>,
) -> Vec<Breach> {
    let mut found = Vec::new();
    for (path, lines) in actual {
        match listed.get(path) {
            Some(allowed) if lines > allowed => found.push(Breach::Grew {
                path: path.clone(),
                allowed: *allowed,
                lines: *lines,
            }),
            None if *lines > cap => found.push(Breach::OverCap {
                path: path.clone(),
                lines: *lines,
            }),
            _ => {}
        }
    }
    for path in listed.keys() {
        if !actual.contains_key(path) {
            found.push(Breach::Missing { path: path.clone() });
        }
    }
    found
}

/// The list after a shrink: every number lowered to today's, entries at or under the cap or
/// without a file dropped. Never a new entry and never a higher number.
pub fn tighten_counted(
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

/// The list without the entries that are no longer found. Never a new entry.
pub fn tighten_listed(listed: &BTreeSet<String>, found: &BTreeSet<String>) -> BTreeSet<String> {
    listed.intersection(found).cloned().collect()
}
