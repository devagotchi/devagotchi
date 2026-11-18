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
- `run` cuts long output mid-line. (Fixed in P6.)
- `devagotchi hub serve --help` says it routes "run and operation_start/get/read/cancel"; it
  serves every file tool too. (Fixed in P6.)

### Missing

- Search: file names and file contents (start_search, get_more_search_results, stop_search,
  list_searches). (Built in P7.)
- Processes the broker did not start: list_processes, kill_process. (Built in P8.)
- Settings over MCP: get_config, set_config_value (broker.json is edited by hand). (Built in P8.)
- write_pdf (P10, waiting on the user's choice); read_file from a URL (built in P9 as `read_url`); usage stats and recent tool calls (built in P9).

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
| P6 | Paths: `~` and relative paths in every file tool and `cwd` (one rule, `files/given.rs`); edits that match the file's own line endings; `run` keeps the tail from a whole line | probes | done: `read_file ~/tilde.txt`, `write_file p9/rel.txt` (under the workspace), `cwd: "~"` and `cwd: "p9"` ran there, `~bob/x` refused with the reason; `alpha\nbeta` edited a CRLF file and kept `\r\n`; the 3000-line tail starts at `line-1788` |
| P7 | Search: names and contents, paged, through the broker (`search_files`, `files/search.rs`; gitignore-aware walk, binary and >8 MiB files skipped, stateless cursor, 5 s per page) | probes | done: names skip hidden and gitignored unless `include_ignored`; contents with `glob: *.rs` and one line of context; case-sensitive and regex finds; 34 matches over 4 pages of 10 with none lost or repeated; a bad regex, a missing directory and a forged cursor each refused with the reason, and broker.log traces each as `refused search_files` |
| P8 | Processes and settings (`get_config`, `set_config_value`, `list_processes`, `kill_process`; `broker/settings.rs`, `broker/system.rs`). The workspace, exec policy and grants stay the operator's; the three limits change live and are saved to broker.json; processes only on an `account` broker; never Devagotchi's own processes; a job's root goes to `operation_cancel` | probes | done: get_config named the file, policy, account (`root`, from the OS, not `USER`) and home; `max_concurrent` 4 to 6 applied, was written to broker.json and survived a broker restart; `workspace`, `colour` and 99 refused with the reason; lowering `max_operations` to 5 retired 36 kept jobs to 5 at once; list_processes tagged a job's `bash` and both `sleep`s with its `operation_id`; kill_process refused the job's root (pointing to operation_cancel), the broker and the adapter, stopped a job's child with SIGKILL and an outside `sleep` with SIGTERM (exit -15), and said `process_not_found` for a missing PID; a granted broker refused list_processes; broker.log traces each change, stop and refusal |
| P9 | `read_url` (`broker/system/web.rs`: the machine fetches, so its localhost is reachable; http and https only; paged with `Range`, skipped by hand when a server ignores it; only on an `account` broker), `get_recent_tool_calls` and `get_usage_stats` (shapes in `contracts/history.rs`; through a hub from the audit by subject, migration 0031 indexes it; through `devagotchi mcp` from the connection's last 1000 calls) | probes, e2e | done: read_url read a 55 KB UTF-8 page in 3 pages byte for byte, after a redirect and with a 206 server and a plain 200 server; binary came back as base64; a 404 came back with its body; `file:`, a closed port and a non-address were refused with the reason; HTTPS worked direct and through the container's proxy; a granted broker refused it; broker.log traces each read. Recent calls listed a missing file as `path_not_found`, a still-running `run` as `running`, and two paths as `/etc/hostname and 1 more`; usage tallied per tool and failure. Through the hub only the caller's own machines count as machines (another tenant's name or a board's project never does) |
| P10 | `write_pdf` | probes | waiting on the user: native, the machine's converter, or both |

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

Fix P6 (2026-11-16, Linux): machine-engine 118, mcp 26, hub 435 unit tests and the layout
ratchet passed; the P6 probes above ran through `devagotchi mcp`; `tests/e2e.py` passed again.

Fix P7 (2026-11-16, Linux): machine-engine 122, mcp 26, hub 435 unit tests and the layout
ratchet passed; the P7 probes ran through `devagotchi mcp`; `tests/e2e.py` passed with a new hub
check that `search_files` finds a line with its context on an enrolled machine. An older machine
refuses `file_search` as an unknown request, and the hub names `machine_update` for that.

Fix P8 (2026-11-16, Linux): machine-engine 125, mcp 26, hub 435 unit tests and the layout
ratchet passed (broker restart recovery moved to `broker/recover.rs`, API key scopes to
`api_keys/scope.rs`, the hub's machine routers composed in `machines`); the P8 probes ran
through `devagotchi mcp`; `tests/e2e.py` passed with hub checks for get_config, set_config_value,
list_processes and kill_process on an enrolled machine, and a granted broker's refusal. The
execution identity no longer includes the limits (they change live), so a retry key from before
this build is refused as `idempotency_conflict` once. rustfmt now runs on everything P1 to P8
touched, and the generated contract schema and desktop types are current
(`node scripts/generate-contracts.cjs --check`).

Fix P9 (2026-11-16, Linux): machine-engine 125, mcp 27, contracts 5, hub 436 unit tests (on
Postgres) and the layout ratchet passed; the P9 probes ran through `devagotchi mcp`;
`tests/e2e.py` passed with hub checks that read_url reads the machine's localhost page by page,
that a granted broker refuses it, and that get_recent_tool_calls and get_usage_stats show the
caller's own calls. Splits for the size cap: `operation_read` to `broker/output.rs`, the AI
capture types to `contracts/ai.rs`, three hosted-account tests to `tenancy/tests/accounts.rs`,
and `machine_names` (what whoami lists) to `tenancy/machines.rs`.

Retry lifecycle (increment 202, branch `increment-202/retry-history` from `main` at `219e008`,
2026-11-18, Windows): the Nov 6 retry work, ported to `crates/machine-engine` without its
legacy pieces (contract [DG-RETRY-1](../../docs/retry-lifecycle.md)). A retired key is a
permanent tombstone: the 4096 prune is gone and triggers refuse any delete, update or replace
of one. The grant list left the execution identity too (the limits left it in P8), so, as in
P8, retrying a key admitted before this build is `idempotency_conflict`; grants and cwd are
still checked live on every start. A refused start (`concurrency_limit_reached`,
`operation_limit_reached`) retires nothing; a smaller `max_operations` after a restart retires
down to it on the next start; `set_config_value` saves before it applies (`config_not_saved`
otherwise) and returns a journal error while retiring instead of ignoring it. Retirement moved
to `broker/history.rs` (broker.rs 1202 -> 1164 lines in the ratchet). Not ported: the pre-v2
replay, its `legacy_config_identity` and `v2:` prefix, and the harness's baseline mode
(docs/product-and-platform-plan.md S08 resolved as alpha). Evidence (Windows 11, this
branch): `cargo test -p devagotchi-machine-engine` 99 passed, with four new tests in
`broker/tests/retry.rs` and `journal::tests::retry_evidence_is_permanent_and_immutable`, all
five of which fail against `main`'s broker, settings and journal; the layout ratchet passed;
`tests/retry_lifecycle.py` (ported, baseline mode dropped) passed the full gate with 4097 real
retirements through `devagotchi broker run` and `devagotchi mcp` (4119 child effects, each
once) and `--quick` on the final build; `tests/e2e.py --no-hub` passed. Linux and the hub e2e
were not run for this increment, and the harness is not wired into CI.

Seen once: right after the broker was killed, `devagotchi mcp` reported "your broker is running
but did not answer within 20 s" while the old one was still shutting down; the next start worked.

## Delivery

Implementation: map, and fixes P1 to P9. Validation: run here, Linux; Windows not run.
Push: increment-179/desktop-commander-parity, and main fast-forwarded to it after each fix.
PR: none (no PR ceremony, by the user's instruction). Main: each fix is on main.
**DELIVERY INCOMPLETE: P10 (write_pdf, waiting on a choice) and step 3 remain**
