# The one-user edition: every requested capability, in order (increments 208 to 269)

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-18. Their order:
  1. every Desktop Commander tool;
  2. Shippin.ai sign-in and a full section to manage the hub;
  3. a true update path, where the UI shows a new release and offers to deploy it;
  4. a phone app truly tailored to Android;
  5. the popup on the computer, approval email that opens the browser, and plugins for VS Code
     and ChatGPT Desktop.

  Then the operator installs it on the phone, and we fix what they find. The commercial
  hosted version comes after. "Nothing was to be replaced, only extended and offered."
- PR: increment 208 is the first; each increment records its own PR
- Branch / baseline: `increment-208/open-output-schemas`, from `main` at `f1b48e8`
- Objective: one ordered program for the self-hosted, one-user edition. Each increment has
  exact files, the tests that prove it, and its dependencies. It is ordered so that it fits
  [the path](path.md): the path still decides what "done" means (Texas and the phone), and
  this page orders the work that gets there.
- Non-goals:
  - the cloud edition, and `shippinAI/devagotchi-cloud`;
  - managing other users or customers in the one-user build;
  - a chat screen inside the hub (chat happens in Claude, ChatGPT and Codex).
- Canonical architecture / related plans: [path](path.md),
  [Desktop Commander parity](desktop-commander-parity.md), [hub settings](hub-settings.md),
  [mobile experience](mobile-experience.md), [plugins](plugins.md),
  [free edition](free-edition.md), [self-hosted and provenance](self-hosted-and-provenance.md).
  Their milestones keep their detail; the rows below name them.

How this was made: six read-only gap maps, one per area, each re-checked claim by claim by a
second reader against the code at `f1b48e8`, then ordered. "Map I2", "update map U-H3" and
similar refer to those maps' increment numbers. Every claim carries a file:line in the maps;
the claims that shape the program are repeated here.

**A correction to what was believed on 2026-11-18.** The hub already serves almost every
Desktop Commander tool:
- `search_files`;
- `get_config` and `set_config_value`;
- `list_processes` and `kill_process`;
- `read_url`;
- `operation_list`, `operation_input` and `operation_resize`;
- `get_recent_tool_calls` and `get_usage_stats`;
- `machine_inspect`.

Their routers are `crates/hub/src/mcp/tools/mod.rs` and `crates/hub/src/machines/mod.rs:54-62`,
and `tests/hub_probe.py` asserts the list. The shorter list seen in claude.ai is a tool list it
cached between 2026-11-03 and 2026-11-14. The cached `list_directory` has no `cursor`, and
there is no `connection_call`.

Based on `main` f1b48e8, which is also what the live hub runs (`/healthz` downloads.version `main-f1b48e8`). I read `.ai/README.md`, `.ai/docs/editions.md`, `.ai/patterns/layout.md` and `crates/devagotchi/tests/layout/oversize.txt`. The six verified maps are the evidence base. I checked the phase 0 blockers again myself; my findings are below.

## Phase 0 blockers, checked again

| Blocker | Evidence |
|---|---|
| Docker join leaves nothing running | `crates/hub/src/machines/join/mod.rs:224-247`: the compose file is one `enroll` service on `debian:stable-slim` that runs `apt-get install -y curl ca-certificates` and then `join.sh`. `join.sh:13` exits unless `xz` **and `systemctl`** exist. A slim Debian container has neither, so it fails at `xz` first. Even if it got past that, it is a one-shot service with no broker or link left running. The join line is `docker compose -f compose.yml up -d` (`crates/hub/src/mcp/machines.rs:160-162`). |
| `/dist/` returns 404 while the instructions point there | curl today: `/dist/` 404, `/dist` 404, `/dist/SHA256SUMS` 200. The pointer is at `crates/hub/src/mcp/tools/machines.rs:133` ("Download the archive and SHA256SUMS from {hub}/dist/") and `docs/identity.md:162`. |
| Manual join never starts the broker or the link | `crates/hub/src/mcp/tools/machines.rs:133` ends at `devagotchi link enroll … --broker <broker-endpoint>`. It never says `devagotchi start` / `broker run` or `link run`. `crates/link/src/lib.rs:22` (USAGE) says enroll "then exits" and `run` must be started. |
| Plugin hub URL is the placeholder; Codex cannot set the hub | Plugins map: `plugins/devagotchi/.mcp.json:5`, `plugins/devagotchi/.codex-plugin/plugin.json:1-20` (no userConfig), `plugins/devagotchi/.claude-plugin/plugin.json:28-30`. |
| False README lines | `README.md:133` says the store is "Postgres … or SQLite". The hub refuses `sqlite:` URLs (`crates/hub/src/store/tests.rs:19-20`) and ADR 0005 is Postgres everywhere (`editions.md`). `README.md:5` says "Windows, macOS, and Linux are required targets", but the build targets are Linux and Windows on x86_64 and aarch64 (`dist-workspace.toml:13`), and `path.md:157` (8.2) already flags README:5, 7 and 117 as wrong. |
| `read_file` / `read_multiple_files` rejected by claude.ai | Desktop Commander parity map: closed `FileContent` (`crates/contracts/src/files.rs:40-67`) plus a stale claude.ai schema snapshot. |

Three more constraints shape the program:
- **No shell in the hub image.** It is distroless (`deploy/compose/Dockerfile:7-8`, `ENTRYPOINT ["/usr/local/bin/devagotchi"]`). A Docker machine therefore has to pass its join code through `DEVAGOTCHI_ENROLL_CODE` (`link/src/lib.rs:22`), not through a pipe.
- **CI never tests one-user mode.** The image CI's compose stack runs `DEVAGOTCHI_HUB_TENANCY=multi` (`.github/workflows/image.yml:122-124`), and so does every phone browser test.
- **Files at or near the 500-line cap.** At their ratchet (cannot grow): `contracts/src/lib.rs` 490, `machine-engine/src/broker.rs` 1153, `machines/join/mod.rs` 492, `site.rs` 708, `pages.rs` 742, `routes.rs` 680, `web/approvals.rs` 713, `mcp/connect.rs` 802, `providers.rs` 937, `identity/auth.rs` 686, `orgs.rs` 1082. Near the cap: `hub_settings.rs` 488, `notify/push/mod.rs` 477, `web/email.rs` 476, `serve/mod.rs` 459, `ops/audit.rs` 452. The program puts new code in new files, and each listed file changes by 0 lines or fewer.

## How to read the program

- **Phases follow the operator's priority (0 to 5).** The lanes in the last section let several phases run at the same time.
- **Test commands.**
  - Hub tests need Postgres: `DEVAGOTCHI_TEST_POSTGRES_URL=postgres://postgres@127.0.0.1:55432/<db> cargo test -p devagotchi-hub --lib <module>` (throwaway server: `tests/throwaway_postgres.sh`).
  - End to end: `python tests/e2e.py --bin-dir target/debug`, which drives `tests/adapter_probe.py` and `tests/hub_probe.py` (ci.yml:232).
  - Browser: `DEVAGOTCHI_HUB_STORAGE_URL=… python tests/<x>_browser.py --channel chrome --devagotchi target/debug/devagotchi` (ci.yml:262-278).
  - Deploy: `sudo bash tests/deploy_scripts.sh devagotchi-hub:ci` (ci.yml:171).
  - Layout: `cargo test -p devagotchi --test layout`. Every increment must keep it green.
- **Sizes.** S is about a day, M a few days, L about a week or more.

---

## Phase 0: blockers on the live hub (milestone M0: the operator can join a PC and use every served tool from claude.ai)

### 208: Open every tool output schema and prove it with a real broker (S)
- **Delivered as (2026-11-18, this branch):** the advertised output schemas are opened where
  the tools are listed. `crates/mcp/src/outputs.rs` is applied to the hub's router and the
  adapter's router. The Rust types and the broker wire stay closed: a wire answer is
  version-gated, and an advertised answer may grow.
  - Tests: `no_output_schema_refuses_a_field_added_later` (hub) and
    `every_output_schema_is_open_and_every_input_closed` (adapter).
  - Still open: the `hub_probe.py` result checks below, and the live receipt after you
    reconnect claude.ai.
- **Why:** claude.ai rejects `read_file` and `read_multiple_files` because the output types are closed (`deny_unknown_fields` becomes `additionalProperties:false`) and the broker now adds `identity` (4b4f2f8). `list_directory` (`next_cursor`, `name_lossy`) and interactive `operation_*` answers (`interactive`, 0c7df78) break the same way.
- **Scope:**
  - Remove `deny_unknown_fields` from the output types only: FileContent, FileEntry, FileListing, FileWritten, FileInfo, OperationSummary, OperationsPage, OperationView, StartOutput, OutputFrame, OutputPage, BridgeError, InteractiveView, InputSent, SearchMatch, SearchPage, BrokerCapabilities, ConfigView, ProcessInfo, ProcessPage, ProcessKilled, UrlContent, ToolCall, RecentCalls, Tally, UsageStats.
  - Input types and the wire enums (RequestBody, ResponseBody, WireResponse, HubFrame, LinkFrame) keep it.
  - Rule written into `crates/contracts/src/lib.rs` module docs: output types only gain optional fields.
  - Refusals become text-only `CallToolResult::error`, the precedent at `mcp/handler.rs:39-42` and `tenancy/gate.rs:56-60`.
  - Regenerate the schema and the desktop TS types.
  - After deploy, the operator refreshes the claude.ai connector (no `tools.listChanged`, `handler.rs:85-112`) and records the results.
- **Files:** `crates/contracts/src/{files,lib,interactive,search,system,web,history}.rs`, `schemas/broker-contracts.schema.json`, `apps/desktop/src/generated/contracts.ts`, `crates/hub/src/mcp/tests/schema.rs`, `crates/hub/src/machines/tests/` (new `output_schemas.rs`), `crates/mcp/src/adapter/tests.rs`, `tests/hub_probe.py`, `.ai/docs/desktop-commander-parity.md`.
- **Acceptance:**
  - `node scripts/generate-contracts.cjs --check` passes.
  - New test `every_output_schema_is_open` walks `Hub::router().list_all()` and the adapter router and fails on any `additionalProperties:false`, `$defs` included.
  - `hub_probe.py` validates successful `read_file`, `read_multiple_files`, `list_directory` (a page cut short) and interactive `operation_get` results against the advertised `outputSchema` (`output_schema::assert_result`), through `python tests/e2e.py --bin-dir target/debug`.
  - Live receipt in the parity doc: after the refresh, the tool list shows `search_files`, `get_config`, `machine_inspect` and `operation_input`, and `read_file` returns content on Windows and Linux machines.
