# Desktop Commander parity: every capability working, traced, and better written

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-16, shippin channel: pre-release, no baseline; map every
  Desktop Commander capability against Devagotchi with evidence, make each one work as intended
  and better written, with traces that say why a run failed (".py files not properly
  associated for running"), then make the code canonical. No CI work, no PR stacks.
- PR: not_created (lands on main as each stage works)
- Branch / baseline: `increment-179/desktop-commander-parity` from `main` at `5104f35`
- Objective: an agent can do through Devagotchi everything it does through Desktop Commander,
  on Linux and Windows, locally (`devagotchi mcp`) and through a hub, and every failure says why.
- Non-goals: `crates/agenthub` (another session owns it); the kernel move (parked on
  `increment-178/kernel`, step 3).

## Step 1: the map (2026-11-16, run on this commit, Linux)

How it was run: `devagotchi mcp` over stdio against a fresh broker (account policy), about 70
tool calls with a driver script covering the happy path and the failures; then the repository's
`tests/e2e.py` (broker, adapter, hub on a throwaway Postgres 16, machine link enroll, revoke)
passed. Windows was read in code only: no Windows machine was reachable (the hosted hub lists no
machines for the account; Desktop Commander's devices Gus and dabo are offline).

### Works, run here

| Desktop Commander | Devagotchi | Evidence |
|---|---|---|
| read_file (offset, length, tail) | read_file | text, line ranges, `start_line: -1` tail, byte offset, binary as base64, clear errors for a missing file or a directory |
| read_multiple_files | read_multiple_files | one read and one per-file error in one answer |
| (image read) | read_image | PNG returned as an image; a text file refused with `not_an_image` |
| write_file (rewrite, append) | write_file | overwrite, create, append, `create_dirs`, base64, SHA-256 guard refuses a stale write |
| edit_block | edit_file | exact match; 0 or several matches refused with the count; `expected_replacements` |
| create_directory, list_directory, move_file, get_file_info | same names | depth, paging cursor, symlink reported with its target, move refuses to replace without `overwrite` |
| start_process, read_process_output, interact_with_process, force_terminate, list_sessions | run, operation_start, operation_wait, operation_read, operation_input, operation_resize, operation_cancel, operation_list | exit codes, `timed_out`, a still-running handoff, Python REPL in a terminal (`print(6*7)`), pipe input, Ctrl-C, cancel with SIGTERM then SIGKILL, another conversation refused without `take_over` |
| (remote devices) | hub routing and machine link | `tests/e2e.py`: E2E PASSED, every file and operation tool through the hub and through an enrolled link |

### Broken, with evidence

1. **A symlinked program runs as the wrong program.** `operation_start` with
   `/root/.cargo/bin/cargo --version` printed "This is the version for the rustup toolchain
   manager". The broker canonicalises the path and spawns the link's target, so the program
   sees `rustup` as its name. `devagotchi start` lists the cargo grant as "rustup" too.
2. **`run` loses the order of its output.** `echo 1; echo 2 >&2; echo 3; echo 4 >&2` returned
   `2 4 1 3`; the tool says "in the order written". stdout and stderr are two pipes.
3. **Nothing runs past five minutes.** `deadline_ms` is capped at 300000 and `run`'s
   `timeout_seconds` at 300; a longer build is killed. Desktop Commander has no such cap.
4. **Scripts are not launched as scripts.** `operation_start` takes only an absolute executable
   (`python3` is refused, no PATH lookup) and refuses a `.py` with "Executable is not an
   executable regular file". On Windows only `.exe` and `.com` pass, and `run` hands a `.py`,
   `.ps1`, `.cmd` or `.bat` line to PowerShell, which opens a `.py` with whatever the file
   association names (an editor on many machines) and captures nothing. Only `.sh` has an
   interpreter route (Git Bash). (Windows: read in code.)
5. **No trace of why a run failed.** After about 70 calls, failures included, `broker.log` holds
   its five start-up lines and nothing else; the broker has no tracing. The reason lives only
   in each operation's `detail` string.

### Half-done

- `edit_file` on a CRLF file: `a\nb` does not match `a\r\nb`; Desktop Commander normalises.
- `~` and relative paths are refused by every file tool and `cwd`; Desktop Commander resolves both.
- `run` cuts long output mid-line.
- `devagotchi hub serve --help` says it routes "run and operation_start/get/read/cancel"; it
  serves every file tool too.

### Missing

- Search: file names and file contents (start_search, get_more_search_results, stop_search,
  list_searches).
- Processes the broker did not start: list_processes, kill_process.
- Settings over MCP: get_config, set_config_value (broker.json is edited by hand).
- write_pdf; read_file from a URL; usage stats and recent tool calls.

### Dead code

`cargo check --workspace --all-targets` (desktop app excluded: this container lacks its GTK
libraries): 19 never-used items in `devagotchi-hub` (for example `store::open`,
`machines/link/registry.rs` `reassign_owner` and `issue_owned_code`, `config.rs` `operator`,
`is_operator`, `site.rs` `asset_url`, `hosted_hub`) and an unused import in
`machine-engine/src/execution/platform/linux.rs:449`.

## Step 2: make it work, in this order

| Milestone | Task | Acceptance evidence | State |
|---|---|---|---|
| P1 | Tracing in the broker: one line per operation (admitted, spawned, how the program was resolved, exit or refusal with its reason) to `broker.log`, and the launch reason in every operation's `detail` | a refused file and a missing interpreter each leave a line naming the reason | done: `trace.rs`; `broker.log` shows `refused operation_start ... not_executable` and `program_not_found ... (15 directories searched)`, and `admitted`/`settled` lines with `Launch: ...` |
| P2 | Script launch: resolve a bare program name on PATH (PATHEXT on Windows, Store alias skipped); run `.py` with the Python launcher or python, `.ps1` with PowerShell `-File`, `.cmd`/`.bat` through cmd.exe, `.js` with node, `.sh` with bash; honour `#!`; say which interpreter ran it; never hand a script to the file association | Linux runs here; Windows unit tests on the resolver | done (Linux run; Windows in 14 resolver tests, not run on Windows): `launch/` |
| P3 | Keep the program's own name: spawn the path as found after checking the canonical target, so `cargo` runs cargo | `cargo --version` prints cargo | done: prints `cargo 1.97.0` |
| P4 | `run` in order: one pipe for stdout and stderr | the 1 2 3 4 probe | done: `1\n2\n3\n4\n` |
| P5 | Long jobs: the deadline cap is a day; `run` defaults to an hour | a job past five minutes | done: `deadline_ms` 3600000 accepted, 90000000 refused naming the day cap; a 330 s `run` (see receipts) |
| P6 | Paths: `~` and CRLF-aware edits | probes | not started |
| P7 | Search: names and contents, paged, through the broker | probes | not started |
| P8 | Processes and settings: list and stop processes; read and change broker settings | probes | not started |

## Verification receipts

Step 1 driver and logs: kept outside Git. `tests/e2e.py --bin-dir target/debug` with
`DEVAGOTCHI_HUB_STORAGE_URL=postgres://postgres@127.0.0.1:55432/e2e`: E2E PASSED.

Step 2, fixes P1 to P5 (2026-11-16, Linux, this branch): `cargo test` for machine-engine
(115, including 14 launch tests), mcp (26) and contracts passed; the layout ratchet passed
after splitting `execution/output.rs` and `broker/input.rs` out. Probes through `devagotchi
mcp`: `cargo --version` by name printed cargo's version; `./hello.py a b` ran with
`/usr/bin/python3`; a non-executable `#!/usr/bin/env python3` file ran with python3; `python9`
and `data.xyz` were refused with `program_not_found` and `not_executable`; `run` returned
`1 2 3 4` in order and ran `./hello.py x` through python3. `tests/e2e.py` (hub on Postgres,
link) passed again on this code. Windows: not run (no Windows machine reachable).

## Delivery

Implementation: map, and fixes P1 to P5. Validation: run here, Linux. Push: branch. PR: not_created.
Main: not_verified.
**DELIVERY INCOMPLETE: NOT MERGED TO MAIN**
