//! `search_files`: names or lines under one directory, found the way ripgrep finds them
//! (hidden files and what .gitignore excludes are skipped unless asked for; links are listed,
//! never followed). A page is answered in path order and ends at its result limit or its time
//! budget; its cursor names the last entry, and the line within it, that the page finished, so
//! the next page starts right after it and the broker holds nothing between pages.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use devagotchi_contracts::{
    BridgeError, BridgeResult, FileSearchInput, MAX_SEARCH_CONTEXT, MAX_SEARCH_RESULTS,
    SearchMatch, SearchPage, SearchTarget,
};
use regex::{Regex, RegexBuilder};

use super::{Scope, display, invalid};

/// How long one page searches before it answers with what it has and a cursor.
const PAGE_TIME: Duration = devagotchi_ipc::IO_TIMEOUT.saturating_sub(Duration::from_secs(2));
const _: () = assert!(PAGE_TIME.as_millis() < devagotchi_ipc::IO_TIMEOUT.as_millis());
/// Larger files are skipped (and counted) by a contents search.
const MAX_SEARCHED_FILE: u64 = 8 * 1024 * 1024;
/// A file whose first bytes hold a NUL is binary, as git and ripgrep judge it.
const BINARY_PROBE: usize = 8 * 1024;
/// Characters of a line returned; the rest is cut.
const MAX_LINE_CHARS: usize = 400;

/// Where a page starts. Everything that sorts before `entry` is done. `done` says how much
/// of `entry` itself is: all of it (`a:<entry>`), or its lines up to `line` (`<line>:<entry>`,
/// `0:` when none of it is).
struct Cursor {
    entry: PathBuf,
    done: Done,
}

#[derive(Clone, Copy)]
enum Done {
    All,
    Lines(u64),
}

impl Cursor {
    fn parse(text: &str) -> BridgeResult<Self> {
        let bad = || invalid("cursor is not one this search returned");
        let (done, entry) = text.split_once(':').ok_or_else(bad)?;
        let done = match done {
            "a" => Done::All,
            line => Done::Lines(line.parse().map_err(|_| bad())?),
        };
        Ok(Self {
            entry: PathBuf::from(entry),
            done,
        })
    }

    fn after(entry: &str) -> String {
        format!("a:{entry}")
    }

    fn at(entry: &str, lines_done: u64) -> String {
        format!("{lines_done}:{entry}")
    }
}