- **Depends on:** nothing.

### 209: Docker machine join that leaves a running machine (M)
- **Why:** The `docker` line `machine_add` hands out cannot succeed, and even if it did it would leave nothing running (see the blocker table).
- **Scope:**
  - Replace the compose template with three services, all on the hub's own image `ghcr.io/devagotchi/devagotchi:<tag>`. The tag comes from the downloads VERSION: `main-<sha7>` becomes `:sha-<sha7>`, `X.Y.Z` becomes `:X.Y.Z` (image.yml:211-214). A `dev` build refuses with a plain message.
    - `broker`: `devagotchi broker run --state-dir /var/lib/devagotchi/broker`, `restart: unless-stopped`, workspace bind `./work:/work`, the IPC socket in a shared volume.
    - `enroll`: `link enroll` with `DEVAGOTCHI_ENROLL_CODE`, `restart: "no"`.
    - `link`: `link run`, `depends_on: enroll (service_completed_successfully)`, `restart: unless-stopped`.
  - `link enroll` exits 0 when the state directory already holds a key for the same hub and device, so a second `docker compose up -d` is harmless.
  - `broker run` writes its default config on first run, as `start` does.
  - The link refuses `HubFrame::Update` on a container (`DEVAGOTCHI_MACHINE_KIND=container`) with reason "container: re-run the join line or `docker compose pull && up -d`". Auto-update skips such machines; the hello reports the kind, and the hub learns it first.
  - Move the template out of `join/mod.rs` (492 lines) into `join/compose.yml` with `include_str!`.
- **Files:** `crates/hub/src/machines/join/mod.rs`, `crates/hub/src/machines/join/compose.yml` (new), `crates/link/src/lib.rs`, `crates/link/src/session.rs`, `crates/machine-engine/src/service.rs` (or wherever `broker run` reads its config), `crates/hub/src/machines/link/updates.rs`, `tests/compose_join.py`, `.github/workflows/image.yml`, `docs/identity.md`.
- **Acceptance:**
  - Hub unit test: the compose file names `:sha-<sha7>` for a `main-<sha7>` build and holds the code only in `environment`.
  - Link unit test: re-enroll with an existing key exits 0 without a network call.
  - `tests/compose_join.py` gains a Docker leg in the image.yml job. It runs the docker line exactly as `machine_add` returns it; the machine shows online in `machines`; `run echo hi` returns `hi`; `docker compose up -d` a second time keeps it online; `machine_update` on it returns the container refusal and no push loop starts.
- **Depends on:** nothing.

### 210: Manual join that works, and `/dist/` that answers (S)
- **Why:** The manual instruction points at a 404 and never starts the broker or the link.
- **Scope:**
  - `GET /dist/` (and `/dist`) answers a `text/plain` index: VERSION, then the `SHA256SUMS` lines with full URLs.
  - The `manual` text in `machine_add` names the exact archive URL per OS and architecture, the checksum, `devagotchi start --background` (broker), `link enroll` with the code on stdin, then `devagotchi link run --state-dir …`. For Windows it names the zip and `.exe` steps.
  - Route both `/dist` paths through a `join::dist_routes()` merged in `routes.rs`, replacing its existing `/dist/{name}` line so the net change is 0.
- **Files:** `crates/hub/src/mcp/tools/machines.rs`, `crates/hub/src/machines/join/mod.rs` (only `dist_routes`; the template moved out in 209), `crates/hub/src/web/routes.rs` (net 0), `crates/hub/src/web/tests/machines.rs`, `docs/identity.md`, `tests/compose_join.py`.
- **Acceptance:**
  - Hub test: `/dist/` lists every `SHA256SUMS` file as a URL under the hub. `machine_add.manual` contains `link run` and `start`, and no URL ending in `/dist/`.
  - `compose_join.py` follows the manual text on the runner (download, `sha256sum -c`, start, enroll, run) and the machine comes online.
- **Depends on:** 209 (same file; land after it).

### 211: Plugins point at the person's hub; Codex can set it; README tells the truth (S)
- **Why:** The plugin and Codex sign in to `hub.example.com`; the README claims SQLite and macOS.
- **Scope:** Plugins map P0.
  - README rewrite: `plugins/README.md:38,49-51`; `.ai/dev/plans/plugins.md:20,37,95-99,118-126`; CLI help (`crates/devagotchi/src/main.rs:32,214`); `crates/mcp/src/connect.rs:3,265`, text only with net 0 lines since it is at its 802 ratchet.
  - Codex hub: `devagotchi connect codex --hub URL` is the documented path.
  - Skills: one line telling the person to run `/plugin configure devagotchi@devagotchi` when `whoami` fails against `hub.example.com`.
  - Remove `app.devagotchi.com` from the userConfig description (G15).
  - `README.md:133`: Postgres only. `README.md:5,7`: the platform statement becomes "hub on Linux (x86_64/arm64); machines on Linux and Windows (x86_64/arm64)", from `dist-workspace.toml:13`.
- **Files:** `plugins/README.md`, `plugins/devagotchi/.claude-plugin/plugin.json`, `plugins/devagotchi/skills/{machines,agenthub}/SKILL.md`, `crates/devagotchi/src/main.rs`, `crates/mcp/src/connect.rs`, `crates/hub/src/mcp/tests/plugins.rs`, `README.md`, `.ai/dev/plans/plugins.md`, `.ai/dev/plans/path.md` (8.2).
- **Acceptance:**
  - `cargo test -p devagotchi-hub --lib mcp::tests::plugins` passes, with new asserts that the README contains neither "uses the hosted hub" nor "plugin_tests.rs".
  - A new `crates/devagotchi/tests/readme_truth.rs` asserts that README.md has no `SQLite` and no `macOS … required`, and that every target in `dist-workspace.toml` is named.
  - `claude plugin validate plugins/devagotchi` passes.
  - With a throwaway `CODEX_HOME`, `devagotchi connect codex --hub https://app.devagotchi.com --no-check` followed by `codex mcp list` shows that hub.
- **Depends on:** nothing.

---

## Phase 1: every Desktop Commander tool through the hub

### 212: Wire-contract split before new frames (S, refactor only)
- **Why:** `contracts/src/lib.rs` is at 490 lines and `machine-engine/src/broker.rs` is at its 1153 ratchet. Every later frame or variant would push them over.
- **Scope:**
  - Move RequestBody, ResponseBody, WireResponse, HubFrame and LinkFrame from `lib.rs:292-442` to `crates/contracts/src/wire.rs`, re-exported from `lib.rs`.
  - Add `RequestBody::domain() -> Domain {Operations, Files, System}`, so the broker dispatch at `broker.rs:293-306` matches on the domain and new variants add no line to `broker.rs`.
  - No behaviour change.
- **Files:** `crates/contracts/src/{lib,wire}.rs`, `crates/machine-engine/src/broker.rs`, `crates/devagotchi/tests/layout/oversize.txt` (lower the `broker.rs` number).
- **Acceptance:** `cargo test -p devagotchi-contracts -p devagotchi-machine-engine`; `node scripts/generate-contracts.cjs --check` unchanged; layout test passes with the `broker.rs` entry lowered.
- **Depends on:** 208.

### 213: One addressing rule, and an optional machine when only one is online (S)
- **Scope:** Map I2.
  - `operation_*` accept `machine`; `broker` stays an alias, refused when both are given and differ.
  - `machine` becomes optional on every machine tool when exactly one machine is online; otherwise the refusal lists the names.
  - Update the tool descriptions and the site's MCP instructions.
- **Files:** `crates/hub/src/mcp/{schema,hub}.rs`, `crates/hub/src/mcp/tools/{operations,machines}.rs`, `crates/hub/src/mcp/callers/mod.rs`, `crates/hub/src/machines/{files,run,search,system}.rs`, `tests/hub_probe.py`.
- **Acceptance:** Hub tests in new `crates/hub/src/machines/tests/addressing.rs`:
  - one machine online: `read_file` without `machine` works;
  - two online: refused, naming both;
  - `operation_start` works with `{machine}` and with `{broker}`;
  - `{machine:a, broker:b}` is refused.
  - `e2e.py` passes.
- **Depends on:** 210 (the same `mcp/tools/machines.rs`).

### 214: Call handles for every machine call (`get_tool_result`, `list_remote_tool_calls`) (M)
- **Scope:** Map I5.
  - **Default chosen: in memory, not in Postgres.** A bounded per-owner table (1,000 entries, 15-minute expiry) in the hub. A hub restart drops every link anyway, so durable rows buy nothing and would deepen the Postgres dependency.
  - A call past its synchronous budget answers `{call_id, state: running}`. For `run` and `operation_start` the call_id is the operation_id.
  - The adapter keeps the same table locally.
- **Files:** `crates/hub/src/machines/calls.rs` (new), `crates/hub/src/machines/history.rs`, `crates/hub/src/mcp/hub.rs`, `crates/mcp/src/adapter/history.rs`, `crates/contracts/src/history.rs`.
- **Acceptance:** Hub test with a stub link that answers after the budget:
  - `write_file` returns a call_id;
  - `get_tool_result` later returns its `FileWritten` and `list_remote_tool_calls` lists it;
  - another account's call_id is `not_found`.
  - Adapter test for the same flow.
- **Depends on:** 212.

### 215: Search sessions (`start_search`, `get_more_search_results`, `stop_search`, `list_searches`) (M)
- **Scope:** Map I3.
  - A bounded in-memory session in the broker (cap 8 per broker, 10-minute idle expiry) over `files/search.rs`.
  - New RequestBody variants, gated on `machine_inspect.supported_requests`. An older link refuses with `machine_update` named instead of dropping its session (`link/session.rs:181`).
- **Files:** `crates/contracts/src/{search,wire}.rs`, `crates/machine-engine/src/files/search.rs`, `crates/machine-engine/src/broker/search_sessions.rs` (new), `crates/mcp/src/adapter/file_tools.rs`, `crates/hub/src/machines/search.rs`, `tests/hub_probe.py`, `tests/adapter_probe.py`.
- **Acceptance:**
  - Broker tests: the cap, the expiry, pages in order with no repeats.
  - Through `e2e.py`, locally and through the hub: an id comes back at once; `stop_search` ends the session and `list_searches` shows it stopped; a link built before this change (the previous build in `--update-builds`) is refused naming `machine_update`.
- **Depends on:** 212.

