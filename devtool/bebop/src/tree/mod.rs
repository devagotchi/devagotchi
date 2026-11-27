//! The tree a rule is checked against: every file and folder under a root, as `/`-separated
//! paths from that root, and a rule's scope over those paths.
//!
//! # Owns
//!
//! The walk: `.gitignore` files are honoured (so build output and `node_modules` are never
//! walked), `.git` is never walked, hidden folders such as `.ai` and `.github` are. Inside a
//! repository walk, the `examples/` folder of every rule is left out: those trees break their
//! rule on purpose and are checked by `bebop rule test` instead. Paths are the same on every
//! OS. A scope is `include` and `exclude` globs, where `*` never crosses a `/` and an excluded
//! folder excludes everything below it.
//!
//! # Serves
//!
//! [`Tree`] and [`Scope`] to the engines and the checks.
//!
//! # Settings
//!
//! None: a scope is part of its rule.
//!
//! # Depends on
//!
//! The `ignore` crate (the walk) and `globset` (the globs).

#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// The files and folders under a root.
pub struct Tree {
    root: PathBuf,
    files: BTreeSet<String>,
    dirs: BTreeSet<String>,
}

impl Tree {
    /// Walks `root`. `skip_rule_examples` leaves out `.ai/rules/<category>/<name>/examples/`.
    pub fn walk(root: &Path, skip_rule_examples: bool) -> Result<Tree, String> {
        if !root.is_dir() {
            return Err(format!("{}: not a folder to check", root.display()));
        }
        let mut walker = ignore::WalkBuilder::new(root);
        walker
            .hidden(false)
            .ignore(false)
            .parents(false)
            .git_global(false)
            .git_exclude(false)
            .git_ignore(true)
            .require_git(false)
            .follow_links(false)
            .sort_by_file_name(|a, b| a.cmp(b))
            .filter_entry(move |entry| {
                let name = entry.file_name();
                if name == ".git" {
                    return false;
                }
                !(skip_rule_examples && name == "examples" && is_rule_folder(entry.path()))
            });
        let mut files = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        for entry in walker.build() {
            let entry = entry.map_err(|e| format!("{}: {e}", root.display()))?;
            if entry.depth() == 0 {
                continue;
            }
            let path = relative(root, entry.path())?;
            match entry.file_type() {
                Some(kind) if kind.is_dir() => {
                    dirs.insert(path);
                }
                Some(kind) if kind.is_file() => {
                    files.insert(path);
                }
                _ => {}
            }
        }
        Ok(Tree {
            root: root.to_path_buf(),
            files,
            dirs,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(String::as_str)
    }

    pub fn dirs(&self) -> impl Iterator<Item = &str> {
        self.dirs.iter().map(String::as_str)
    }

    pub fn is_file(&self, path: &str) -> bool {
        self.files.contains(path)
    }

    pub fn is_dir(&self, path: &str) -> bool {
        self.dirs.contains(path)
    }

    /// The files directly in `dir` (`""` is the root).
    pub fn child_files<'a>(&'a self, dir: &str) -> Vec<&'a str> {
        children(&self.files, dir)
    }

    /// The folders directly in `dir` (`""` is the root).
    pub fn child_dirs<'a>(&'a self, dir: &str) -> Vec<&'a str> {
        children(&self.dirs, dir)
    }

    /// Every file anywhere below `dir`.
    pub fn files_under<'a>(&'a self, dir: &str) -> impl Iterator<Item = &'a str> {
        let prefix = format!("{dir}/");
        self.files
            .range(prefix.clone()..)
            .take_while(move |path| path.starts_with(&prefix))
            .map(String::as_str)
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        fs::read(self.root.join(path)).map_err(|e| format!("{path}: {e}"))
    }
}

/// `path` is `<…>/.ai/rules/<category>/<name>/examples`'s parent, a rule folder.
fn is_rule_folder(examples: &Path) -> bool {
    let Some(rule) = examples.parent() else {
        return false;
    };
    let rules = rule.parent().and_then(Path::parent);
    let ai = rules.and_then(Path::parent);
    rule.join("rule.toml").is_file()
        && rules
            .and_then(Path::file_name)
            .is_some_and(|n| n == "rules")
        && ai.and_then(Path::file_name).is_some_and(|n| n == ".ai")
}

fn children<'a>(set: &'a BTreeSet<String>, dir: &str) -> Vec<&'a str> {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    set.range(prefix.clone()..)
        .take_while(|path| path.starts_with(&prefix))
        .filter(|path| !path[prefix.len()..].contains('/'))
        .map(String::as_str)
        .collect()
}

/// `path` from `root`, `/`-separated on every OS.
fn relative(root: &Path, path: &Path) -> Result<String, String> {
    let inner = path
        .strip_prefix(root)
        .map_err(|_| format!("{}: outside {}", path.display(), root.display()))?;
    Ok(inner
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

/// The last part of a `/`-separated path.
pub fn name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Everything before the last `/`, or `""` at the top.
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// A path as a command line or an editor gives it: `\` becomes `/`, no `./` in front, no `/`
/// behind.
pub fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut path = path.as_str();
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    path.trim_end_matches('/').to_owned()
}

/// A rule's scope: the paths its `include` globs match, less those its `exclude` globs match
/// (an excluded folder takes everything below it).
pub struct Scope {
    include: GlobSet,
    exclude: GlobSet,
    /// Whether each folder seen so far is excluded, so a folder's globs run once per scope.
    folders: RefCell<HashMap<String, bool>>,
}

impl Scope {
    pub fn new(include: &[String], exclude: &[String]) -> Result<Scope, String> {
        Ok(Scope {
            include: globs(include)?,
            exclude: globs(exclude)?,
            folders: RefCell::new(HashMap::new()),
        })
    }

    pub fn contains(&self, path: &str) -> bool {
        self.include.is_match(path) && !self.excludes(path)
    }

    fn excludes(&self, path: &str) -> bool {
        if self.exclude.is_empty() {
            return false;
        }
        let mut folders = self.folders.borrow_mut();
        let excluded_folder = path
            .match_indices('/')
            .map(|(at, _)| &path[..at])
            .any(|folder| {
                *folders
                    .entry(folder.to_owned())
                    .or_insert_with(|| self.exclude.is_match(folder))
            });
        excluded_folder || self.exclude.is_match(path)
    }
}

/// One glob as rules write them: `*` stays within a folder, `**` crosses folders.
pub fn glob(pattern: &str) -> Result<globset::Glob, String> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .backslash_escape(true)
        .build()
        .map_err(|e| format!("{pattern:?} is not a glob: {e}"))
}

pub fn globs(patterns: &[String]) -> Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        set.add(glob(pattern)?);
    }
    set.build().map_err(|e| e.to_string())
}
