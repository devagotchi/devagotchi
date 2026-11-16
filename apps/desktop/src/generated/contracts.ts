/**
 * Generated from canonical Rust DTOs in src/contracts.rs. DO NOT EDIT.
 * Generator: node scripts/generate-contracts.cjs
 * Drift check: node scripts/generate-contracts.cjs --check
 * JSON Schema: schemas/broker-contracts.schema.json
 * Broker validation enforces domain bounds beyond these structural types.
 */

export type ListScope = "mine" | "account";
/**
 * How an interactive operation takes input.
 */
export type InputMode = "terminal" | "pipe";
export type OperationState =
  | "accepted"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "timed_out"
  | "interrupted"
  | "unknown";

/**
 * Export-only schema root. These fields reference canonical Rust DTOs and are
 * not a broker request or a second set of independently authored contracts.
 */
export interface BrokerContracts {
  bridge_error: BridgeError;
  changes_input: ChangesInput;
  config_get_input: ConfigGetInput;
  config_set_input: ConfigSetInput;
  config_view: ConfigView;
  directory_input: DirectoryInput;
  file_content: FileContent;
  file_edit_input: FileEditInput;
  file_info: FileInfo;
  file_info_input: FileInfoInput;
  file_list_input: FileListInput;
  file_listing: FileListing;
  file_move_input: FileMoveInput;
  file_read_input: FileReadInput;
  file_search_input: FileSearchInput;
  file_write_input: FileWriteInput;
  file_written: FileWritten;
  get_input: GetInput;
  list_input: ListInput;
  operation_view: OperationView;
  operations_page: OperationsPage;
  output_page: OutputPage;
  process_kill_input: ProcessKillInput;
  process_killed: ProcessKilled;
  process_list_input: ProcessListInput;
  process_page: ProcessPage;
  read_input: ReadInput;
  revision: Revision;
  search_page: SearchPage;
  start_input: StartInput;
  start_output: StartOutput;
  wait_input: WaitInput;
}
export interface BridgeError {
  code: string;
  message: string;
}
/**
 * Wait until the broker's operations change as a list shows them: one admitted, retired,
 * or moved to another state (output alone is not a change here). For a view that
 * re-lists only when something changed, instead of on a timer.
 */