### 216: `interact_with_process` waits for output; sessions say when they are blocked (M)
- **Scope:** Map I4. `operation_input` gains `wait_ms` and returns the output written after the input. `OperationSummary` and `OperationView` gain a heuristic `waiting_for_input`. These are optional output fields, legal after 208.
- **Files:** `crates/contracts/src/interactive.rs`, `crates/contracts/src/lib.rs` (only the OperationView field), `crates/machine-engine/src/broker/{input,output}.rs`, `crates/mcp/src/adapter/terminal.rs`, `crates/hub/src/mcp/callers/mod.rs`.
- **Acceptance:** Broker test on Linux and Windows CI: a Python REPL gets `print(6*7)` plus enter with `wait_ms=2000` and returns `42` in one call; `operation_list` shows `waiting_for_input=true` at the `>>>` prompt. The same flow runs through the hub in `hub_probe.py`.
- **Depends on:** 212.

### 217: Config parity for the owner (allowed directories, blocked commands, default shell, line limits) (M)
- **Scope:** Map I7.
  - Settable through `set_config_value` by the owner of an account broker.
  - Widening (a new allowed directory, removing a blocked command) needs an approved `approval_request` id.
  - A granted broker still refuses.
  - Saved to `broker.json`.
- **Files:** `crates/contracts/src/system.rs`, `crates/machine-engine/src/broker/settings.rs`, `crates/mcp/src/adapter/system_tools.rs`, `crates/hub/src/machines/system.rs`.
- **Acceptance:** Broker and hub tests: a blocked command is refused with its reason; a newly allowed directory can be read; settings survive a broker restart; widening without an approval id is refused.
- **Depends on:** 212.

### 218: `edit_block` parity for text: near-miss report and line ranges (S)
- **Scope:** Map I8. When `old_text` is not found, report the closest match with its line. Add a line-range form guarded by `expected_sha256`. Both go through the mutation gate.
- **Files:** `crates/machine-engine/src/files.rs`, `crates/machine-engine/src/broker/files.rs`, `crates/contracts/src/files.rs`, `crates/mcp/src/adapter/file_tools.rs`, `crates/hub/src/machines/files.rs`.
- **Acceptance:** Broker tests: a near miss reports the line; a range edit with a stale sha is refused. `python tests/file_mutation_concurrency.py --devagotchi target/debug/devagotchi --work-root target --source-commit $(git rev-parse HEAD)` passes.
- **Depends on:** 212.