impl Scope<'_> {
    pub(crate) fn search(&self, input: FileSearchInput) -> BridgeResult<SearchPage> {
        self.search_for(input, PAGE_TIME)
    }

    pub(super) fn search_for(
        &self,
        input: FileSearchInput,
        budget: Duration,
    ) -> BridgeResult<SearchPage> {
        if input.pattern.is_empty() {
            return Err(invalid("pattern must not be empty"));
        }
        if !(1..=MAX_SEARCH_RESULTS).contains(&input.max_results) {
            return Err(invalid(format!(
                "max_results must be 1..={MAX_SEARCH_RESULTS}"
            )));
        }
        if input.context_lines > MAX_SEARCH_CONTEXT {
            return Err(invalid(format!(
                "context_lines must be 0..={MAX_SEARCH_CONTEXT}"
            )));
        }
        let matcher = matcher(&input)?;
        let glob = match &input.glob {
            Some(glob) => Some(
                globset::Glob::new(glob)
                    .map_err(|error| invalid(format!("glob {glob} is not a glob: {error}")))?
                    .compile_matcher(),
            ),
            None => None,
        };
        let cursor = input.cursor.as_deref().map(Cursor::parse).transpose()?;
        let root = self.resolve(&input.path)?;
        if !root.is_dir() {
            return Err(BridgeError::new(
                "not_a_directory",
                format!("{} is not a directory", display(&root)?),
            ));
        }
        let mut page = SearchPage {
            path: display(&root)?,
            matches: Vec::new(),
            files_searched: 0,
            files_skipped: 0,
            complete: true,
            stopped_by: None,
            next_cursor: None,
        };
        let started = Instant::now();
        let mut searched_one = false;
        for entry in walker(&root, &input, cursor.as_ref().map(|c| c.entry.clone())) {
            let Ok(entry) = entry else {
                page.files_skipped += 1;
                continue;
            };
            if entry.depth() == 0 {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(&root) else {
                continue;
            };
            let mut after_line = 0;
            if let Some(cursor) = &cursor {
                match (relative.cmp(&cursor.entry), cursor.done) {
                    (std::cmp::Ordering::Less, _) | (std::cmp::Ordering::Equal, Done::All) => {
                        continue;
                    }
                    (std::cmp::Ordering::Equal, Done::Lines(line)) => after_line = line,
                    (std::cmp::Ordering::Greater, _) => {}
                }
            }
            let shown = relative_text(relative);
            // Every page takes at least one entry, so a slow disk still makes progress.
            if searched_one && started.elapsed() >= budget {
                stop(&mut page, "time", Cursor::at(&shown, after_line));
                return Ok(page);
            }
            searched_one = true;
            let kind = entry.file_type().map_or("other", |kind| {
                if kind.is_dir() {
                    "dir"
                } else if kind.is_symlink() {
                    "symlink"
                } else {
                    "file"
                }
            });
            let wanted = glob
                .as_ref()
                .is_none_or(|glob| kind != "dir" && glob.is_match(glob_target(relative, &input)));
            if !wanted {
                continue;
            }
            match input.target {
                SearchTarget::Names => {
                    let name = entry.file_name().to_string_lossy();
                    if matcher.is_match(&name) {
                        page.matches.push(SearchMatch {
                            path: shown.clone(),
                            kind: Some(kind.to_owned()),
                            line: None,
                            text: None,
                            before: Vec::new(),
                            after: Vec::new(),
                        });
                        if page.matches.len() as u32 == input.max_results {
                            stop(&mut page, "max_results", Cursor::after(&shown));
                            return Ok(page);
                        }
                    }
                }
                SearchTarget::Contents if kind == "file" => {
                    let Some(text) = searchable(entry.path()) else {
                        page.files_skipped += 1;
                        continue;
                    };
                    page.files_searched += 1;
                    let lines: Vec<&str> = text.lines().collect();
                    let context = input.context_lines as usize;
                    for (index, line) in lines.iter().enumerate() {
                        let number = index as u64 + 1;
                        if number <= after_line || !matcher.is_match(line) {
                            continue;
                        }
                        page.matches.push(SearchMatch {
                            path: shown.clone(),
                            kind: None,
                            line: Some(number),
                            text: Some(cut(line)),
                            before: lines[index.saturating_sub(context)..index]
                                .iter()
                                .map(|l| cut(l))
                                .collect(),
                            after: lines[index + 1..(index + 1 + context).min(lines.len())]
                                .iter()
                                .map(|l| cut(l))
                                .collect(),
                        });
                        if page.matches.len() as u32 == input.max_results {
                            let next = if index + 1 == lines.len() {
                                Cursor::after(&shown)
                            } else {
                                Cursor::at(&shown, number)
                            };
                            stop(&mut page, "max_results", next);
                            return Ok(page);
                        }
                    }
                }
                SearchTarget::Contents => {}
            }
        }
        Ok(page)
    }
}

fn stop(page: &mut SearchPage, why: &str, cursor: String) {
    page.complete = false;
    page.stopped_by = Some(why.to_owned());
    page.next_cursor = Some(cursor);
}

fn relative_text(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn glob_target<'a>(relative: &'a Path, input: &FileSearchInput) -> &'a Path {
    match &input.glob {
        Some(glob) if !glob.contains('/') => Path::new(relative.file_name().unwrap_or_default()),
        _ => relative,
    }
}

fn matcher(input: &FileSearchInput) -> BridgeResult<Regex> {
    let pattern = if input.regex {
        input.pattern.clone()
    } else {
        regex::escape(&input.pattern)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!input.case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|error| {
            invalid(format!(
                "pattern is not a regular expression this search reads: {error}"
            ))
        })
}

/// The walk, in path order, skipping whole directories that a cursor has already passed.
fn walker(root: &Path, input: &FileSearchInput, after: Option<PathBuf>) -> ignore::Walk {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(!input.include_ignored)
        .require_git(false)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    if let Some(after) = after {
        let root = root.to_path_buf();
        builder.filter_entry(move |entry| {
            let Ok(relative) = entry.path().strip_prefix(&root) else {
                return true;
            };
            !(entry.file_type().is_some_and(|kind| kind.is_dir())
                && relative < after.as_path()
                && !after.starts_with(relative))
        });
    }
    builder.build()
}