export interface ChangesInput {
  /**
   * The revision your list reflects (from the last answer). Absent: answer at once with
   * the current revision. A broker that restarted answers at once, too.
   */
  after_revision?: number | null;
  /**
   * Longest to wait, 1..=50000 milliseconds; at the limit the same revision comes back.
   */
  timeout_ms: number;
}
export interface ConfigGetInput {}
export interface ConfigSetInput {
  /**
   * One of `output_limit_bytes`, `max_operations`, `max_concurrent`.
   */
  key: string;
  /**
   * The new value, a whole number.
   */
  value: number;
}
export interface ConfigView {
  /**
   * The OS account the broker runs as, and its home directory (what `~` means).
   */
  account?: string | null;
  /**
   * The programs a `granted` broker runs (canonical paths).
   */
  allowed_executables: string[];
  arch: string;
  /**
   * The settings `set_config_value` changes.
   */
  changeable: string[];
  /**
   * What the last change did, for `set_config_value`.
   */
  changed?: string | null;
  /**
   * The broker.json this broker was started from, when it was started from one.
   */
  config_file?: string | null;
  /**
   * `account`: anything this OS account can run. `granted`: only `allowed_executables`.
   */
  exec_policy: string;
  home?: string | null;
  /**
   * Jobs running at once.
   */
  max_concurrent: number;
  /**
   * Jobs kept, running and finished; the oldest finished one is retired past this.
   */
  max_operations: number;
  /**
   * Jobs held now, and how many of them are running.
   */
  operations_held: number;
  operations_running: number;
  /**
   * `linux` or `windows`, and the CPU architecture.
   */
  os: string;
  /**
   * Output kept per job, in bytes.
   */
  output_limit_bytes: number;
  /**
   * The broker's build.
   */
  version: string;
  /**
   * Where a relative path and a job without `cwd` start (canonical).
   */
  workspace: string;
}
export interface DirectoryInput {
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of the directory to make; missing parents are made too. An existing
   * directory is not an error.
   */
  path: string;
}
export interface FileContent {
  /**
   * The raw bytes, base64, when they are not text.
   */
  bytes_base64?: string | null;
  /**
   * Whether this page reached the end of the file.
   */
  eof: boolean;
  /**
   * Which file object was read: device and inode on Unix, volume serial and file ID on
   * Windows. It stays through a rename and changes when the file is replaced by another, so
   * pages read in turn are known to be of one file when it is the same on each.
   */
  identity?: string | null;
  modified_ms?: number | null;
  /**
   * Read by lines: the `start_line` to read on from, when lines remain and the number
   * is known. A single line longer than `max_bytes` is read on by bytes, from
   * `next_offset`.
   */
  next_line?: number | null;
  /**
   * The offset to read on from.
   */
  next_offset: number;
  /**
   * Where these bytes start.
   */
  offset: number;
  /**
   * The file's canonical path.
   */
  path: string;
  /**
   * The file's size in bytes when it was read.
   */
  size: number;
  /**
   * Read by lines: the 1-based number of the first line returned, when known (always
   * for a positive `start_line`; from the end, for files up to 8 MiB).
   */
  start_line?: number | null;
  /**
   * The bytes as text, when they are UTF-8 text. A character split at the end of the
   * page is left for the next read.
   */
  text?: string | null;
  /**
   * Read by lines: the file's line count, for files up to 8 MiB, or once a read from a
   * known line number reaches the end.
   */
  total_lines?: number | null;
}
export interface FileEditInput {
  /**
   * How many times `old_text` must occur (default 1); every occurrence is replaced, and
   * a different count changes nothing.
   */
  expected_replacements?: number;
  /**
   * Text to put in its place.
   */
  new_text: string;
  /**
   * Exact text to find, including whitespace and line endings.
   */
  old_text: string;
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of a UTF-8 text file.
   */
  path: string;
}
export interface FileInfo {
  created_ms?: number | null;
  /**
   * `file`, `dir`, `symlink` or `other`.
   */
  kind: string;
  /**
   * For a text file of at most 8 MiB: how many lines it has.
   */
  lines?: number | null;
  /**
   * Where a symbolic link points, as stored in the link.
   */
  link_target?: string | null;
  /**
   * After create_directory: whether this call made it (false: it existed already).
   */
  made?: boolean | null;
  /**
   * Unix permission bits in octal (`0644`); absent on Windows.
   */
  mode?: string | null;
  modified_ms?: number | null;
  /**
   * The path, canonical up to its own name (a link is reported as itself).
   */
  path: string;
  /**
   * Whether its permissions mark it read-only.
   */
  readonly: boolean;
  /**
   * After move_file: whether a file at the destination was replaced.
   */
  replaced?: boolean | null;
  /**
   * Size in bytes (for a link, the link's own).
   */
  size: number;
}
export interface FileInfoInput {
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of a file, directory or symbolic link.
   */
  path: string;
}
export interface FileListInput {
  /**
   * Where to go on from: the previous page's `next_cursor`. Absent for the first page.
   */
  cursor?: string | null;
  /**
   * How many levels below it to include, 0..=3 (default 0: its own entries).
   */
  depth?: number;
  /**
   * Most entries to return, 1..=1000 (default 200).
   */
  max_entries?: number;
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of the directory.
   */
  path: string;
}
export interface FileListing {
  /**
   * Directories first at each level, then by name.
   */
  entries: FileEntry[];
  /**
   * When it did: pass this as `cursor` for the entries after these, in the same order.
   */
  next_cursor?: string | null;
  /**
   * The directory's canonical path.
   */
  path: string;
  /**
   * Whether `max_entries` cut the listing short.
   */
  truncated: boolean;
}
export interface FileEntry {
  /**
   * Why a directory's own entries are missing (it could not be read).
   */
  error?: string | null;
  /**
   * `file`, `dir`, `symlink` or `other`. Symbolic links are listed, never followed.
   */
  kind: string;
  modified_ms?: number | null;
  /**
   * True when the name on disk is not valid Unicode: `path` shows it with U+FFFD in
   * place of what could not be decoded, so it cannot be used to reach the entry.
   */
  name_lossy?: boolean;
  /**
   * Path relative to the listed directory, `/`-separated.
   */
  path: string;
  size?: number | null;
}
export interface FileMoveInput {
  /**
   * Path (absolute, `~/...`, or relative to the workspace) it moves to, on the same filesystem. Its parent directory must exist.
   */
  destination: string;
  /**
   * Replace a file already at the destination (never a directory). Default false: an
   * existing destination refuses the move and changes nothing.
   */
  overwrite?: boolean;
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of the file, directory or link to move or rename (a link moves as
   * itself, never what it points to).
   */
  source: string;
}
export interface FileReadInput {
  /**
   * With `start_line`: how many lines, 1..=5000 (default 1000; from a negative
   * `start_line`, the lines to the end). Never more than `max_bytes` of text.
   */
  line_count?: number | null;
  /**
   * Bytes to read, 1..=24576 (default 24576). Read on from `next_offset` for more.
   */
  max_bytes?: number;
  /**
   * Byte offset to start at; negative counts back from the end (a tail). Default 0.
   * With a positive `start_line`: the `next_offset` that came with that `next_line`, so
   * the line is not searched for again.
   */
  offset?: number;
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of the file.
   */
  path: string;
  /**
   * Read by lines instead of bytes: the 1-based line to start at, or negative to count
   * back from the end (-50: the last 50 lines). `offset` is then not used.
   */
  start_line?: number | null;
}
export interface FileSearchInput {
  /**
   * Match case exactly; by default case is ignored.
   */
  case_sensitive?: boolean;
  /**
   * Lines of context before and after each content match, 0..=5.
   */
  context_lines?: number;
  /**
   * Where to go on from: the previous page's `next_cursor`. Absent for the first page.
   */
  cursor?: string | null;
  /**
   * Only files whose name matches this glob, such as `*.rs` or `*.{ts,tsx}`.
   */
  glob?: string | null;
  /**
   * Also search hidden files and what .gitignore excludes (both are skipped by default,
   * as git and ripgrep skip them).
   */
  include_ignored?: boolean;
  /**
   * Most matches on this page, 1..=500 (default 100).
   */
  max_results?: number;
  /**
   * Directory to search (absolute, `~/...`, or relative to the workspace).
   */
  path: string;
  /**
   * What to look for: plain text, or a regular expression with `regex`.
   */
  pattern: string;
  /**
   * Read `pattern` as a regular expression (Rust syntax, like ripgrep's).
   */
  regex?: boolean;
  /**
   * `names` (default) or `contents`.
   */
  target?: "names" | "contents";
}
export interface FileWriteInput {
  /**
   * The content as base64 bytes. At most 32 KiB of content per call; append for more.
   */
  bytes_base64?: string | null;
  /**
   * Create missing parent directories.
   */
  create_dirs?: boolean;
  /**
   * Overwrite only if the file's current SHA-256 is this (from an earlier write or
   * edit), so a change made in between is not lost.
   */
  expected_sha256?: string | null;
  mode?: "overwrite" | "create" | "append";
  /**
   * Path (absolute, `~/...`, or relative to the workspace) of the file.
   */
  path: string;
  /**
   * The content as text, written as UTF-8 exactly as given. Give this or `bytes_base64`.
   */
  text?: string | null;
}
export interface FileWritten {
  /**
   * Whether the file was created.
   */
  created: boolean;
  /**
   * The file's canonical path.
   */
  path: string;
  /**
   * For an edit: how many occurrences were replaced.
   */
  replacements?: number | null;
  /**
   * SHA-256 of its whole content after the change: pass it as `expected_sha256` next time.
   */
  sha256: string;
  /**
   * Its size after the change.
   */
  size: number;
}
export interface GetInput {
  operation_id: string;
}
export interface ListInput {
  limit?: number;
  offset?: number;
  /**
   * With a caller: `account` (the default) lists what any of its account's conversations
   * started, also before a restart (for this machine's own account, everything here);
   * `mine` only what this conversation or client started. Without a caller: everything.
   */
  scope?: ListScope | null;
}
export interface OperationView {
  args: string[];
  cancellation_requested: boolean;
  created_at_ms: number;
  cwd: string;
  deadline_ms: number;
  detail?: string | null;
  executable: string;
  exit_code?: number | null;
  /**
   * Present for an interactive operation: how it takes input and whether it still can.
   */
  interactive?: InteractiveView | null;
  operation_id: string;
  output_complete: boolean;
  /**
   * Root process and its inherited process group only; this is not OS containment.
   */
  ownership_scope: string;
  state: OperationState;
  updated_at_ms: number;
}
/**
 * An interactive operation's input, as it stands.
 */
