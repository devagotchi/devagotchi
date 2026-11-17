# Desktop Commander parity

What a person can do through [Desktop Commander](https://github.com/wonderwhy-er/DesktopCommanderMCP)
and what Devagotchi does instead. This is a gap list. It does not add the missing tools.

The Desktop Commander side is the snapshot under `reference/DesktopCommanderMCP`
(`src/server.ts` advertises the tools below). That tree is a partial snapshot
([audit/README.md](../../audit/README.md)); a tool that is not in it is not claimed to be absent
from upstream. Devagotchi's side is the tools `devagotchi mcp` registers
(`crates/mcp/src/adapter/`) and the same calls through a hub
(`crates/hub/src/machines/`, `crates/hub/src/mcp/`). The earlier working notes are
[the parity plan](../dev/plans/desktop-commander-parity.md). `write_pdf` (P10) is still open.

Devagotchi names a job by `operation_id`. Desktop Commander names a terminal session by pid.
That is a different shape, not a missing stop or read.

## Tool by tool

| Desktop Commander | What it does | Devagotchi | Gap |
|---|---|---|---|
| `get_config` | Server config: shell, blocked commands, allowed directories, limits, telemetry | `get_config` | Different keys: workspace, exec policy, grants, output and operation limits. No `blockedCommands`, `defaultShell`, `allowedDirectories` or telemetry switch |
| `set_config_value` | Change one config key, including clearing allowed directories | `set_config_value` | Only `max_concurrent`, `max_operations` and `output_limit_bytes`. Workspace and policy stay the operator's |
| `read_file` | A local path or a URL, line pages, and PDF, Excel, DOCX and images | `read_file`, plus `read_url` and `read_image` | Text, bytes and images. No URL inside `read_file`. No PDF, Excel or DOCX rendering |
| `read_multiple_files` | Many paths; one failure does not fail the batch | `read_multiple_files` | Present. Images come back from `read_image`, not inline in this call |
| `write_file` | Write or append text, including a DOCX from markdown headings | `write_file` | Text or base64, with a SHA-256 guard. No DOCX-from-markdown |
| `write_pdf` | Create or change a PDF | — | Missing |
| `create_directory` | Create a directory tree inside allowed directories | `create_directory` | Present. Bounds are the workspace and exec policy, not an allowed-directory list |
| `list_directory` | A listing, depth 2 by default, 100 entries under each directory | `list_directory` | Depth 1–3 and a cursor. Not Desktop Commander's text listing |
| `move_file` | Move or rename inside allowed directories | `move_file` | Present. Replacing an existing file needs `overwrite` |
| `start_search` | Start a background search; returns a session id | `search_files` | One stateless call. The cursor is `next_cursor`, not a session id |
| `get_more_search_results` | The next page of a search session | `search_files` with `cursor` | Same tool, no session id |
| `stop_search` | Cancel a search session | — | Missing. A page finishes or the caller stops asking |
| `list_searches` | Active search sessions | — | Missing. Nothing is held on the server between pages |
| `get_file_info` | Metadata, including a text file's line count and Excel sheets | `get_file_info` | Kind, size, times, mode, link target, line count up to 8 MiB. No Excel sheet table |
| `edit_block` | Edit text, an Excel range, or DOCX XML | `edit_file` | Exact text, including the file's own line endings. No range mode, no Excel, no DOCX |
| `start_process` | A shell command or a REPL, with `timeout_ms` required | `operation_start`, and `run` for one shot | `operation_id` and `deadline_ms` or `interactive`. No REPL prompt detection in the result. `run` waits up to 50 s for the answer; the command itself defaults to an hour and can be asked to run up to a day |
| `read_process_output` | Output by pid, by line offset | `operation_read`, `operation_wait` | A byte cursor on the operation, not a line offset on a pid |
| `interact_with_process` | Type into a pid; optional wait for a prompt | `operation_input` | Text and named keys for the operation. The caller has to own it (`take_over` for another conversation). No built-in wait for a prompt |
| `force_terminate` | Kill the terminal session by pid | `operation_cancel` | Present, by operation |
| `list_sessions` | Terminal sessions by pid, and whether one is blocked | `operation_list` | The caller's operations, not raw pids, and no blocked-on-a-prompt flag |
| `list_processes` | Processes on the machine, with CPU and memory | `list_processes` | Present on an account broker. Tags a job's processes with its operation. Refuses a granted broker |
| `kill_process` | Kill a process by pid | `kill_process` | Present on an account broker. Refuses the broker and a job's root (that one is `operation_cancel`) |
| `get_usage_stats` | Success, failure and timing, from a local history file | `get_usage_stats` | Counts by tool and failure. Locally, this MCP connection. Through a hub, the caller's audit. Not Desktop Commander's file |
| `get_recent_tool_calls` | Recent calls with arguments and output | `get_recent_tool_calls` | Tool, target and outcome. Not the full arguments and output, and no `since` filter |
| `give_feedback_to_desktop_commander` | Open a feedback form with usage attached | — | Missing. There is no product-feedback tool |
| `track_ui_event` (handled, not listed in `tools/list`) | Widget telemetry | — | Missing. There is no MCP-Apps UI layer |

## What Devagotchi has that this list does not

`run` (one call), `operation_get`, `operation_wait`, `operation_resize`, `read_image`, and
`read_url` as its own tool. Through a hub: `whoami`, `machines`, `machine_add`,
`machine_update`, plus the board, connections and kernel tools, which are not a desktop
shell.

## Not in this document's work

The missing rows above are not implemented here. The smallest ones (`stop_search`,
`list_searches`) need a server-side search session, which Devagotchi's search deliberately
does not keep. `write_pdf` and Office formats are the large gap and still wait on a choice
of renderer (the parity plan, P10).
