//! The file tools' wire types: bounded reads, listings, writes, edits, information, new
//! directories and moves, answered by the broker under its execution policy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most entries one `list_directory` page returns.
pub const MAX_LIST_ENTRIES: u32 = 1000;
/// Deepest `list_directory` goes below the directory it lists.
pub const MAX_LIST_DEPTH: u32 = 3;
/// Largest page of file bytes one read returns, so the answer fits one wire frame.
pub const MAX_FILE_READ_BYTES: usize = 24 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileReadInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of the file.
    pub path: String,
    /// Byte offset to start at; negative counts back from the end (a tail). Default 0.
    /// With a positive `start_line`: the `next_offset` that came with that `next_line`, so
    /// the line is not searched for again.
    #[serde(default)]
    pub offset: i64,
    /// Bytes to read, 1..=24576 (default 24576). Read on from `next_offset` for more.
    #[serde(default = "default_file_read_bytes")]
    pub max_bytes: usize,
    /// Read by lines instead of bytes: the 1-based line to start at, or negative to count
    /// back from the end (-50: the last 50 lines). `offset` is then not used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i64>,
    /// With `start_line`: how many lines, 1..=5000 (default 1000; from a negative
    /// `start_line`, the lines to the end). Never more than `max_bytes` of text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u32>,
}

/// Most lines one read by lines returns.
pub const MAX_READ_LINES: u32 = 5000;

/// Default for omitted file_read_bytes inputs on either tool surface.
pub const DEFAULT_FILE_READ_BYTES: usize = MAX_FILE_READ_BYTES;
pub fn default_file_read_bytes() -> usize {
    DEFAULT_FILE_READ_BYTES
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileContent {
    /// The file's canonical path.
    pub path: String,
    /// The file's size in bytes when it was read.
    pub size: u64,
    /// Where these bytes start.
    pub offset: u64,
    /// The bytes as text, when they are UTF-8 text. A character split at the end of the
    /// page is left for the next read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The raw bytes, base64, when they are not text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    /// The offset to read on from.
    pub next_offset: u64,
    /// Whether this page reached the end of the file.
    pub eof: bool,
    pub modified_ms: Option<u64>,
    /// Which file object was read: device and inode on Unix, volume serial and file ID on
    /// Windows. It stays through a rename and changes when the file is replaced by another, so
    /// pages read in turn are known to be of one file when it is the same on each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Read by lines: the 1-based number of the first line returned, when known (always
    /// for a positive `start_line`; from the end, for files up to 8 MiB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    /// Read by lines: the `start_line` to read on from, when lines remain and the number
    /// is known. A single line longer than `max_bytes` is read on by bytes, from
    /// `next_offset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_line: Option<u64>,
    /// Read by lines: the file's line count, for files up to 8 MiB, or once a read from a
    /// known line number reaches the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileListInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of the directory.
    pub path: String,
    /// How many levels below it to include, 0..=3 (default 0: its own entries).
    #[serde(default)]
    pub depth: u32,
    /// Most entries to return, 1..=1000 (default 200).
    #[serde(default = "default_list_entries")]
    pub max_entries: u32,
    /// Where to go on from: the previous page's `next_cursor`. Absent for the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Default for omitted list_entries inputs on either tool surface.
pub const DEFAULT_LIST_ENTRIES: u32 = 200;
pub fn default_list_entries() -> u32 {
    DEFAULT_LIST_ENTRIES
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    /// Path relative to the listed directory, `/`-separated.
    pub path: String,
    /// `file`, `dir`, `symlink` or `other`. Symbolic links are listed, never followed.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_ms: Option<u64>,
    /// Why a directory's own entries are missing (it could not be read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// True when the name on disk is not valid Unicode: `path` shows it with U+FFFD in
    /// place of what could not be decoded, so it cannot be used to reach the entry.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub name_lossy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileListing {
    /// The directory's canonical path.
    pub path: String,
    /// Directories first at each level, then by name.
    pub entries: Vec<FileEntry>,
    /// Whether `max_entries` cut the listing short.
    pub truncated: bool,
    /// When it did: pass this as `cursor` for the entries after these, in the same order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Largest content one write or edit carries, so the request fits one wire frame.
pub const MAX_FILE_WRITE_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    /// Replace the file's whole content (atomically: a new file renamed into place), or
    /// create it. The default.
    #[default]
    Overwrite,
    /// Create the file; refuse when it exists.
    Create,
    /// Add to the end of the file, creating it when missing.
    Append,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileWriteInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of the file.
    pub path: String,
    /// The content as text, written as UTF-8 exactly as given. Give this or `bytes_base64`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The content as base64 bytes. At most 32 KiB of content per call; append for more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    #[serde(default)]
    /// overwrite (default), create (refuse if it exists) or append.
    pub mode: WriteMode,
    /// Overwrite only if the file's current SHA-256 is this (from an earlier write or
    /// edit), so a change made in between is not lost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    /// Create missing parent directories.
    #[serde(default)]
    pub create_dirs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileEditInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of a UTF-8 text file.
    pub path: String,
    /// Exact text to find, including whitespace and line endings.
    pub old_text: String,
    /// Text to put in its place.
    pub new_text: String,
    /// How many times `old_text` must occur (default 1); every occurrence is replaced, and
    /// a different count changes nothing.
    #[serde(default = "default_replacements")]
    pub expected_replacements: u32,
}

/// Default for omitted replacements inputs on either tool surface.
pub const DEFAULT_REPLACEMENTS: u32 = 1;
pub fn default_replacements() -> u32 {
    DEFAULT_REPLACEMENTS
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileWritten {
    /// The file's canonical path.
    pub path: String,
    /// Its size after the change.
    pub size: u64,
    /// SHA-256 of its whole content after the change: pass it as `expected_sha256` next time.
    pub sha256: String,
    /// Whether the file was created.
    pub created: bool,
    /// For an edit: how many occurrences were replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacements: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileInfoInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of a file, directory or symbolic link.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectoryInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of the directory to make; missing parents are made too. An existing
    /// directory is not an error.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileMoveInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of the file, directory or link to move or rename (a link moves as
    /// itself, never what it points to).
    pub source: String,
    /// Path (absolute, `~/...`, or relative to the workspace) it moves to, on the same filesystem. Its parent directory must exist.
    pub destination: String,
    /// Replace a file already at the destination (never a directory). Default false: an
    /// existing destination refuses the move and changes nothing.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileInfo {
    /// The path, canonical up to its own name (a link is reported as itself).
    pub path: String,
    /// `file`, `dir`, `symlink` or `other`.
    pub kind: String,
    /// Size in bytes (for a link, the link's own).
    pub size: u64,
    pub modified_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<u64>,
    /// Whether its permissions mark it read-only.
    pub readonly: bool,
    /// Unix permission bits in octal (`0644`); absent on Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Where a symbolic link points, as stored in the link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// For a text file of at most 8 MiB: how many lines it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u64>,
    /// After create_directory: whether this call made it (false: it existed already).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made: Option<bool>,
    /// After move_file: whether a file at the destination was replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced: Option<bool>,
}
