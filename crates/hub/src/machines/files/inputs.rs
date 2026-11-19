//! Hub mirrors of file contracts, with one routing field.
use devagotchi_contracts::WriteMode;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadFileInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Path (absolute, `~/...`, or relative to the workspace) of the file.
    pub path: String,
    /// Byte offset to start at; negative counts back from the end (a tail). Default 0.
    /// With a positive `start_line`: the `next_offset` that came with that `next_line`, so
    /// the line is not searched for again.
    #[serde(default)]
    pub offset: i64,
    /// Bytes to read, 1..=24576 (default 24576). Read on from `next_offset` for more.
    #[serde(default = "devagotchi_contracts::default_file_read_bytes")]
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListDirectoryInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Path (absolute, `~/...`, or relative to the workspace) of the directory.
    pub path: String,
    /// How many levels below it to include, 0..=3 (default 0: its own entries).
    #[serde(default)]
    pub depth: u32,
    /// Most entries to return, 1..=1000 (default 200).
    #[serde(default = "devagotchi_contracts::default_list_entries")]
    pub max_entries: u32,
    /// Where to go on from: the previous page's `next_cursor`. Absent for the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteFileInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditFileInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Path (absolute, `~/...`, or relative to the workspace) of a UTF-8 text file.
    pub path: String,
    /// Exact text to find, including whitespace and line endings.
    pub old_text: String,
    /// Text to put in its place.
    pub new_text: String,
    /// How many times `old_text` must occur (default 1); every occurrence is replaced, and
    /// a different count changes nothing.
    #[serde(default = "devagotchi_contracts::default_replacements")]
    pub expected_replacements: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadFilesInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Paths (absolute, `~/...`, or relative to the workspace) of the files, 1 to 20.
    pub paths: Vec<String>,
    /// Bytes to read from the start of each, 1 to 24576 (default 24576). Read on from a
    /// file's `next_offset` with read_file.
    #[serde(default)]
    pub max_bytes_each: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathOnMachine {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Path (absolute, `~/...`, or relative to the workspace) of a file, directory or symbolic link.
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveFileInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectoryOnMachine {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Path (absolute, `~/...`, or relative to the workspace) of the directory to make; missing parents are made too. An existing
    /// directory is not an error.
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageOnMachine {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Path (absolute, `~/...`, or relative to the workspace) of a PNG, JPEG, GIF or WebP image, up to 3.75 MB.
    pub path: String,
}