### 219: `read_file` conveniences (`is_url`, image pointer) (S)
- **Scope:** Map I12. `is_url:true` routes to `read_url`; an image path answers with a pointer to `read_image`.
- **Files:** `crates/mcp/src/adapter/file_tools.rs`, `crates/hub/src/machines/files.rs`.
- **Acceptance:** Adapter and hub tests: both results validate against the open schema (208's test helper).
- **Depends on:** 218 (same files).

### 220: `machine_disconnect` (Desktop Commander's `shutdown`) and `review_call` (M)
- **Scope:** Map I6.
  - `machine_disconnect {machine, revoke?}`: a stop frame over the link marks the machine offline. `revoke=true` needs an approved approval and then calls `Hub::revoke_own_machine` (`tenancy/machines.rs:200`), then `Links::revoke_owned`.
  - `review_call`: creates an approval card. On Run it executes through `Hub::call` as the person and stores the result under a call_id from 214; on Cancel it executes nothing.
- **Files:** `crates/contracts/src/wire.rs`, `crates/link/src/session.rs`, `crates/hub/src/machines/link/{serve,registry}.rs`, `crates/hub/src/mcp/tools/machines.rs`, `crates/hub/src/machines/review.rs` (new), `crates/hub/src/web/approval_page.rs` (from 256, as the Run/Cancel surface).
- **Acceptance:**
  - `e2e.py` leg: `machine_disconnect` and `machines` shows the machine offline; `link run` again brings it back; `revoke=true` without an approval is refused.
  - Hub test: `review_call` shows a card; Run executes and `get_tool_result` returns the answer; Cancel runs nothing; both are in the audit.
- **Depends on:** 214, 256.

### 221: Read PDF, DOCX and XLSX in `read_file`, `get_file_info` and `read_multiple_files` (L)
- **Scope:** Map I9. Broker side, behind a `documents` cargo feature that is on in release builds. Pages stay under 24 KiB and under the 1 MiB link frame. Every candidate crate must pass `cargo deny`; BSD-2-Clause and OFL are not allowed (`deny.toml:41-60`).
- **Files:** `crates/machine-engine/src/files/documents/` (new: `mod.rs`, `pdf.rs`, `xlsx.rs`, `docx.rs`), `crates/machine-engine/Cargo.toml`, `crates/contracts/src/files.rs` (FileReadInput gains `sheet`, `range`, `page`), `crates/mcp/src/adapter/file_tools.rs`, `crates/hub/src/machines/files.rs`, `deny.toml` (only if a new licence passes review), `tests/fixtures/documents/`, `tests/hub_probe.py`.
- **Acceptance:**
  - Locally and through the hub on Windows and Linux CI: PDF page 2 text; `Sheet2!B2:C3`; the DOCX outline.
  - `get_file_info` lists the sheets and the page count.
  - `cargo deny check` passes, and the binary size change is recorded in the PR.
- **Depends on:** 218.

### 222: Office writes (DOCX from markdown, XLSX range writes, DOCX XML edits) (M)
- **Scope:** Map I11. All through the mutation gate.
- **Files:** `crates/machine-engine/src/files/documents/`, `crates/contracts/src/files.rs`, `crates/mcp/src/adapter/file_tools.rs`, `crates/hub/src/machines/files.rs`.
- **Acceptance:** Round-trip tests on Windows and Linux:
  - a DOCX written from markdown reads back through 221;
  - an XLSX range write reads back exactly;
  - a DOCX edit that breaks well-formedness is refused and the file is unchanged.
- **Depends on:** 221.

### 223: `write_pdf` (P10) (L)
- **Scope:** Markdown to PDF, plus modify mode (insert and delete pages), with the renderer from Operator decision 2.
- **Files:** `crates/machine-engine/src/files/documents/pdf_write.rs` (new), `crates/machine-engine/Cargo.toml`, `crates/contracts/src/{files,wire}.rs`, `crates/mcp/src/adapter/file_tools.rs`, `crates/hub/src/machines/files.rs`, `deny.toml`, `.ai/dev/plans/desktop-commander-parity.md`, `.ai/docs/desktop-commander-parity.md`.
- **Acceptance:**
  - On Windows, Linux x86_64 and Linux arm64, locally and through the hub: markdown becomes a 2-page PDF whose text `read_file` (221) pages; delete and insert give the expected page order; an existing output is refused without `overwrite`.
  - `cargo deny` passes, and every target in `dist-workspace.toml` cross-builds.
- **Depends on:** 221 and Operator decision 2.

### 224: Live claude.ai receipts for all 30 Desktop Commander tools (S)
- **Scope:** Run every Desktop Commander flow from claude.ai against the operator's Windows PC and a Linux machine (path 5.1).
- **Files:** `.ai/docs/desktop-commander-parity.md`, `verification/README.md`.
- **Acceptance:** A receipt for every upstream tool, and none of them shows "structured content does not match".
- **Depends on:** 213-223.

---

## Phase 2: Shippin.ai sign-in and the owner's management section (one user only)

### 225: The one-user hub hides the multi-user surfaces (S)
- **Delivered as (2026-11-18, branch `increment-225/one-user-hides-multi-user`, stacked on
  208):** `web/one_user/mod.rs` decides whether the organisation pages are mounted;
  `routes.rs` merges `one_user::orgs(multi_tenant)` where it merged `orgs::routes()` (net 0:
  `router` takes `multi_tenant`, passed by `serve::app` and the two web fixtures). `/admin`
  drops the People section and chip, You drops the Organisations card, and Boards drops its
  organisations link too (`app/home.rs`, the same dead link). "17 clients" is
  `devagotchi_connectors::all().len()`.
  - Tests: `web::one_user::tests::a_one_user_hub_hides_the_multi_user_surfaces` and
    `a_hosted_hub_keeps_the_multi_user_surfaces` (Postgres).
  - Still open: the JSON API (`/api/v1/orgs/…`, `/api/v1/invites/accept`,
    `tenancy/orgs/api`) is still mounted on a one-user hub; it was not in this scope.
- **Why:** The operator ruled on 2026-11-18 that the standalone build manages no other users. Today `/invite` answers 200 on the live hub; `/admin` shows People (`hub_settings.rs:224,231,251-255`) and You shows an Organisations card (`app/me.rs:170-197`).
- **Scope:** When `!config.multi_tenant()`:
  - drop the People section and its chip from `/admin`;
  - drop the Organisations card from You;
  - `/orgs*`, `/invite` and `/assets/invite.js` get the unknown-path 404. Gate this where the orgs router is merged in `routes.rs` (net 0), not inside `orgs.rs` (1082 lines, at its ratchet).
  - Replace the literal "17 clients" with `devagotchi_connectors::all().len()`.
- **Files:** `crates/hub/src/web/hub_settings.rs`, `crates/hub/src/web/app/me.rs`, `crates/hub/src/web/routes.rs` (net 0), `crates/hub/src/web/one_user/tests.rs` (new, under a `web/one_user/` module).
- **Acceptance:**
  - New Postgres test on a single-tenancy hub: `/admin` has no `id="people"` and no `tenancy.` inputs; `/orgs` and `/invite` return 404; `/app/me` has no `id="orgs"`.
  - `web::shippin_tests` and the operator tests still pass on a multi-tenant hub.
- **Depends on:** nothing.

### 226: Shippin on a one-user hub: web tests, and no silent link by matching email (M)
- **Why:** A Shippin or generic OIDC sign-in with a verified email links to an existing identity that has the same address, without asking (`identity/accounts/identities.rs:259-281`). Only Google goes through the password-confirm page (`providers.rs:689-699`).
- **Scope:**
  - A single-tenancy web fixture with the stand-in Nomen (`tests/fake_nomen.py` or the in-process stand-in).
  - The confirm-to-link page applies to every provider: change the `== "google"` check, net 0 lines in `providers.rs`.
  - An `operators` `email:` entry keeps working only for providers whose email is verified, as today, and is documented on the Shippin card.
- **Files:** `crates/hub/src/web/providers.rs` (net 0), `crates/hub/src/identity/accounts/identities.rs`, `crates/hub/src/identity/tests/providers_single.rs` (new).
- **Acceptance:** New Postgres tests:
  - the owner links Shippin from `/account`, then signs in with it;
  - an unknown subject gets 403 `no_account`;
  - a Shippin sign-in whose verified email matches the owner's Google identity is not linked without confirmation.
  - `identity::accounts::tenants::tests::a_one_owner_hub_makes_no_account_for_a_strangers_sign_in` still passes.
- **Depends on:** 225.

### 227: OIDC hardening (D1b) (M)
- **Scope:** Free-edition D1b.
  - Discovery's authorization and token endpoints must be on the issuer's origin. Loopback stand-ins are allowed as today; Google is exempt.
  - Check `azp` and `iat`.
  - `OidcVerifier` uses `redirect(Policy::none())`, and `jwks_uri` must be on the issuer's origin.
- **Files:** `crates/hub/src/identity/id_token.rs` (new), `crates/hub/src/identity/tests/id_token.rs` (new), `crates/hub/src/web/providers.rs` and `crates/hub/src/identity/auth.rs` (call sites only, net 0 or less).
- **Acceptance:** The D1b list (`free-edition.md:174`) as tests:
  - two audiences without `azp` refused; a foreign `azp` refused; `iat` before the sign-in refused;
  - a cross-origin token endpoint refused; Google accepted; a loopback stand-in accepted;
  - a 302 during discovery refused; a cross-origin `jwks_uri` refused.
- **Depends on:** 226.

### 228: `connect-nomen` keeps the hub in hub mode (M)
- **Why:** `connect-nomen` rewrites the config to `auth.mode nomen`, which takes away `/admin`, passkeys and keys (`identity/connect/mod.rs:206-214`). Yet the `/admin` Shippin card tells the owner to use it (`sign_in_cards.rs:75`).
- **Scope:** Save the Shippin web app as the `nomen` sign-in setting (issuer, client id, enabled) and set `auth.nomen` inside `auth.mode hub` for `/mcp` bearer tokens. Never write `auth.mode nomen` or a `web` section. Retiring `nomen_login` waits until its phone-session test (`web/tests/nomen_login.rs:344`) moves to hub-mode sessions; not in this increment.
- **Files:** `crates/hub/src/identity/connect/mod.rs`, `crates/hub/src/identity/connect/tests.rs`.
- **Acceptance:** A `connect-nomen` test against the stand-in: the config keeps `auth.mode hub`; after start, `/admin` opens for the owner, the Shippin card shows on with the client id, `/auth/login` shows "Continue with Shippin", and a Nomen bearer token is accepted at `/mcp`.
- **Depends on:** 226.

### 229: `/admin` → Shippin: Check and status (S)
- **Scope:** A Check button runs discovery with no redirects, checks the issuer matches, shows the exact redirect URI `<public_url>/auth/provider/nomen/callback`, and shows when Shippin last signed someone in.
- **Files:** `crates/hub/src/web/hub_settings_shippin.rs` (new), `crates/hub/src/web/hub_settings_shippin_tests.rs` (new), `crates/hub/src/web/sign_in_cards.rs`.
- **Acceptance:** With the stand-in: OK for a matching issuer, "names itself X" for a mismatch, a 302 refused; a non-owner gets 404.
- **Depends on:** 227.

### 230: Connect to Shippin from the browser, and the network settings on the page (M)
- **Scope:**
  - "Connect this hub to Shippin" runs the `connect-nomen` device flow in process and shows the user code to approve on the phone.
  - `shippin.api` and `organization_id` move to the page, with the token sealed in Postgres.
  - The offers switch moves from `site.json` to the live catalog.
- **Files:** `crates/hub/src/web/hub_settings_shippin.rs`, `crates/hub/src/identity/connect/mod.rs`, `crates/hub/src/config/shippin.rs`, `crates/hub/src/config/live/catalog.rs`.
- **Acceptance:**
  - Stand-in tests: Connect shows a code; approving it saves the provider, and `/auth/login` shows the button with no file edited; the token is stored sealed and never shown again.
  - Operator receipt: a fresh self-hosted hub signs in through the real Shippin issuer (see Operator decision 3).
- **Depends on:** 228, 229.

### 231: `/account` → Sign-in and security (passkeys, linked sign-ins, sessions) (M)
- **Scope:**
  - List passkeys and remove one (`remove_passkey`).
  - List linked sign-ins and unlink one (a new store function).
  - Never remove the last way in.
  - "Sign out everywhere else" through `end_sessions`, keeping this session.
- **Files:** `crates/hub/src/web/account_security.rs` (new), `crates/hub/src/web/account_security_tests.rs` (new), `crates/hub/src/identity/accounts/identities.rs`, `crates/hub/src/web/pages.rs` (one link, net 0).
- **Acceptance:** Tests:
  - a removed passkey no longer signs in;
  - an unlinked Shippin identity no longer reaches the account;
  - removing the last way in is refused;
  - sign-out-elsewhere ends the other cookie and keeps this one.
- **Depends on:** 226.

### 232: You → AI apps: see and disconnect connected AI clients (M)
- **Scope:** List the OAuth clients that hold tokens for this account, with family and last use. Disconnect one (a new revoke by client) or all (`revoke_connections`, `oauth/mod.rs:519`, which has no product caller today).
- **Files:** `crates/hub/src/web/app/ai_clients.rs` (new), `crates/hub/src/identity/accounts/oauth/mod.rs` (at its 647 ratchet: the new function goes in `oauth/revoke.rs`), `crates/hub/src/web/app/me.rs` (one link).
- **Acceptance:** Hub test: two clients are listed; disconnecting one makes its next `/mcp` call 401 while the other still works; "disconnect all" ends both.
- **Depends on:** 225.

### 233: `/admin` → Activity (M)
- **Scope:** A paged view of the audit chain over `Audit::after` and `calls_of` (`ops/audit.rs:125,144`). Filters by machine and client, a chain-verified badge (`verify`, :227), phone layout.
- **Files:** `crates/hub/src/web/hub_settings_activity.rs` (new), `crates/hub/src/web/hub_settings_activity_tests.rs` (new).
- **Acceptance:** Hub test: a `run` call appears with its machine and client; the machine filter narrows the list; a tampered row shows the chain broken; a non-owner gets 404.
- **Depends on:** 225.

### 234: Token lifetimes and AI client limits on `/admin` (S)
- **Scope:** Move `auth.tokens` (access_secs, refresh_secs, code_secs, max_clients, claim_secs) into the live catalog under "AI connections". A value set in the file pins the setting.
- **Files:** `crates/hub/src/config/live/catalog.rs`, `crates/hub/src/config/live.rs`, `crates/hub/src/config/auth.rs`.
- **Acceptance:** `config::live::tests` key coverage passes; a `refresh_secs` change applies to the next token; a pinned value is read-only on the page.
- **Depends on:** 225.

### 235: Machine settings card in the browser (M)
- **Scope:**
  - `/app/machines/{name}` gains a Settings card that reads `ConfigView` through the same routed request as `get_config`.
  - It edits the limits and the 217 settings through the `set_config_value` path, with a same-origin check, owner only and audited. Widening triggers the 217 approval inline.
  - Exec policy and workspace stay read-only, with the `broker.json` lines to change.
  - Rename is excluded: the name is the routing and enroll key.
- **Files:** `crates/hub/src/web/app/machine_settings.rs` (new), `crates/hub/src/web/app/machine.rs` (one card call), `crates/hub/src/web/app/machine_settings_tests.rs` (new).
- **Acceptance:** Hub test with a real broker: the card shows the limits; saving `max_concurrent=2` changes `get_config` and `broker.json`; an out-of-range value shows the broker's refusal; the change is in the audit.
- **Depends on:** 217, 225.

---

## Phase 3: a real update path, shown in the UI, deployed with one click

### 236: The hub knows and shows its build, commit and channel (S)
- **Scope:** Update map U-H1.
  - `image.yml` and `release.yml` pass `DEVAGOTCHI_REVISION=${{ github.sha }}`; `crates/devagotchi/build.rs` exports it; `main.rs` hands build and revision to `hub serve` through its options.
  - `compose.yaml` passes `DEVAGOTCHI_CHANNEL` and `DEVAGOTCHI_TRACK`, which `install.sh` already writes (`install.sh:374-375`).
  - `/healthz` gains `build`, `revision` and `channel`. This is public, like `downloads.version` already is.
  - MCP `serverInfo.version` becomes the build. `site.rs:605-608` is at its ratchet, so the change is one expression.
  - A Hub card on `/admin` in a new file shows build, linked commit, channel, the build offered to machines, and an "alpha, pre-release" badge (path.md 6.4).
- **Files:** `crates/devagotchi/{build.rs,src/main.rs}`, `.github/workflows/{image,release}.yml`, `deploy/compose/compose.yaml`, `crates/hub/src/serve/options.rs` (or the existing options struct), `crates/hub/src/ops/health.rs`, `crates/hub/src/site.rs` (net 0), `crates/hub/src/web/hub_settings.rs` (one chip and one call), `crates/hub/src/web/hub_settings_hub.rs` (new), `crates/hub/src/web/hub_settings_hub_tests.rs` (new), `crates/hub/site/default/content/messages.json`.
- **Acceptance:**
  - Hub tests: `/healthz` build and revision equal the values passed at start; `/admin` has `id="hub"` showing them; MCP `initialize` reports `serverInfo.version` equal to the build.
  - After deploy, `curl https://app.devagotchi.com/healthz` gives a revision equal to the GHCR `:main` revision label.
- **Depends on:** nothing (it can run alongside phases 0-2).

### 237: Graceful shutdown and a compose healthcheck (H1) (S)
- **Why:** Every hub update restarts the hub. Today SIGTERM kills it abruptly (`serve/mod.rs:346-347` only handles Ctrl-C), with no close frame to links, and compose has no hub healthcheck.
- **Scope:** Handle SIGTERM and Ctrl-C the same way: drain HTTP, send links a close, then exit. Add `/readyz`. Add a compose `healthcheck` on the hub (the hub binary answers `devagotchi hub health`, since there is no curl in distroless).
- **Files:** `crates/hub/src/serve/shutdown.rs` (new; `serve/mod.rs` gets one line), `crates/hub/src/cli/mod.rs`, `deploy/compose/compose.yaml`.
- **Acceptance:**
  - Hub test: SIGTERM on a test server closes a connected stub link with a close frame inside 5 s.
  - `tests/deploy_scripts.sh`: `docker compose up --wait` waits on the hub's health.
- **Depends on:** nothing.

### 238: The host updater reports to the hub and takes approved requests (M)
- **Scope:** Update map U-H2.
  - `update.sh` writes `status.json`: current and newest digest and revision, last check, last result and reason, the bad list, the pre-deploy backup path. The hub mounts it read-only.
  - `hub.update_mode` (`auto` or `ask`) is a live `/admin` setting. The hub writes it into a second mounted directory owned by uid 65532; `update.sh` reads it, falling back to `.env`.
  - Ask mode records "available" and does not deploy.
  - A root `devagotchi-hub-update.path` unit watches for `request.json` and runs `update.sh --apply <digest>`. That deploys only the digest the track points at after a fresh pull, and only if it is not on the bad list.
  - The hub gets no `docker.sock`; `cap_drop ALL` stays.
  - Defaults: see Operator decision 1 and the defaults section (new installs `ask`, existing hosts `auto`).
- **Files:** `deploy/compose/{update.sh,install.sh,compose.yaml,README.md}`, `deploy/proxmox/hub-up.sh`, `crates/hub/src/config/live/catalog.rs`, `crates/hub/src/ops/update_status.rs` (new), `tests/deploy_scripts.sh`.
- **Acceptance:** New cases in `sudo bash tests/deploy_scripts.sh devagotchi-hub:ci`:
  - (a) ask mode: a new digest is not deployed and status says `available`;
  - (b) a request for the track's digest deploys it, and status says `deployed` with the backup path;
  - (c) a request for any other digest, or a bad one, changes nothing;
  - (d) an unhealthy digest rolls back, joins the bad list, and status says `rolled_back` with the reason;
  - (e) with no mode file the behaviour equals today's.
  - `bash -n` and `shellcheck` pass on every script.
- **Depends on:** 236.

### 239: Check the image's provenance before any deploy (S)
- **Scope:** Update map U-H7. `update.sh` runs `gh attestation verify oci://…@digest --repo devagotchi/devagotchi` (or `cosign verify-attestation`), installed by `install.sh`, and refuses an unattested digest, with the reason in `status.json`.
- **Files:** `deploy/compose/{update.sh,install.sh}`, `tests/deploy_scripts.sh`, `.github/workflows/image.yml` (a verify step after publish).
- **Acceptance:** `deploy_scripts.sh`: a locally built image without an attestation is refused with the reason. `image.yml`: the just-published `:sha-<x>` passes the same verify step.
- **Depends on:** 238.

### 240: The hub detects a newer release by itself (M)
- **Scope:** Update map U-H3.
  - Every 6 hours, behind the live setting `updates.check` (on; shown on the Hub card as an outbound call).
  - Release channel: GitHub `releases/latest`, resolved to a commit. Main channel: the anonymous GHCR `:main` manifest's revision label.
  - Compare commits, never names. "Available" means a different commit that the compare API says is ahead. `status.json` from 238 wins when present.
  - The repository comes from config; no hostname in code.
- **Files:** `crates/hub/src/ops/release_check.rs` (new), `crates/hub/src/ops/mod.rs`, `crates/hub/src/serve/mod.rs` (one spawn line), `crates/hub/src/config/live/catalog.rs`, `crates/hub/src/config/live.rs`.
- **Acceptance:** Unit tests against a local stub HTTP server:
  - a commit ahead gives available;
  - the same commit under a new tag gives none;
  - a re-cut tag with a new commit gives available;
  - a 302, a 5xx or a malformed body gives none with one logged error;
  - with the setting off, the stub sees zero requests.
- **Depends on:** 236.

### 241: A clear new-release notice and a one-click hub update, phone first (M)
- **Scope:** Update map U-H4.
  - "Hub update available: <running> → <new>" is the first Needs-you item on Home and on the `/admin` Hub card, with one push per commit to the owner.
  - The card links release notes (the release body, or a compare link on main).
  - The Update hub button: same-origin check, owner only (`hub_settings_owner::owner`), audited as `hub.update`, writes the 238 request for the exact digest shown.
  - The page then shows Updating and polls `/healthz` until the revision changes, or shows "rolled back: <reason>".
  - With no host updater (a plain binary), the card shows the command instead of the button.
  - The route lives in `web/hub_update.rs`, merged into the admin router, not `routes.rs`.
  - Layout: one column, 44 px targets, readable at 360 px.
- **Files:** `crates/hub/src/web/hub_update.rs` (new), `crates/hub/src/web/hub_update_tests.rs` (new), `crates/hub/src/web/hub_settings_hub.rs`, `crates/hub/src/web/app/home.rs` (one call), `crates/hub/src/notify/push/update.rs` (new; `push/mod.rs` is at 477), `crates/hub/src/ops/audit.rs` (action name only), `crates/hub/site/default/content/messages.json`, `crates/hub/src/web/hub.css`.
- **Acceptance:**
  - Hub tests: with a release available, Home shows the notice and `/admin` the button; a cross-origin POST gets 403 and writes nothing; a non-owner is refused; the owner's POST writes `request.json` naming the digest and one `hub.update` audit row.
  - Unit test: two checks of the same commit queue one push (dedupe state).
  - Browser: the new `tests/android_browser.py` (from 245) at 390 px sees the notice, taps Update hub and sees Updating.
- **Depends on:** 238, 239, 240.

### 242: Machines follow, visibly (M)
- **Scope:** Update map U-H5.
  - A machine row or page shows "on X, hub offers Y" when they differ. Update now names its target and hides when the machine is current.
  - "Update all" covers the out-of-date online machines, within the limit of 20, audited each.
  - The hub update card says how many machines will follow and how many are offline.
  - No new frames.
  - The link's `/dist` client uses `redirect(Policy::none())` (D2b).
- **Files:** `crates/hub/src/web/app/machine.rs`, `crates/hub/src/web/app/machines_update.rs` (new), `crates/hub/src/web/app/home.rs`, `crates/hub/src/machines/link/updates.rs`, `crates/link/src/update.rs`, `crates/link/src/update_tests.rs`.
- **Acceptance:**
  - Hub test (new file): an out-of-date machine shows both builds and "Update to <v>"; a current one shows no button; Update all sends `HubFrame::Update` only to out-of-date machines, with one audit row each.
  - Link test: a 302 on `/dist/SHA256SUMS` fails the update and leaves nothing under `updates/`.
- **Depends on:** 236, 241 (the `home.rs` hook).

### 243: The whole update path proven in one CI job (M)
- **Scope:** Update map U-H8.
  - Compose stack in ask mode on image A **with single tenancy**, one linked machine on build A.
  - Retag the track to B. Then: the hub shows B available; the owner's POST deploys it; the `/healthz` revision becomes B; the machine reconnects on B.
  - Negative case: an unhealthy B rolls back and the notice shows the failure.
- **Files:** `tests/hub_update_e2e.py` (new), `.github/workflows/image.yml`, `tests/compose_join.py`.
- **Acceptance:** The `image.yml` job is green. Its log shows the revision going A to B, the machine's hello build going A to B, and the audit rows `hub.update` and `machine.update_status done`. The negative case ends on A with `rolled_back`.
- **Depends on:** 237, 241, 242.

### 244: A plain-binary hub can update too (M, after the rest of this phase)
- **Scope:** Update map U-H6.
  - `install-devagotchi.sh --role hub` lays down a downloads directory and writes `downloads` into `hub.json`; today `healthz.downloads` is null there.
  - A root `.path` unit runs the new `--apply-hub-update` mode: sha256 and attestation checks are fatal; switch `current`; restart; check health; roll back on failure.
  - It writes the same `status.json` as 238.
- **Files:** `deploy/linux/{install-devagotchi.sh,README.md,ci-install-check.sh}`.
- **Acceptance:** `deploy/linux/ci-install-check.sh` (ci.yml:338):
  - a hub at build A with a request for B ends on B;
  - a broken B rolls back to A;
  - a wrong sha256 changes nothing;
  - `healthz.downloads` is not null after install.
- **Depends on:** 238, 241.

---

## Phase 4: an app made for Android, to a high visual bar

**Shared-file rule for this phase.** 246 makes the pure moves first: `manifest()` and `service_worker()` out of `site.rs` into `site/pwa.rs`, and the head and CSP builder out of `pages.rs` (lines 424-509) into `web/pages/head.rs`, with both ratchet entries lowered. It lands early so the other lanes rebase onto the split files.

### 245: Make the phone CI check honest, and start the Android browser suite (S)
- **Scope:**
  - Map A0: `tests/app_browser.py:372` waits on cache `devagotchi-app-2`, but the service worker uses `-3`. Read the name from `/sw.js`. Fix the docstring (Postgres).
  - Create `tests/android_browser.py` (Pixel 7, **single tenancy**, claimed owner) as the home of later Android asserts, so lanes stop colliding in `app_browser.py`.
  - Wire it in `ci.yml` beside :269.
- **Files:** `tests/app_browser.py`, `tests/android_browser.py` (new), `.github/workflows/ci.yml`.
- **Acceptance:** Renaming `FRAME` in `sw.js` locally makes `app_browser.py` fail at the offline wait. `android_browser.py` signs in as the one owner and opens `/app`, `/approvals` and `/app/me` at 412 px with no sideways scroll.
- **Depends on:** nothing.

### 246: Installable on Android: PNG icons, and the manifest on the sign-in page (M)
- **Scope:** Map A1, plus the moves described above.
  - Add `icon-192.png`, `icon-512.png`, a full-bleed `icon-maskable-512.png` and a monochrome `badge-96.png`, emitted with sizes measured by `png_size` (`site/discovery.rs:294`).
  - On `/auth/login` (and the manifest also on `/`): manifest link, `theme-color` and icon, with `manifest-src 'self'` and `img-src 'self'` added to the Kind::Plain CSP for those pages.
  - `background_color` comes from the `--bg` token.
- **Files:** `crates/hub/src/site.rs` → `crates/hub/src/site/pwa.rs` (new), `crates/hub/src/site/{model,assets}.rs`, `crates/hub/site/default/{site.json,tokens.json,assets/}`, `crates/hub/src/web/pages.rs` → `crates/hub/src/web/pages/head.rs` (new), `crates/hub/src/web/{sign_in,landing}.rs`, `crates/hub/src/web/app/tests.rs` (extend :247), `tests/fixtures/neutral-site/site.json`, `crates/devagotchi/tests/layout/oversize.txt` (numbers only go down).
- **Acceptance:**
  - `the_app_installs_at_app_and_opens_offline` asserts 192 and 512 `image/png` icons with measured sizes and a separate maskable PNG.
  - A new test: `/auth/login` has `<link rel=manifest>` and a CSP with `manifest-src 'self'`.
  - `android_browser.py`: CDP `Page.getInstallabilityErrors` is empty on `/auth/login` and `/app`.
- **Depends on:** 245.

### 247: A sign-in page that fits a phone (S)
- **Scope:** Map A2.
  - The sign-in switch stops using class `tabs` (`sign_in.rs:80`); scope the tab-bar CSS to `body.app > nav.tabs`.
  - Drop the bar's "Sign in" link that points back at the sign-in page.
  - Provider buttons, Shippin included, become full-width rows of at least 48 px.
- **Files:** `crates/hub/src/web/{sign_in,sign_in_cards}.rs`, `crates/hub/src/web/pages/head.rs` or `pages.rs` (net 0), `crates/hub/src/web/hub.css`, `crates/hub/src/web/tests/sign_in_phone.rs` (new), `tests/android_browser.py`.
- **Acceptance:** Unit test: `/auth/login` has no `class="tabs"` and no bar link to `/auth/login`. `android_browser.py`: every `a` and `button` on `/auth/login` is at least 44×44 and the switch text is at least 15 px.
- **Depends on:** 246.

### 248: Versioned assets, so a deploy reaches the phone at once (S)
- **Scope:** Map A3.
  - `hub.css`, `app.js`, `reveal.js` and `push.js` are referenced as `?v=<content hash>`.
  - The service worker's cache name derives from the same hash.
  - This defeats Cloudflare's `max-age=14400`, which was observed live.
- **Files:** `crates/hub/src/web/pages/head.rs`, `crates/hub/src/web/{routes,notify}.rs` (net 0), `crates/hub/src/web/app/mod.rs`, `crates/hub/src/site/pwa.rs`, `crates/hub/src/web/assets/sw.js`, `crates/hub/src/web/tests/assets.rs` (new), `tests/app_browser.py`.
- **Acceptance:** Rust test: the HTML references `/assets/hub.css?v=<hash>`, the hash changes with the file, and `/sw.js` carries the same hash. 245's offline wait still passes.
- **Depends on:** 246.

### 249: 48 dp tap targets and touch feel (S)
- **Scope:** Map A4. Row links on Overview, Machines and Boards become block links of at least 48 px; sheet rows at least 48 px; chips at least 40 px with a 48 px hit area; the Install and top-bar buttons at least 44 px. `:active` states and no tap highlight. The page still works without JS.
- **Files:** `crates/hub/src/web/hub.css`, `crates/hub/src/web/app/home.rs`, `crates/hub/src/web/pages.rs` (net 0), `tests/android_browser.py`.
- **Acceptance:** `android_browser.py` visits `/app`, `/app/machines`, `/app/boards`, a board, a ticket, `/approvals` and `/app/me`, and asserts every visible `a`, `button` and `summary` is at least 44×44, listing any failures, with no sideways scroll.
- **Depends on:** 246.

### 250: Visual system: depth, elevation, crisp type (M)
- **Scope:** Map A7.
  - Elevation tokens `--elev-1..3` per theme, as layered shadows.
  - Cards at elev-1, an Overview status hero at elev-2, the sheet at elev-3.
  - Tab bar: backdrop blur like the top bar (`hub.css:14-16`) and an active pill.
  - Display font for h1 and counts, tabular numerals, `-webkit-font-smoothing`.
  - A glow on status dots and a pulse while updating; `prefers-reduced-motion` honoured.
  - Dark `theme-color` metas (two `media`-scoped metas).
  - Tricks only: CSS, SVG, transforms and opacity. No web fonts, no images, no JS animation.
- **Files:** `crates/hub/site/default/tokens.json`, `crates/hub/src/site/style.rs`, `crates/hub/src/web/hub.css`, `crates/hub/src/web/app/home.rs`, `crates/hub/src/web/pages/head.rs`, `tests/android_browser.py`, `tests/requirements-browser.txt` (axe).
- **Acceptance:**
  - axe colour contrast: 0 violations on `/app`, `/approvals` and `/app/me`, light and dark, on a Pixel 7; 249's audit still passes.
  - Pixel 7 screenshots of `/app`, `/approvals`, a machine and the Go-to sheet, light and dark, attached to the PR for the operator to judge on the phone in 253.
- **Depends on:** 249.

### 251: Manifest extras: shortcuts, screenshots, monochrome icon (S)
- **Scope:** Map A8. Shortcuts (Approvals, Machines, Overview), narrow screenshots, a monochrome icon.
- **Files:** `crates/hub/src/site/pwa.rs`, `crates/hub/site/default/{assets/,site.json}`, `crates/hub/src/web/app/tests.rs`.
- **Acceptance:** The test asserts shortcuts to `/approvals` and `/app/machines`, and a narrow screenshot whose `png_size` matches its declared size. In 253, a long-press on the installed icon shows the shortcuts.
- **Depends on:** 246, 250.

### 252: CI proves the Android path on a one-user hub (M)
- **Scope:** Map A11, in `android_browser.py`:
  - installability on `/auth/login` and `/app`;
  - manifest icons decoded at their declared sizes;
  - the target audit;
  - axe on every app page;
  - the one owner's home, approvals and machine.
- **Files:** `tests/android_browser.py`, `tests/claim_browser.py`, `.github/workflows/ci.yml`.
- **Acceptance:** On main, the e2e log has PASS lines for each check. Removing the sign-in manifest link fails the job (proved once in the PR).
- **Depends on:** 246, 247, 249.

### 253: Real Android install receipt, and plans brought up to date (S)
- **Scope:** Map A5 plus A12.
  - `tests/android_device.py`: `adb forward` to the phone's Chrome, Playwright `connect_over_cdp`, installability, screenshots in both themes.
  - After the operator installs: `adb shell pm list packages | grep webapk` (a WebAPK, not a shortcut), the push test, the offline page in airplane mode, the machine-offline push.
  - Mark `mobile-navigation.md` and `phone-fit.md` merged (19db815 and acbc886 are on main). Add rows 245-255 to `mobile-experience.md`; P2g points here.
- **Files:** `tests/android_device.py` (new), `.ai/dev/plans/{mobile-experience,mobile-navigation,phone-fit}.md`.
- **Acceptance:** A receipt with device, Android and Chrome versions, the WebAPK package name, screenshots, and push, offline and machine-offline results, each marked pass or fail as observed.
- **Depends on:** 246, 247, 248 (the M1 milestone below), and 257 for the push part.

### 254: Machine control from the phone: run, watch output, cancel (L)
- **Scope:** Map A9.
  - A "Run a command" sheet on the machine page. `POST /app/machines/{name}/run` goes through the **same hub function** as MCP `run` and `operation_start`: same audit, same approval policy, same-origin check.
  - Job page `/app/machines/{name}/jobs/{op}` pages `operation_read` output and offers Cancel.
  - Recent jobs link to their job pages.
- **Files:** `crates/hub/src/web/app/job.rs` (new), `crates/hub/src/web/app/job_tests.rs` (new), `crates/hub/src/web/app/{machine,mod}.rs`, `crates/hub/src/web/hub.css`, `crates/hub/site/default/content/messages.json`, `tests/android_browser.py`.
- **Acceptance:**
  - Rust: a run is audited as the person, its output shows, it can be cancelled; another account's machine gives 404; a cross-origin POST gives 403.
  - `android_browser.py`: `echo hi` shows `hi` and exit 0 with no sideways scroll.
- **Depends on:** 213 (optional machine and addressing), 242 (the same `machine.rs`).

### 255: Live counts and machine state over SSE; job output once 254 exists (M)
- **Scope:** Map A10. `GET /app/events`: SSE, same-origin, signed in. `app.js` updates the badge and status dots in place, falls back to the existing refetch on visibility change, and stops while the page is hidden.
- **Files:** `crates/hub/src/web/app/events.rs` (new), `crates/hub/src/web/app/mod.rs`, `crates/hub/src/web/assets/app.js`, `crates/hub/src/web/pages/head.rs`.
- **Acceptance:** Rust: a subscriber gets the count change after `approval_request`, and never another account's events. `android_browser.py`: the badge goes from 0 to 1 without a reload.
- **Depends on:** 248 (254 only for job output).

---

## Phase 5: approvals by email to the browser, proven push, the on-computer popup, plugin views, VS Code and ChatGPT Desktop

### 256: A page for one approval that survives sign-in (S)
- **Scope:** Approvals map N1.
  - `GET /approvals/<project>/<id>` in a new file with its own router, merged with one line in `routes.rs` at net 0. It shows the card and posts to the existing `/approvals/decide`. A signed-out visitor goes to `return_to=` that same path; today the `#fragment` is lost.
  - Push and `approval_request` URLs switch to this path.
  - Fix the "approve it from the notification" copy (`approvals.rs:565`, net 0 or less).
- **Files:** `crates/hub/src/web/approval_page.rs` (new), `crates/hub/src/web/approval_page_tests.rs` (new), `crates/hub/src/web/routes.rs` (net 0), `crates/hub/src/agenthub/mcp/approvals.rs`, `crates/hub/src/web/approvals.rs` (copy only), `tests/android_browser.py`.
- **Acceptance:**
  - Tests: signed out, the page redirects with `return_to` set to the exact path and lands there after sign-in; a decided or unknown approval refuses and changes nothing; another account's approval gives "Not yours to decide"; the `approval_request` url is the new path.
  - `android_browser.py` decides an approval at 412 px.
- **Depends on:** nothing.

### 257: Android-grade notifications (S)
- **Scope:** Plugins P1 plus the safe part of mobile A6.
  - PNG `icon` and a monochrome PNG `badge` (from 246), plus `timestamp` and `vibrate`.
  - On approvals, a `Review` action. A tap on the action or the body opens the 256 page.
  - `notificationclick` focuses and navigates an open client before opening a new window (`sw.js:69-74`).
  - The approvals, `push.js` and push-title wording moves into `messages.json`.
  - **No approving from the notification shade.** See the defaults section.
- **Files:** `crates/hub/src/web/assets/{sw.js,push.js}`, `crates/hub/src/site/pwa.rs`, `crates/hub/src/agenthub/mcp/approvals.rs`, `crates/hub/site/default/content/messages.json`, `crates/hub/src/web/tests/push_payload.rs` (new).
- **Acceptance:**
  - Hub test: `/sw.js` names PNG icon and badge URLs that answer 200 `image/png`; the approval payload carries `actions` and the 256 URL.
  - Browser: `notificationclick` with an open client leaves the client count unchanged.
  - Receipt in 253: the Devagotchi glyph shows in the status bar, and Review opens the approval.
- **Depends on:** 246, 248, 256.

### 258: Notification channels, a verified address, and the list of devices (M)
- **Scope:** Approvals map N2.
  - `Preferences.approvals_by`: push on or off; email off, on, or only when no device was reached. Default: push, then email when no device was reached.
  - A Notifications card in a new file shows the verified address, or "Verify an address" or "Sign in with Shippin" to get one, and says when mail is not set.
  - It lists this account's push subscriptions with Remove (scoped to the subject).
  - "Notify this phone" becomes "Notify this device", which also covers desktop Chrome and Edge.
- **Files:** `crates/hub/src/identity/preferences/{mod,tests}.rs`, `crates/hub/src/web/app/notifications.rs` (new), `crates/hub/src/web/app/mod.rs`, `crates/hub/src/notify/push/subscriptions.rs`, `crates/hub/src/web/approvals.rs` (label, net 0 or less), `crates/hub/site/default/content/messages.json`.
- **Acceptance:**
  - Preference tests: an old row upgrades to the default; an unknown field survives a round trip.
  - Removing a device stops pushes to it (fake push service), and one account cannot remove another's.
  - A password-only account shows "Verify an address"; a verified Shippin or GitHub identity shows its address.
- **Depends on:** 225 (`me.rs`).

### 259: Approval email that sends you to the browser, sent durably (L)
- **Scope:** Approvals map N3, with the outbox default from Operator decision 4.
  - `Hub` holds a `MailCell` clone; `Mailer::send_text` becomes `pub(crate)`, so there is still one sender (`web/email.rs`).
  - `notify/approvals.rs` follows `approval.requested` with its own `signal_cursors` row (`notify:approvals`), so a restart loses nothing and sends no duplicate.
  - It sends push and/or one mail per the 258 preferences. Subject: "Approval needed: <title, control characters stripped, 80 chars>". The body says who asks, the kind and target, and the 256 link, and that replying does nothing. It never includes What, Why or Undo.
  - Limit 20 per address per hour, then one digest.
  - Each outcome goes to `ops::audit` without the address. `approval_request` gains `emailed`.
  - The template goes in `site/approval_mail.rs`, since `site.rs` is at its ratchet.
- **Files:** `crates/hub/src/notify/approvals.rs` (new), `crates/hub/src/notify/approvals_tests.rs` (new), `crates/hub/src/mcp/hub.rs`, `crates/hub/src/serve/mod.rs` (one line), `crates/hub/src/web/email.rs` (visibility only), `crates/hub/src/agenthub/mcp/{approvals,schema}.rs`, `crates/hub/src/site/approval_mail.rs` (new), `crates/hub/site/default/content/messages.json`.
- **Acceptance:** Hub tests with a stub SMTP server:
  - "email only when no device" with no device sends exactly one mail, with the title and the 256 link, no What text, and no injected header;
  - opening the link and deciding wakes `approval_wait`;
  - a notifier restarted mid-run neither duplicates nor misses (cursor test);
  - a refusing server is audited and retried;
  - the 21st mail in an hour is a digest;
  - with push off and email on, nothing is pushed.
  - Live receipt (needs SMTP on the test hub, see Operator actions): an approval arrives by email with push off.
- **Depends on:** 256, 258.

### 260: End the desktop binary collision (S)
- **Scope:** Plugins P2. Rename the Tauri inspector's `[[bin]]` to `devagotchi-desktop` (`apps/desktop/src-tauri/Cargo.toml:18-20`). Building it today overwrites `target/*/devagotchi`, the binary the hooks call.
- **Files:** `apps/desktop/src-tauri/{Cargo.toml,src/lib.rs}`, `apps/desktop/README.md` (also fix the stale :5).
- **Acceptance:** `cargo build -p devagotchi -p devagotchi-desktop` produces two distinct executables with no output-collision warning; `cargo check --locked -p devagotchi-desktop --all-targets` is green on windows-2025.
- **Depends on:** nothing.

### 261: Guided plugin install on the hub (G2) (M)
- **Scope:** Plugins P4. `/connect` and the `ai_client` workflow step show each client's plugin commands with this hub's `public_url` filled in: Claude Code `/plugin` plus the configure value, and `devagotchi connect codex --hub`. The VS Code and companion downloads are added when 263 and 268 land.
- **Files:** `crates/hub/src/web/connectors.rs`, `crates/connectors/src/lib.rs`, `crates/connectors/catalog/anthropic/claude-code.json`, `crates/hub/src/workflows/mod.rs` (at its 541 ratchet: net 0, or move into `workflows/ai_client.rs`).
- **Acceptance:** Web and workflow tests: the commands contain `public_url` and never `hub.example.com`. After deploy, `curl https://app.devagotchi.com/connect` contains "plugin install".
- **Depends on:** 211.

### 262: `devagotchi watch`: one local event feed (G4 core) (M)
- **Scope:** Plugins P5.
  - Signed in through `devagotchi login`: list boards from `whoami`; long-poll `board_wait` per board (25 s cap); follow `approval_wait`; poll `machines` for presence; read the local broker's `OperationsChanged`.
  - Output: one NDJSON line per event (`approval.requested`, `approval.decided`, `note.to_me`, `ticket.changed`, `machine.offline`, `op.finished{executable,args,exit_code}`).
  - One process fans out to every local surface.
  - **This lands in the `devagotchi/agenthub` repository first, then the submodule is bumped** (memory: AgentHub is its own repo).
- **Files:** `crates/agenthub/src/cli/commands/{mod,watch}.rs` (new `watch.rs`), `crates/agenthub/src/cli/remote.rs`, `crates/devagotchi/src/main.rs`, `crates/devagotchi/tests/board.rs`.
- **Acceptance:** `cargo nextest run -p devagotchi --test board` on a Postgres test hub:
  - `approval.requested` arrives within 2 s, and `approval.decided` after a cookie decide;
  - a finished broker operation emits `op.finished` with its exit code;
  - a stopped link emits `machine.offline` within the poll interval.
- **Depends on:** 211.

### 263: Desktop companion with a styled popup and an OS toast fallback (L)
- **Scope:** Plugins P6. `apps/desktop` becomes the companion.
  - Tray, single instance, autostart, `tauri-plugin-notification`.
  - It consumes 262.
  - A pre-created hidden popup window: no decorations, transparent, always on top, no focus steal, bottom-right.
  - Look: solid layered-shadow surfaces, system font, a 150 ms transform and opacity entrance, reduced motion honoured.
  - The approval popup offers "Open to decide" (opens the 256 URL) and "Later", and never decides.
  - Windows bundle first.
- **Files:** `apps/desktop/src-tauri/{Cargo.toml,tauri.conf.json,src/lib.rs,src/popup.rs,capabilities/popup.json}`, `apps/desktop/src/popup/` (new), `dist-workspace.toml`, `.github/workflows/ci.yml`.
- **Acceptance:**
  - CI builds the companion on windows-2025 and ubuntu-24.04 (webkitgtk) and runs a component test per popup kind.
  - Operator receipt on the Windows PC: an approval from Claude raises the popup within 2 s and "Open to decide" lands on the 256 page. Cold start and idle RSS are recorded.
- **Depends on:** 260, 262, 256.

### 264: Claude Code live surfaces: Notification hook and plugin monitor (M)
- **Scope:** Plugins P7. Add Notification (and Stop as the "finished" cue) to the hooks plugin. It forwards to the companion over local IPC, with an OS toast fallback. A plugin monitor runs `devagotchi watch --as auto`. `plugin_hooks()` and `hooks.json` change together, and the `--as auto --if-board` suffix assert (`plugins.rs:43`) is relaxed for the new command.
- **Files:** `crates/agenthub/src/cli/hooks/install.rs` (agenthub repository first), `plugins/devagotchi-agenthub/{hooks/hooks.json,.claude-plugin/plugin.json}`, `crates/hub/src/mcp/tests/plugins.rs`.
- **Acceptance:** `claude plugin validate plugins/devagotchi-agenthub` passes; the plugins tests pass. Receipt: a real Claude Code session on the PC raises the popup when Claude asks for permission.
- **Depends on:** 262, 263.

### 265: MCP Apps views on the hub (G3) (M)
- **Scope:** Plugins P3.
  - Declare the `io.modelcontextprotocol/ui` extension and resources in `get_info`.
  - Serve `ui://devagotchi/{approval,board,ticket}` as `text/html;profile=mcp-app`: self-contained, layered-shadow elevation, system fonts.
  - Set `_meta.ui.resourceUri` on `approval_request`, `board_prime` and `board_get`. Add `openai/outputTemplate` only if ChatGPT's current docs require it.
  - The approval view opens the 256 page through the host's open-link request; deciding stays in the browser session. Text answers stay complete.
- **Files:** `crates/hub/src/mcp/handler.rs`, `crates/hub/src/mcp/views/{mod.rs,approval.html,board.html,ticket.html}` (new), `crates/hub/src/agenthub/mcp/tools/{messages,read}.rs`, `crates/hub/src/mcp/tests/views.rs` (new).
- **Acceptance:**
  - MCP tests: `initialize` advertises the ui extension; `resources/list` has the three URIs; `resources/read` returns the mcp-app MIME type; `tools/list` shows `_meta.ui.resourceUri` on the three tools.
  - Receipt in `plugins.md`: claude.ai or Claude Desktop renders the approval card against app.devagotchi.com.
- **Depends on:** 208 (schemas), 256.

### 266: Deterministic cues and the test-run factoid (G13/G14) (M)
- **Scope:** Plugins P8. A Rust rule table with no model calls.
  - `op.finished` from a test runner gives a pass/fail cue with duration; the same failure repeated gives "same failure N times".
  - Factoids are local JSONL, sent to the hub only as a board note.
  - Hook-sourced cues run only when capture is `content`; with capture off, nothing runs.
- **Files:** `crates/kernel/src/cues/mod.rs` (new), `crates/kernel/src/capture/mod.rs` (at its 1017 ratchet: net 0), `crates/agenthub/src/cli/commands/watch.rs`, `apps/desktop/src/popup/`.
- **Acceptance:** A unit test per rule proves no model call; a recorded failing `cargo test` operation raises its factoid; with `devagotchi ai policy set --capture off` the same hook input stores nothing.
- **Depends on:** 262.

### 267: Codex parity: seat, hooks plugin, notify (M)
- **Scope:** Plugins P9. After the G5 seat work: add a `.codex-plugin` and hooks to `devagotchi-agenthub`, list it in the Codex marketplace, and wire Codex `notify` through `devagotchi connect codex`. Check every step against the Codex docs first.
- **Files:** `plugins/devagotchi-agenthub/.codex-plugin/plugin.json` (new), `.agents/plugins/marketplace.json`, `crates/mcp/src/connect.rs` (net 0; new logic in `crates/mcp/src/connect/codex.rs`), `crates/hub/src/mcp/tests/plugins.rs`, `.ai/dev/plans/plugins.md`.
- **Acceptance:** With a throwaway `CODEX_HOME`, `codex mcp list` shows the person's hub. Receipt: a signed-in Codex session calls `whoami` and the hooks fire.
- **Depends on:** 211, 264.

### 268: VS Code extension (M-L)
- **Scope:** Plugins P10. TypeScript under `plugins/vscode`.
  - Asks for the hub on first run (G15) and registers it through VS Code's MCP server definition provider.
  - A status-bar item (machines online, approvals waiting) and notifications fed by `devagotchi watch --json`.
  - A webview that reuses the 265 view bundle.
  - Check the API names against the VS Code docs first.
- **Files:** `plugins/vscode/{package.json,src/extension.ts,src/watch.ts}` (new), `crates/connectors/catalog/microsoft/vscode-copilot.json`, `.github/workflows/ci.yml`.
- **Acceptance:** `npx @vscode/vsce package` produces a `.vsix` in CI. A `@vscode/test-electron` test: the extension activates, the status item shows a fake feed's counts, and a fake approval raises "Open to decide". Plus an operator install receipt.
- **Depends on:** 262, 265.

### 269: Claude Desktop local package and the ChatGPT Desktop row (M)
- **Scope:** Plugins P11.
  - A `.mcpb` that wraps `devagotchi mcp` for the no-hub path (path 5.4).
  - ChatGPT Desktop: first establish from OpenAI's docs whether any local extension surface exists. Until then it gets the hub connector, the 265 views and the 263 companion cues.
  - Record both in the G11 table with sources and dates.
- **Files:** `plugins/claude-desktop/manifest.json` (new), `crates/connectors/catalog/{openai/chatgpt.json,anthropic/claude-desktop.json}`, `.ai/dev/plans/plugins.md`.
- **Acceptance:** `mcpb validate` and pack succeed; Claude Desktop on Windows installs it and runs a command with no hub. A ChatGPT connector receipt against app.devagotchi.com is recorded, and `chatgpt.json` is promoted beyond docs level only with a named test.
- **Depends on:** 265.

---

## Milestones

- **M0, the operator can use it from claude.ai on the PC:** 208, 209, 210, 211, then the connector refresh.
- **M1, the operator installs on the PC and the Android phone, and we fix what breaks:** M0 plus 225, 245, 246, 247, 248, 249, then 253 (receipt, round 1). 236 and 257 should ride along if ready, so the phone shows the build and gets proper pushes.
- **M2, full Desktop Commander:** 212-224.
- **M3, Shippin and management:** 226-235.
- **M4, the update path:** 236-243 (244 after).
- **M5, the high visual bar:** 250-255.
- **M6, notifications everywhere:** 256-269.

## Parallel worktrees

Each lane is its own worktree with its **own `CARGO_TARGET_DIR`** (memory: a shared target dir runs stale binaries). Within a lane, land increments in order. Across lanes, only the shared files listed below collide.

| Lane | Increments, in order | Files it owns |
|---|---|---|
| L1 machine surface | 208 → 212 → 213 → 214, 215, 216, 217, 218 (serial; all touch contracts) → 219 → 221 → 222 → 223 → 224 | `crates/contracts`, `crates/machine-engine`, `crates/mcp/src/adapter`, `crates/hub/src/machines/{files,search,system,history,calls}`, `crates/hub/src/mcp/{schema,callers,tools/operations}`, `tests/{hub,adapter}_probe.py` |
| L2 join | 209 → 210; then 220 when 214 and 256 are in | `crates/hub/src/machines/join/`, `crates/link/src/lib.rs`, `tests/compose_join.py`, image.yml join leg |
| L3 plugins and docs | 211 → 260 → 261 → (after 262) 264 → 267 → 268 → 269 | `plugins/`, `README.md`, `crates/mcp/src/connect*`, `crates/connectors`, `apps/desktop` (263 stays with L7) |
| L4 identity and admin | 225 → 226 → 227 → 228 → 229 → 230; 231, 232, 233, 234 in any order after 225; 235 after 217 | `web/providers.rs`, `identity/`, `web/hub_settings_{shippin,activity}.rs`, `web/account_security.rs`, `web/app/{ai_clients,machine_settings}.rs`, `web/one_user/` |
| L5 update | 236 → 237 → 238 → 239 → 240 → 241 → 242 → 243 → 244 | `deploy/compose/`, `deploy/linux/`, `crates/devagotchi/build.rs`, `ops/{release_check,update_status,health}.rs`, `serve/shutdown.rs`, `web/hub_settings_hub.rs`, `web/hub_update.rs`, `web/app/machines_update.rs`, `crates/link/src/update.rs` |
| L6 phone | 245 → 246 (merge fast: it splits `site.rs` and `pages.rs`) → 247, 248, 249 → 250 → 251 → 252 → 253; 254 after 213 and 242; 255 after 248 | `web/hub.css`, `web/pages/head.rs`, `site/pwa.rs`, `site/style.rs`, `tokens.json`, `web/assets/{app.js,sw.js}`, `web/sign_in*.rs`, `web/app/{job,events}.rs`, `tests/android_browser.py`, `tests/android_device.py` |
| L7 notifications | 256 → 258 → 259; 257 after 246 and 248; 262 (agenthub repository) → 263 → 265 → 266 | `web/approval_page.rs`, `notify/approvals.rs`, `notify/push/subscriptions.rs`, `identity/preferences/`, `web/app/notifications.rs`, `crates/agenthub/src/cli/`, `crates/kernel/src/cues/`, `mcp/views/` |

**Can start on day one with no file conflict:** 208 (L1), 209 (L2), 211 (L3), 225 (L4), 236 (L5), 245 (L6), 256 and 260 (L7 and L3; 260 is in `apps/desktop`, untouched by 211).

**Shared files to sequence or rebase.** All are small additive edits:
- `web/app/home.rs`: 241 and 242 (one call each) against 249 and 250 (L6 owns the file). Land 249 first.
- `web/app/machine.rs`: 235 (one call), 242, 254. Order 242 → 235 → 254.
- `web/app/me.rs`: 225, 232 (one link), 258 (one link). 225 first.
- `web/hub_settings.rs` (488 lines): 225 removes People, 236 adds one chip. 225 first frees the lines.
- `config/live/catalog.rs`: 230, 234, 238, 240. Each adds catalog entries; rebase.
- `site.rs` and `pages.rs`: only 246 moves code out. Everyone else edits the new files.
- `routes.rs` (680): 210, 225, 256 each change one line, net 0. Expect a rebase conflict only.
- `web/assets/sw.js`: 248, then 257.
- `mcp/tools/machines.rs`: 210 → 213 → 220.
- `crates/hub/site/default/content/messages.json` and `.github/workflows/ci.yml`: everyone appends. Rebase, do not reorder keys.
- `crates/devagotchi/tests/layout/oversize.txt`: numbers only go down (212, 246). Run `DEVAGOTCHI_WRITE_LAYOUT=1 cargo test -p devagotchi --test layout` after moves.

## Defaults I chose

1. **Postgres stays for the one-user edition.** editions.md: ADR 0005, and "the standalone release stays on Postgres" until LiftOffDB proves itself. The one-user install is `deploy/compose/install.sh`, which brings its own Postgres container (`compose.yaml:45`). Nothing here adds SQLite, and 211 removes the README's SQLite claim.
2. **The hub runs on Linux** (a VM, WSL or Docker host), x86_64 or arm64. Machines run Linux or Windows. There is no Windows hub installer in this program.
3. **A Docker machine uses the hub's own image** (209). No new machine image, and the same provenance as the hub. It updates by image pull, not in place.
4. **Call handles (214) and search sessions (215) are in memory.** No new tables, no deeper Postgres use.
5. **Update mode:** new `install.sh` installs default to `ask`, which is the operator's "UI shows a new release, one click to deploy". Hosts with no mode file keep today's `auto`, and app.devagotchi.com stays on `auto` following `:main`. The hub never gets the Docker socket; the host's `.path` unit does the deploy.
6. **`/healthz` shows build, revision and channel publicly.** `downloads.version` already exposes the build.
7. **No approving from the notification shade, the popup or VS Code.** Deciding stays a same-origin POST from the signed-in page (`web/approvals.rs:411-424`). Every surface offers Review or Open to decide, which lands on the 256 page. That matches the operator's "email that sends you to the browser to approve".
8. **Approval email links are signed-in links (256), not token links.** dormancy-and-reauth.md:62 and :86-93.
9. **Machine rename is excluded,** because the name is the routing and enroll key.
10. **No chat screen inside the hub in this program.** Chat happens in Claude, ChatGPT and Codex. The phone gets run, output and cancel (254).
11. **The multi-tenant-only forgot-password fix** (admin map increment 14) is out of scope for the one-user edition.

## Decisions only the operator can make

1. **Default update channel for self-hosters.**
   - The conflict: approved S1 (`self-hosted-and-provenance.md`, 6381d4e, `:latest`) against texas-baseline M2 (`texas-baseline.md:27-29`, `:main`). `:latest` is still v0.1.0 from 2026-11-14.
   - Proposed: keep S1 (release channel). Once M1 is green, you cut **v0.1.1**. Never re-cut v0.1.0: a re-cut keeps the build name `0.1.0`, so machines answer `already_on` (`machines/link/updates.rs:69`).
   - Agents never tag. Mark texas-baseline M2 superseded.
2. **The `write_pdf` renderer (223, P10 has waited on you).**
   - Proposed: a permissively licensed PDF writer using the PDF base-14 fonts, so nothing is embedded and there is no OFL issue with `deny.toml`, plus a markdown subset. Desktop Commander's arbitrary HTML/CSS stays partial.
   - The alternative is Typst: much richer, but it needs bundled fonts, so an OFL licence exception in `deny.toml` and a larger binary.
3. **How a self-hosted hub gets a Shippin client before S0** (Client ID Metadata Documents is a Nomen-side change and has not landed).
   - Proposed: after 228, the owner runs `connect-nomen` (or 230's browser button) once, which registers one client per hub in your Nomen project. You decide whether that Nomen project is open to any self-hoster or invite-only.
4. **Where the durable approval notifier lives (259).**
   - Proposed: the hub's existing `signal_cursors` stream (no new store, no submodule change). That contradicts F18's "outbox in crates/agenthub" (`agenthub-factory.md:220`), whose only stated reason is the `approvals.rs` ratchet. Confirm, and I will record F18 as split into 256 and 259.

## Operator actions, not decisions

- After 208 deploys: disconnect and reconnect the Devagotchi connector in claude.ai. Nothing reaches claude.ai's cached tool list until you do.
- For 259's live receipt and for "Forgot password" on app.devagotchi.com: SMTP credentials on `/admin` → Email (`/healthz` shows `mail.configured:false`).
- For 253 and 257: install from app.devagotchi.com on your Android phone with USB debugging on, so `tests/android_device.py` can attach.
- For 230: a real Shippin issuer and client (see decision 3).
- For 224, 263 and 264: receipts from your Windows PC.
