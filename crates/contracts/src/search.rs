//! `search_files`: files found by name, or lines found inside files, under one directory.
//! Paged and stateless: a page ends at `max_results` or after a few seconds of searching, and
//! its `next_cursor` says where the next page starts, so nothing is held open between calls
//! and a search goes through a hub as easily as through a local adapter.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most matches one page returns.
pub const MAX_SEARCH_RESULTS: u32 = 500;
/// Most lines of context around a content match.
pub const MAX_SEARCH_CONTEXT: u32 = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTarget {
    /// File and directory names (the default).
    #[default]
    Names,
    /// Lines inside text files.
    Contents,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileSearchInput {
    /// Directory to search (absolute, `~/...`, or relative to the workspace).
    pub path: String,
    /// What to look for: plain text, or a regular expression with `regex`.
    pub pattern: String,
    /// `names` (default) or `contents`.
    #[serde(default)]
    pub target: SearchTarget,
    /// Read `pattern` as a regular expression (Rust syntax, like ripgrep's).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub regex: bool,
    /// Match case exactly; by default case is ignored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub case_sensitive: bool,
    /// Only files whose name matches this glob, such as `*.rs` or `*.{ts,tsx}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    /// Also search hidden files and what .gitignore excludes (both are skipped by default,
    /// as git and ripgrep skip them).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_ignored: bool,
    /// Lines of context before and after each content match, 0..=5.
    #[serde(default)]
    pub context_lines: u32,
    /// Most matches on this page, 1..=500 (default 100).
    #[serde(default = "default_search_results")]
    pub max_results: u32,
    /// Where to go on from: the previous page's `next_cursor`. Absent for the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Default for omitted search_results inputs on either tool surface.
pub const DEFAULT_SEARCH_RESULTS: u32 = 100;
pub fn default_search_results() -> u32 {
    DEFAULT_SEARCH_RESULTS
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchMatch {
    /// Path relative to the searched directory, `/`-separated.
    pub path: String,
    /// `file` or `dir`, for a name match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The matching line's number (1-based), for a content match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    /// The matching line, without its line ending; a very long line is cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Lines just before the match, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    /// Lines just after the match, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchPage {
    /// The searched directory's canonical path.
    pub path: String,
    /// In path order: a directory's entries by name, each directory's contents where its name
    /// falls. Within a file, by line.
    pub matches: Vec<SearchMatch>,
    /// Files whose contents this page read (contents searches).
    pub files_searched: u64,
    /// Files this page could not search: unreadable, binary, or over 8 MiB.
    pub files_skipped: u64,
    /// True when the whole search is done; false with `next_cursor` when there is more.
    pub complete: bool,
    /// Why this page stopped early: `max_results` or `time`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_by: Option<String>,
    /// Pass as `cursor`, with the same other inputs, for the next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