/// A file's text when a contents search reads it: a regular file, not too large, not binary.
fn searchable(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SEARCHED_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn cut(line: &str) -> String {
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::ExecPolicy;
    use std::fs;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        for (path, text) in [
            ("a.txt", "nothing here\n"),
            (
                "src/Match.rs",
                "fn one() {}\n// match me\nfn two() {}\n// MATCH again\n",
            ),
            ("src/deep/match.md", "match\n"),
            (".hidden/match.txt", "match\n"),
            ("build/match.txt", "match\n"),
            (".gitignore", "build/\n"),
            ("blob.bin", "match\0binary\n"),
        ] {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        (directory, root)
    }

    fn input(root: &Path, pattern: &str, target: SearchTarget) -> FileSearchInput {
        FileSearchInput {
            path: root.display().to_string(),
            pattern: pattern.into(),
            target,
            regex: false,
            case_sensitive: false,
            glob: None,
            include_ignored: false,
            context_lines: 0,
            max_results: 100,
            cursor: None,
        }
    }

    fn found(page: &SearchPage) -> Vec<String> {
        page.matches
            .iter()
            .map(|m| match m.line {
                Some(line) => format!("{}:{line}", m.path),
                None => m.path.clone(),
            })
            .collect()
    }

    #[test]
    fn names_skip_hidden_and_gitignored_unless_asked() {
        let (_directory, root) = tree();
        let scope = Scope {
            workspace: &root,
            policy: ExecPolicy::Granted,
        };
        let page = scope
            .search(input(&root, "match", SearchTarget::Names))
            .unwrap();
        assert_eq!(found(&page), ["src/Match.rs", "src/deep/match.md"]);
        assert!(page.complete);
        let mut all = input(&root, "match", SearchTarget::Names);
        all.include_ignored = true;
        let page = scope.search(all).unwrap();
        assert_eq!(
            found(&page),
            [
                ".hidden/match.txt",
                "build/match.txt",
                "src/Match.rs",
                "src/deep/match.md"
            ]
        );
    }

    #[test]
    fn contents_give_lines_with_context_and_skip_binary_files() {
        let (_directory, root) = tree();
        let scope = Scope {
            workspace: &root,
            policy: ExecPolicy::Granted,
        };
        let mut asked = input(&root, "match", SearchTarget::Contents);
        asked.context_lines = 1;
        let page = scope.search(asked).unwrap();
        assert_eq!(
            found(&page),
            ["src/Match.rs:2", "src/Match.rs:4", "src/deep/match.md:1"]
        );
        assert_eq!(page.matches[0].before, ["fn one() {}"]);
        assert_eq!(page.matches[0].after, ["fn two() {}"]);
        assert_eq!(page.files_skipped, 1, "blob.bin is binary");
        let mut exact = input(&root, "MATCH", SearchTarget::Contents);
        exact.case_sensitive = true;
        exact.glob = Some("*.rs".into());
        assert_eq!(found(&scope.search(exact).unwrap()), ["src/Match.rs:4"]);
        let mut pattern = input(&root, r"fn \w+\(\)", SearchTarget::Contents);
        pattern.regex = true;
        assert_eq!(
            found(&scope.search(pattern).unwrap()),
            ["src/Match.rs:1", "src/Match.rs:3"]
        );
    }

    /// Paging one match at a time, or stopping on the time budget at every entry, finds the
    /// same matches as one page, each once.
    #[test]
    fn pages_go_on_from_their_cursor_without_losing_or_repeating() {
        let (_directory, root) = tree();
        let scope = Scope {
            workspace: &root,
            policy: ExecPolicy::Granted,
        };
        for target in [SearchTarget::Names, SearchTarget::Contents] {
            let whole = found(&scope.search(input(&root, "match", target)).unwrap());
            for (max_results, budget) in [(1, PAGE_TIME), (100, Duration::ZERO)] {
                let mut paged = Vec::new();
                let mut asked = input(&root, "match", target);
                asked.max_results = max_results;
                for _ in 0..100 {
                    let page = scope.search_for(asked.clone(), budget).unwrap();
                    paged.extend(found(&page));
                    match page.next_cursor {
                        Some(cursor) if !page.complete => asked.cursor = Some(cursor),
                        _ => break,
                    }
                }
                assert_eq!(paged, whole, "{target:?} by {max_results} with {budget:?}");
            }
        }
    }

    #[test]
    fn a_search_that_cannot_run_says_why() {
        let (_directory, root) = tree();
        let scope = Scope {
            workspace: &root.join("src"),
            policy: ExecPolicy::Granted,
        };
        let mut bad = input(&root, "(", SearchTarget::Contents);
        bad.regex = true;
        assert!(
            scope
                .search(bad)
                .unwrap_err()
                .message
                .contains("regular expression")
        );
        assert_eq!(
            scope
                .search(input(&root, "x", SearchTarget::Names))
                .unwrap_err()
                .code,
            "path_outside_workspace"
        );
        let mut cursor = input(&root.join("src"), "x", SearchTarget::Names);
        cursor.cursor = Some("nonsense".into());
        assert!(scope.search(cursor).unwrap_err().message.contains("cursor"));
    }
}