export interface InteractiveView {
  /**
   * The terminal's current size (terminal mode).
   */
  cols: number;
  /**
   * Whether `operation_input` can still reach the program.
   */
  input_open: boolean;
  mode: InputMode;
  rows: number;
  /**
   * Until when (milliseconds since the Unix epoch) one client holds the keyboard; others
   * wait or take it over. Absent when nobody holds it.
   */
  writer_until_ms?: number | null;
}
export interface OperationsPage {
  next_offset?: number | null;
  operations: OperationSummary[];
}
export interface OperationSummary {
  created_at_ms: number;
  /**
   * Bounded display preview only. Execute only the exact argv from the canonical request.
   */
  display_command: string;
  /**
   * How an interactive operation takes input; absent for one that takes none.
   */
  interactive?: InputMode | null;
  operation_id: string;
  state: OperationState;
  updated_at_ms: number;
}
export interface OutputPage {
  earliest_retained_sequence: number;
  frames: OutputFrame[];
  /**
   * True when requested output was evicted from this operation's bounded retention.
   */
  gap: boolean;
  has_more: boolean;
  latest_sequence: number;
  next_cursor: number;
  operation: OperationView;
  operation_id: string;
}
export interface OutputFrame {
  /**
   * Raw process bytes, losslessly encoded. Text below is a UTF-8 convenience view.
   */
  bytes_base64: string;
  sequence: number;
  stream: string;
  /**
   * Incrementally decoded per stream; split UTF-8 sequences are retained between chunks.
   */
  text: string;
}
export interface ProcessKillInput {
  /**
   * Kill at once (SIGKILL) instead of asking it to stop (SIGTERM). On Windows a process is
   * always ended at once.
   */
  force?: boolean;
  pid: number;
}
export interface ProcessKilled {
  /**
   * Whether it was gone within a few seconds.
   */
  exited: boolean;
  name: string;
  pid: number;
  /**
   * `terminate` or `kill`.
   */
  signal: string;
}
export interface ProcessListInput {
  /**
   * Only processes whose name or command line contains this (case ignored).
   */
  filter?: string | null;
  /**
   * Most processes to return, 1..=1000 (default 200), busiest first.
   */
  max_results?: number;
}
export interface ProcessPage {
  /**
   * Processes that matched, before `max_results`.
   */
  matched: number;
  processes: ProcessInfo[];
  /**
   * Processes on the machine.
   */
  total: number;
}
export interface ProcessInfo {
  /**
   * This broker, a process it runs under, or another Devagotchi process (its link, an
   * adapter): stopping it would cut this connection, so kill_process refuses it.
   */
  broker?: boolean;
  /**
   * The command line, cut when very long.
   */
  command: string;
  /**
   * Share of one CPU over a short sample (100 is one whole core).
   */
  cpu_percent: number;
  memory_bytes: number;
  name: string;
  /**
   * The Devagotchi job this process belongs to, when it is one of this broker's.
   */
  operation_id?: string | null;
  parent_pid?: number | null;
  pid: number;
  /**
   * When it started, ms since the epoch.
   */
  started_at_ms?: number | null;
  /**
   * The OS account it runs as.
   */
  user?: string | null;
}
export interface ReadInput {
  /**
   * Last sequence consumed, starting at zero. Reading never advances another reader.
   */
  cursor?: number;
  /**
   * Maximum raw bytes to return, 1024..=16384. Frames are at most 1024 bytes.
   */
  max_bytes?: number;
  operation_id: string;
}
/**
 * The broker's current operations revision.
 */
