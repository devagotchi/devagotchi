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

Every shared machine tool has one description and the same arguments on both surfaces; the hub adds `machine`
(B1, [the pattern](../patterns/machine-capability.md)). Since B1 the hub's `operation_*` tools take
`machine` beside their arguments, not `{broker, input}`, and a known tool given bad arguments
answers `invalid_arguments` as a tool error. `crates/hub/src/machines/tests/parity.rs` holds both.

`operation_heartbeat(machine, operation_id)` is a hosted authority operation, so it is
available only on an account-backed hub. It renews the original interactive admission
with current credentials and `proc.heartbeat` policy; it creates no process, spends no
additional grant use and does not renew a task lease or revive expired authority.
Its receipt contains only the acknowledged operation ID and deadline. The existing
local adapter has no hosted grant to renew. This is the preserved source's explicit
process heartbeat mapped to the existing admission/Refresh protocol.

Account-backed file mutations additionally take `idempotency_key`; local IPC does not.
`get_file_info(sha256=true)` obtains a whole-file guard; partial `read_file` pages omit
SHA-256. Account writes require a checksum or `mode=create`, move/delete require the
source checksum, and edit keeps its exact text/count guard. `delete_file` removes one
regular file, never a directory or link. Batch/image reads use one admission, and access
failure aborts disclosure of the composite result. Search uses only in-root ignore files.
External editors do not join the broker mutation gate; these guards are not filesystem-wide CAS.

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

## Mutation ordering

Desktop Commander gives no ordering between two calls that change files. Devagotchi's broker
orders `write_file`, `edit_file`, `create_directory`, `move_file` and `delete_file` through one gate per
broker, held from path resolution through the preconditions, the change and the answer
(`crates/machine-engine/src/broker/files.rs` has the whole contract in its module docs).

- Two writes guarded by the same `expected_sha256` cannot both succeed: the second answers
  `changed_since_read`. Of two `mode: create` writes or two moves onto one path, one answers
  `already_exists`; of two edits of the same `old_text`, one answers `edit_mismatch`. Disjoint
  edits and unconditional appends both land, each answering the checksum of what it left.
- A write that leaves the checksum as it was does not use it up: a later write guarded by
  that checksum can still succeed.
- One gate, not one per path, so unrelated mutations wait too. That is for correctness, not
  throughput. Reads (`read_file`, `read_multiple_files`, `list_directory`, `get_file_info`,
  `search_files`), operations and settings never take it.
- The gate belongs to the blocking worker once the change starts, so a caller that goes away
  cannot let another mutation in half way. The broker answers each local connection in a task
  of its own, so neither a client that hangs up nor an MCP cancellation withdraws a request
  the broker already has; only the broker's shutdown does.
- **`mutation_busy`.** A local client (`devagotchi mcp`, and the machine link that carries a
  hub's calls) waits 5 s for a file tool's answer (`devagotchi_ipc::IO_TIMEOUT`) and then
  reports `ipc_timeout`. A mutation that has not got the gate within 3 s is refused with
  `mutation_busy` and has changed nothing, so a queued change never lands after that IPC
  deadline. Try it again. A change that got the gate but runs past the deadline by itself (a
  very large file, a slow disk) still lands; after `ipc_timeout`, read the file before
  assuming it did not. At most 8 mutations wait at once; one more is refused `mutation_busy`
  at once, so queued changes (with the 16 held `operation_wait`s) never take all of the
  broker's 32 local connections.
- **MCP cancellation does not withdraw a change.** An MCP client that cancels a file call
  (through `devagotchi mcp` or a hub) gets `request_cancelled`: only that adapter stops
  waiting, and the request it already sent goes on in the broker. A change cancelled while it
  was queued still gets the gate if it frees within 3 s, and then lands after its client
  stopped waiting, with no answer to anyone. After `request_cancelled`, read the file before
  assuming the change did not happen.
- Limits: this is not a filesystem-wide compare-and-swap. External editors, commands the
  broker runs, and a second broker do not take the gate. Reads can see a move or an append
  half done. A change made through a hub's grant has a retry identity and a receipt in the
  machine journal, so a retry returns the first answer; a local change through `devagotchi
  mcp` has neither. There is no rollback. Whole-file replacement is by rename; a create on a
  filesystem without hard links (FAT32, exFAT, some network and Android storage) writes the
  new file in place. Under a hub grant a move never overwrites: delete the destination with
  its own `expected_sha256` first.

Checked by `broker/tests/file_mutations.rs` (gate coverage, reads and queries not gated,
cancellation before and during a change, release after an error or panic, `mutation_busy`
before the client's deadline and past the queue cap) and by `tests/file_mutation_concurrency.py`: a broker and two
`devagotchi mcp` adapters race barrier-released pairs over ten scenarios and check both answers
and the bytes on disk. It passes when every check holds and every scenario overlapped at least
once; how many pairs overlapped is a measurement. CI's e2e job runs it on every OS in the matrix.

## What Devagotchi has that this list does not

`run` (one call), `operation_get`, `operation_wait`, `operation_resize`, `read_image`,
`read_url` as its own tool, and `broker_capabilities`. Through a hub: `whoami`, `machines`,
`machine_add`, `machine_update`, `machine_inspect`, plus the board, connections and kernel
tools, which are not a desktop shell.

## Readiness

Desktop Commander's remote device shows `online` only while its channel is joined and its
local MCP child is alive ([teardown 3.5](../../docs/desktop-commander-teardown.md),
`src/remote-device/remote-channel.ts:627-679`). Devagotchi's `machines` says `online` while a
machine's link is connected, which does not show that its broker answers. That is a separate
call: `machine_inspect` through a hub, `broker_capabilities` through `devagotchi mcp`. It asks
the broker itself, waits at most 3 seconds (`inspection_timeout`: readiness unknown), and
returns its build, OS, policy and the requests it answers under that policy, with no paths or
grants.

| Capability | Devagotchi | Conformance case |
|---|---|---|
| Is the machine there and answering | `machines` (link), then `machine_inspect` / `broker_capabilities` (broker) | `tests/adapter_probe.py` (granted and account brokers); `tests/hub_probe.py` through `tests/e2e.py` (granted, account, unknown, not enrolled, offline, revoked, a stopped broker behind a live link, and after it restarts); `crates/hub/src/machines/tests/inspect.rs` (each outcome audited) |

## Not in this document's work

The missing rows above are not implemented here. The smallest ones (`stop_search`,
`list_searches`) need a server-side search session, which Devagotchi's search deliberately
does not keep. `write_pdf` and Office formats are the large gap and still wait on a choice
of renderer (the parity plan, P10).