export interface Revision {
  revision: number;
}
export interface SearchPage {
  /**
   * True when the whole search is done; false with `next_cursor` when there is more.
   */
  complete: boolean;
  /**
   * Files whose contents this page read (contents searches).
   */
  files_searched: number;
  /**
   * Files this page could not search: unreadable, binary, or over 8 MiB.
   */
  files_skipped: number;
  /**
   * In path order: a directory's entries by name, each directory's contents where its name
   * falls. Within a file, by line.
   */
  matches: SearchMatch[];
  /**
   * Pass as `cursor`, with the same other inputs, for the next page.
   */
  next_cursor?: string | null;
  /**
   * The searched directory's canonical path.
   */
  path: string;
  /**
   * Why this page stopped early: `max_results` or `time`.
   */
  stopped_by?: string | null;
}
export interface SearchMatch {
  /**
   * Lines just after the match, in order.
   */
  after?: string[];
  /**
   * Lines just before the match, in order.
   */
  before?: string[];
  /**
   * `file` or `dir`, for a name match.
   */
  kind?: string | null;
  /**
   * The matching line's number (1-based), for a content match.
   */
  line?: number | null;
  /**
   * Path relative to the searched directory, `/`-separated.
   */
  path: string;
  /**
   * The matching line, without its line ending; a very long line is cut.
   */
  text?: string | null;
}
export interface StartInput {
  /**
   * Exact argument vector. No shell parsing, interpolation, or added command text.
   */
  args?: string[];
  /**
   * One pipe for stdout and stderr, so the output keeps the order the program wrote it
   * (as a terminal shows it); every frame is then `stdout`. `run` sets it. Ignored for an
   * interactive operation, whose terminal already merges them.
   */
  combined_output?: boolean;
  /**
   * A command line for the machine's own shell (bash or sh on Linux, Windows PowerShell
   * on Windows), accepted by a broker that runs commands as its account. Give either
   * this or `executable` with `args`, not both.
   */
  command?: string | null;
  /**
   * Working directory (absolute, `~/...`, or relative to the workspace); defaults to the
   * workspace. A broker that runs only
   * granted programs keeps it within the workspace.
   */
  cwd?: string | null;
  /**
   * Execution deadline, not a client wait timeout. 1..=[`MAX_DEADLINE_MS`] milliseconds.
   */
  deadline_ms: number;
  /**
   * The program to run, with `args`: a name on PATH (`python3`, `cargo`), or a path,
   * absolute or relative to `cwd`. A script runs with its interpreter (its `#!` line, or
   * its extension: `.py` with Python, `.ps1` with PowerShell, `.sh` with bash, `.js` with
   * node), never through a file association; the operation's detail says what ran and why.
   * On a broker that runs only operator-granted programs the program must be granted.
   * Leave empty when `command` is given.
   */
  executable?: string;
  /**
   * A caller-created retry key. The same key and inputs return the same operation.
   */
  idempotency_key: string;
  /**
   * Keep the program's input open for `operation_input`: on a pseudo-terminal (`terminal`,
   * the default: prompts, REPLs, sign-ins, Ctrl-C) or on a plain pipe (`pipe`). Absent:
   * the program reads nothing (its stdin is the null device), as before.
   */
  interactive?: InteractiveInput | null;
}
export interface InteractiveInput {
  /**
   * Terminal width, 2..=500 columns (default 80).
   */
  cols?: number;
  /**
   * How an interactive operation takes input.
   */
  mode?: "terminal" | "pipe";
  /**
   * Terminal height, 1..=200 rows (default 24).
   */
  rows?: number;
}
export interface StartOutput {
  operation: OperationView;
  replayed: boolean;
}
export interface WaitInput {
  operation_id: string;
  /**
   * Return when output beyond this sequence exists (the cursor you have read up to), as
   * well as when the operation settles. Without it, return only when it settles.
   */
  output_after?: number | null;
  /**
   * Longest to wait, 1..=50000 milliseconds. The answer comes as soon as the operation
   * changes as asked, without polling; at the limit it comes back unchanged.
   */
  timeout_ms: number;
}
