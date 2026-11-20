# The rewrite: every major area on its golden pattern

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Request: the operator, 2026-11-19: "Get this actually planned out and fully implemented
  with clean patterns either already established or properly now getting implemented, and
  files and folders properly planned out and consistent."
- Objective: bring the code in every major area onto its pattern in
  [.ai/patterns](../../patterns/README.md), in seven batches, each one PR merged on green.
- Non-goals: new features; the cloud edition (`devagotchi-cloud` waits until the self-hosted
  edition is confirmed on Texas and the phone); renames the structure plan owns.
- Canonical architecture: [the patterns](../../patterns/README.md) (shared rules G1-G14);
  [the product](../../../docs/product.md); [the structure plan](structure-cleanup.md).

## Status

| Batch | What | State | PR |
|---|---|---|---|
| B0 | The patterns, and CI that cannot pass without running | merged | #259 (`a61cfb7`) |
| B1 | Desktop Commander capabilities on both surfaces ([plan](desktop-commander-b1.md)) | merged | #260 (`e7aeb51`) |
| B2 | The machine link and join ([plan](link-and-join-b2.md)) | merged | #261 (`86b9d8b`) |
| B2a | Main builds again: no build of main existed since 2b3feec ([plan](main-builds.md)) | merged | #262 (`3e2e5b9`) |
| B3 | AgentHub board operations (an agenthub PR, then a submodule bump and a hub PR) ([plan](board-operations-b3.md)) | merged | #263 (`ec69ed6`); agenthub `3866d18`, #8 (`f4af7f9`) |
| B4 | The rest of the hub MCP surface ([plan](hub-mcp-surface-b4.md)) | merged | #264 (`726af4e`) |
| B5 | Hub pages, identity and configuration (ordered by [the standalone build](standalone-build.md), W2, after the W1 moves) | pending | not_created |
| B6 | Persistence and the shape of the test suite | waits on B4, B5 | not_created |

Every batch runs the alpha way: if it fails, leave it failed and work it through. Every batch
ends with `cargo build --locked --bins`, the workspace tests with the test Postgres required,
and `python tests/e2e.py --bin-dir target/debug` on a fresh database, before it is pushed.
The file references below were measured at main 2b3feec, before B0; each batch re-reads its
files before it changes them.

## 0. Contradictions between the patterns, and what holds

| # | Contradiction | Resolution (now in the README's G-rules and amended into each pattern in B0) |
|---|---|---|
| C1 | **Where a shared description lives.** machine-capability R8 says `contracts::describe` only. mcp-tools D2 also allows `devagotchi_mcp::{INPUT,RESIZE}_DESCRIPTION`. | `describe.rs` only. `INPUT_DESCRIPTION` and `RESIZE_DESCRIPTION` move to `describe::{OPERATION_INPUT, OPERATION_RESIZE}`, and the re-export at `crates/mcp/src/lib.rs:92` goes. Amend D2. |
| C2 | **Malformed arguments.** mcp-tools E1 says "ErrorData ... malformed arguments (rmcp)". machine-capability R12 says MCP 2025-11-25 makes input validation a tool execution error. | 2025-11-25 holds (G4): for a known tool, a serde refusal becomes `CallToolResult` isError with `invalid_arguments`. An unknown tool stays -32602 (`tests/mcp_protocol.py:265-268`). Amend E1. Patterns cite 2025-11-25 for errors; annotation semantics are the same in both revisions. |
| C3 | **set_config_value's `destructive_hint`.** machine-capability's canonical snippet says `false`; mcp-tools A2 says `true`. | `true` on both surfaces: lowering `max_operations` retires retained jobs (`machine-engine/src/broker/history.rs:47-50`). Fix the snippet. |
| C4 | **Hub defaults.** machine-capability R15 says a hub field has the contract's type with `#[serde(default = "devagotchi_contracts::default_*")]`. mcp-tools I3 says keep `Option` with `unwrap_or(DEFAULT_*)`. | R15's form, because the parity test compares inputSchemas and `Option` widens the hub's. Both are honoured: `pub const DEFAULT_X` plus `pub fn default_x() -> T { DEFAULT_X }` (G3). Amend I3. |
| C5 | **Where hub operation and machine-management tools live.** machine-capability says move all 8 `operation_*` tools to `machines/operations.rs`. mcp-tools says move 3 into `mcp/tools/operations.rs` and shrink `machines/mod.rs` *Serves*. link.md cites a non-existent `mcp/machines.rs`. | Layout rule 7: the domain serves its tools. All `operation_*` go to `machines/operations.rs` (`operations_router`, merged in `machine_router()`). `machines`, `machine_add` and `machine_update` go to `machines/manage.rs`. `mcp/` keeps the `Hub` surface, `call`, `caller`, `send_hinted`, `outdated`, `whoami` and the shared schema. *Serves* in `machines/mod.rs` then becomes true by moving the code, not by deleting lines. Fix link.md's path. |
| C6 | **Origin check.** hub-pages rule 2 makes `web::from_this_hub` lenient (neither header is allowed as non-browser) and keeps a separate strict `from_own_page`. identity K5 rejects when neither header is present (OWASP). | One strict function, `web::from_this_hub`: Sec-Fetch-Site must be `same-origin` when present; otherwise Origin must equal the hub's; with neither, reject. It runs as one layer with an `EXEMPT` list of the 5 cookie-less routes. `HubSignIn::from_own_page` calls it. Non-browser callers use bearer routes, which are exempt. Test forms already send Origin (`hub_settings_tests.rs:21-23`, `tenants.rs` `form()`). |
| C7 | **Referrer-Policy.** identity K5 says "pages that post set same-origin". hub-pages rule 5 says it is the default in `head.rs::render`. | Default in `render()`; delete the 13 per-page inserts. Remove the per-page sentence from K5. |
| C8 | **Origin before or after owner.** identity's record recipe checks owner first (404), then origin. hub-pages enforcement 1 puts the origin check in a layer first. | The layer runs first. A cross-site POST to an owner route gets 403; GET still hides the route with 404. Recipes renumber: origin (layer), owner, file-set 409, validate ... |
| C9 | **Audit failure after the effect.** mcp-tools AU2 says a failed audit write fails the call (`ops/audit.rs:268-271`, `agenthub/mcp/calls.rs:174-186`). board-operations rule 12 says report the committed result and surface the audit failure. | G7: refuse only before the effect; after it, return the true result plus a log and an `audit.write_failed` operator signal. Keep one line per call: no "started" pre-row, which would break AU1 and the outcome vocabulary. **Needs the operator's agreement** (AGENTS.md: material behaviour change to authority/audit). Default if no answer: G7 as written. |
| C10 | **Inline test modules.** persistence and machine-capability treat every inline `mod tests {}` as a layout-3 violation. link.md says rule 3 does not forbid them. identity's canonical section (`config/analytics.rs:68-111`) is inline. | G10: inline is allowed in a leaf file under the cap; a front door or an over-cap file moves its tests out. **Withdrawn:** `broker/settings.rs:284-341`, `files/write.rs:244`, `files/given.rs:63`, `mcp/src/files.rs:83`, `outputs.rs:58`, `adapter/history.rs:168`, `ops/audit.rs:356-452`, `machines/join/compose.rs`, `machines/update.rs` (each unless it is over the cap when touched). **Kept:** `files/search.rs` (463 lines, growing), `broker/system.rs` (becomes `system/mod.rs`), `machines/join/mod.rs` (front door, 482 lines), `agenthub/src/cli/hooks/mod.rs` (front door), `crates/link/src/lib.rs` (layout rule 4), `web/sessions.rs` (moves with S8). |
| C11 | **Contracts layout.** link.md flat-re-exports `link::*` and notes this breaks layout rule 5. machine-capability keeps family modules in `lib.rs`. | G10's named exception for `crates/contracts`. Amend layout.md rule 5. One AGENTS.md:39 sentence (B0). |
| C12 | **Clock.** persistence rule 15 takes `now` before the block; board-operations rule 3 takes the DB clock inside it. | G9: one clock per decision. AgentHub uses the DB clock inside (re-evaluated on retry, which is correct). Hub stores take `now` once before. Never mix them in a table. Board's standing-approval and undo violations are exactly such a mix. |
| C13 | **Skip guarantee.** persistence uses a `layout.rs` scan that each Postgres binary is named in ci.yml. tests-and-ci uses `rdeps(devagotchi-store)` plus a REQUIRE flag. Skip text differs (`skipped:` vs `SKIPPED:`). | tests-and-ci's mechanism; drop the ci.yml scan (rdeps covers every binary). One text: `SKIPPED: no DEVAGOTCHI_TEST_POSTGRES_URL`. The skip-*shape* scan lives once, in `test_shape.rs`. |
| C14 | **Overlapping tool checks.** mcp-tools `tools.rs`, board `contract.rs` (title, hints, field docs) and machine-capability `parity.rs` overlap. | `check_tools` (one function in `crates/mcp/src/pattern.rs`) is used by both surfaces. board `contract.rs` keeps only board-specific checks (`READ_ONLY_TOOLS` against hints, `PER_CALLER_WRITES`, `next_for` coverage). Its field-doc test is dropped as a duplicate. A single parity test lives in `machines/tests/parity.rs`; mcp-tools step 3 folds into it. |
| C15 | **When web and identity tests move.** identity says move `web/*_tests.rs` to `web/tests/` before S8. hub-pages says move once, with S8. | Move once, with S8 (hub-pages). No interim move. |
| C16 | **mcp_protocol.py in CI.** machine-capability runs it from `tests/e2e.py` on every OS. tests-and-ci adds a ubuntu ci.yml step. | From `tests/e2e.py` (tests-and-ci rule 11: one scenario runner), so it runs on Linux and Windows. |
| C17 | **Compatibility under alpha.** `.ai/README.md` says no compatibility layers. link rule 2 (append-only golden, never bump) and persistence rule 5 (add-only migrations) keep old peers working. | Not a production layer. Enrolled machines and the hash chain cannot be fixed from the hub, so the wire and chain stay additive. Everything else (tool schemas, outbox kind names, MCP inputs) changes freely in alpha, noted in the parity doc and the changelog. |

Operator decisions this plan needs (each batch proceeds on the default):

| # | Decision | Default |
|---|---|---|
| D1 | C9 audit semantics | **(a), chosen by the operator 2026-11-19:** return the real result, log the audit failure, raise an operator alert. Built in B1 (`Audit::record_after`, `Hub::recorded`, `/healthz` `operator_alerts`). |
| D2 | `FETCH_TIME` | derived under `IO_TIMEOUT`; `held_for` not used. **Done in B1.** |
| D3 | `/enroll` unsigned route | (b) remove it; tests call `enroll_owned`. **Done in B2.** |
| D4 | Provider overlay of file entries (identity C5) | (a) record the exception for the four guided names |
| D5 | Auto-link by email (identity P5) | proof required for every provider |
| D6 | The S8 ruling (domain `web/`) | waits; the S8 moves listed below wait with it |
| D7 | Board action renames | do them (alpha) and note them in release notes |

## Batches, in order

### B0: The patterns, and CI that cannot pass without running (1 day)

Docs:
- Write `.ai/patterns/{machine-capability,mcp-tools,link,board-operations,persistence,identity-and-config,hub-pages,tests-and-ci}.md` from the area patterns, with C1-C17 applied, and the README above.
- `.ai/README.md`: one row per pattern.
- `layout.md`:
  - rule 3 clarification (G10);
  - rule 5 contracts exception;
  - `:39-40` prefix test path → `crates/hub/src/store/tests.rs`, plus AgentHub's `migrations/tests.rs`.
- `AGENTS.md:39`: "Contracts belong in a family module of `crates/contracts/src/`, re-exported from lib.rs; a new machine capability follows `.ai/patterns/machine-capability.md`. Regenerate artifacts, never edit them."
- Fix the dead `crates/store/migrations/postgres` paths in `crates/hub/README.md:503`, `tests/throwaway_postgres.sh:5` and `docs/security-baseline.md:50`.

Honest-CI guard (tests-and-ci violations 1, 2, 3, 11, 12, 29, 33, 35):
- `crates/store/src/testing.rs`:
  - `REQUIRE_ENV`; `empty_postgres` asserts it;
  - `or_skip!` prints `SKIPPED:`;
  - `server_url()`; `clickhouse()` with `DEVAGOTCHI_TEST_REQUIRE_CLICKHOUSE`.
- Move `store/src/tests.rs:44,78` and `ops/warehouse/tests.rs:254-259` onto them.
- `ci.yml` hub-postgres: one step, `cargo nextest run --locked --profile ci -E 'rdeps(devagotchi-store)'`, with both REQUIRE flags. This brings in `hub_recovery.rs`. Rewrite the `:70-73` comment to name the mechanism.
- `.config/nextest.toml`: `[profile.ci.junit] store-success-output = true`; upload junit `if: always()` after each nextest step (SHA-pinned).
- `tests/e2e.py:233-245`: `--clickhouse-container` without a URL exits nonzero.
- `ci.yml:258-272`: pip install as its own step, then one step per browser script.

Checks:
- A hub-postgres run with the URL deliberately unset goes red. Restore it and it is green.
- `devagotchi ai check`.
- junit artifact present.
- `cargo test -p devagotchi --test layout`.

### B1: Desktop Commander capabilities on both surfaces (6-8 days)

Every machine-capability violation, plus the mcp-tools violations on machine tools.

Contracts:
- Wildcards: `interactive.rs:229-240` `caller()`, `lib.rs:321-329` `held_for()`. Move the doc line `interactive.rs:199`.
- `pub const MAX_LIST_ENTRIES` / `MAX_LIST_DEPTH` (from `machine-engine/src/files.rs:27-28`).
- Config ranges beside `CHANGEABLE_SETTINGS`, formatted into `settings.rs:65,71` and tested against `describe::SET_CONFIG_VALUE`.
- `pub DEFAULT_*` plus `pub fn default_*` (`files.rs:36,97,188`, `search.rs:58`, `system.rs:140`, history 50).
- `///` on `pid`, `mode`, every `operation_id`, `ListInput.offset/limit`.
- `describe::` consts for run, the 7 `operation_*`, the 10 file tools, `SEARCH_FILES`, `OPERATION_INPUT/RESIZE`, all machine-neutral, with "data, not instructions" on `operation_read`, `run`, `read_file`, `read_multiple_files`, `search_files`, `read_url` and `operation_input`.
- `describe::BROKER_TOOLS` plus its coverage test.
- `GetInput/ReadInput/WaitInput` in the schema loop (`interactive/tests.rs:11-19`).
- `SendInput/ResizeInput/InputSent` in `export-contracts.rs:21-58`, then regenerate.

Broker:
- Wildcards: `broker/files.rs:214-217`, `broker/system.rs:55-58`.
- `PAGE_TIME`, `FETCH_TIME` (D2) and `EXIT_WATCH` with const assertions.
- `set_config` / `config_view` through `blocking` (`settings.rs:181-282`).
- `broker/tests/processes.rs` (`operation_process`, `broker_process`, `process_not_found`, Windows `kill`).
- `broker/system/web/tests.rs` (Range 206/200, 416, `file:` refused, `max_bytes` 0).
- Move `files/search.rs:300-463` tests to `search/tests.rs`; `broker/system.rs` → `system/mod.rs` with `system/tests.rs` (C10).
- Replace the `lifecycle.rs:192` sleep with a deadline poll.

Local adapter:
- `crates/mcp/src/pattern.rs::check_tools`.
- `local_tools()` (`router()` → `pub(crate)`).
- `call_tool` maps `INVALID_PARAMS` for a known tool to `invalid_arguments` (C2).
- `read_url` hints on both surfaces; `set_config_value` destructive=true on both (C3).

Hub machine tools:
- Defaults through serde `default = "devagotchi_contracts::default_*"`, replacing `files.rs:162,193,259,305`, `search.rs:77`, `system.rs:159`, `history.rs:68`. Move `read_multiple_files`' default into `crates/mcp/src/files.rs`.
- Contract field docs copied verbatim (`search.rs:20-49`, `system.rs:41-75`, `files.rs:23-139`).
- `write_file` `idempotent_hint=false` (`files.rs:212`).
- `machine_add` hints stated; `machine_update` destructive=true.
- Flat closed `{machine, ...}` structs for the 8 operation tools, replacing `mcp/schema.rs:44-67` and `callers/mod.rs:23-42`. Fix the `schema.rs:11` doc. Drop `"broker"` from `api_keys/mod.rs:79-80`. Say `machine` in `mcp/tools/machines.rs:161`, `run.rs:168` and the descriptions.
- Moves under C5: `machines/operations.rs`, `machines/manage.rs`.
- `MachineAdd` / `MachineUpdate` `deny_unknown_fields`.
- `jobs_saw` names every kind (`machines/link/calls.rs:140-146`), with a test in `web/app/tests.rs:400-420`.
- `Hub::recorded` beside `Hub::record` (`ops/audit.rs`), with G7 failure semantics (D1) in `Hub::call`.
- Audits:
  - `machine_add` and `machine_update`;
  - `whoami` and `machines`;
  - `run` bounds (`run.rs:86-100`);
  - `run` outdated code (`run.rs:133-136`).
- `machines/mod.rs` *Serves* rewritten.

Tests and probes:
- `machines/tests/parity.rs`.
- `mcp/tests/tools.rs` over all hub tools, with a shrinking `NOT_YET` list of (tool, rule) for board, workflow, offers, connections and kernel tools.
- `mcp/tests/scan.rs` (N1 uniqueness, AU1) with the same shrinking list.
- `mcp_protocol.py`: 8 entries plus `cursor`, run from `tests/e2e.py`.
- `adapter_probe.py`:
  - `search_files`, `get_config`, `set_config_value`, `list_processes`, `kill_process`, `read_url`, `operation_cancel`, `operation_resize`;
  - `operation_wait` instead of polling;
  - `missing` built from `TOOL_SCHEMAS`.
- `hub_probe.py`: flat `machine` (`:209,217,219,237,252,277,284`) and `read_image`.
- CI `static` job seeded with `node scripts/generate-contracts.cjs --check` and `devagotchi ai check`.
- Update `.ai/docs/desktop-commander-parity.md` (schema change, C2).

Checks:
- `cargo nextest -p devagotchi-contracts -p devagotchi-machine-engine -p devagotchi-mcp -p devagotchi-hub`, including parity, tools and scan.
- generate-contracts `--check`.
- e2e on Linux and windows-2025 (`adapter_probe`, `hub_probe`, `mcp_protocol`).
- Before/after `tools/list` diff reviewed.

### B2: The machine link and join (4-5 days)

Every link violation. This is Desktop Commander's transport.

Contracts:
- Move `lib.rs:402-494` and `link_frame_tests.rs` to `contracts/src/link/{mod,tests}.rs` with `pub use link::*` (C11).
- `LinkFrame::Hello.build` with `skip_serializing_if` (`:456-457`).
- `LINK_HEARTBEAT` / `LINK_IDLE_TIMEOUT` plus an assert.
- `MACHINE_KIND_CONTAINER`, `REFUSAL_UNAVAILABLE`, `UPDATE_*` (replacing 15 literals), `valid_build_name` (replacing `link/update.rs:240-251`, `hub/machines/downloads.rs:72-82` and `compose.rs:32-36`).
- Append-only golden `SEEN`, pinned signing bytes and version.

Link:
- Name the variants at `session.rs:98,126,185`.
- Idle deadline with Ping, and a send timeout (`:156-211,276-284`).
- `session.rs` → `session/mod.rs`; `update/` folder with `tests.rs`; `lib.rs:113-249` → `run/`.
- `session/tests.rs`: refusal classification, `hub_mismatch`, an update-failed frame, and a silent hub giving Retry (current_thread with paused time).

Hub:
- `Session::fake`, replacing the 5 hand-built sessions.
- Drop `links!` (`link/tests.rs:16-24`) and its tempdirs.
- Split `link/tests.rs` into `tests/{mod,presence,admit,wire,calls,container}.rs`.
- Gate tests (`update_unsupported`, `AlreadyOn`, `no_build`).
- Refusal table (`protocol_version`, `audit_write`, `early_access`, version+1).
- Socket test on `hosted_with` with tokio-tungstenite as a dev-dependency.
- `link/tests.rs:91` absence window derived from `offline_after`.
- `/enroll` (D3), with `discovery.rs` `PRIVATE` updated.
- Join tests move to `join/tests.rs`.
- Fix the doc paths at `link/mod.rs:52` and `machines/update.rs:5`.

Join and installers:
- `join.ps1:33-34` `DEVAGOTCHI_ROOT`, else `LOCALAPPDATA`.
- `install-devagotchi.ps1`: `:7,:42` default `Root`; `:35,:348` the environment variable; `-EnrollCode` deprecated.
- `crates/link/README.md:33-41`.

Checks:
- `crates/devagotchi/tests/link_protocol.rs`.
- Contracts golden.
- Hub socket test (hub-postgres).
- e2e on Linux and Windows 2022/2025.
- `deploy/*/ci-install-check.*`, `tests/compose_join.py`, `tests/machine_update_probe.py`.
- An older-link-hello golden line still parses.

### B3: AgentHub board operations (7-9 days; an agenthub PR, then a submodule bump and a hub PR, landed together)

Every board-operations violation, plus mcp-tools on board tools, plus tests-and-ci and persistence on AgentHub tests.

Store (`crates/agenthub`):
- `hosted.rs` → `hosted/mod.rs`; split `hosted.rs` and `operations.rs` into `leases`, `notes`, `graph`, `settings`, `reads`, `tickets`, `inbox`, `issues`; update `oversize.txt`.
- `migrations.rs` → `migrations/mod.rs`.
- New single-transaction operations:
  - `ask`, with a shared `decide_in` (replacing `approval.rs:229-285`);
  - decide plus standing (`web/approvals.rs:440-466`);
  - `delegate` (`ticket_actions.rs:172-184`);
  - create's default repo (`mcp/tools/tickets.rs:77-88`);
  - `set_wake` (`wake.rs:257-287`);
  - `undo_close` on the DB clock (`ticket_actions.rs:206-213`).
- `check_wip` shared (`hosted.rs:544-563`, `operations.rs:53-65`).
- `edit_settings` takes the scope's `now` (`approval.rs:254,343,384,410,419`, `board/mod.rs:86-90`, `me.rs:390`); drop `Board::now`.
- handoff `{"assignee": to}` (`operations.rs:623-631`).
- No-ops write nothing: `add_dep` (`:415-429`), `set_project` (`:186-207`), `put_settings` (`:124-150`).
- `chain::actions` consts, with the 5 renames (D7).
- `decode` in `inbox.rs:24-26,128-129`; `invalid()` at `operations.rs:593-596`.
- `Board::issue_context` / `write_issue_context(&Actor)` in `board/issues.rs`, with `issue_source` moved there.
- The registry SQL from hub `registry.rs:66-99,136-140` → `hosted::registry`.
- Hooks: `//!` front door; tests → `cli/hooks/tests.rs`.
- CLI `next_for` / `exit_code_for` (`cli/errors.rs:18-80`); `parent_closed` is a conflict.
- Replace the `stores()` loops (`tests/mod.rs:115-118`, `hosted.rs:17,294`, `hosted_lifecycle.rs:15,152`) and `lanes.rs:7` with `or_skip!(fixture())`.

Hub board surface:
- `deny_unknown_fields` on the 22 input structs (`agenthub/mcp/schema.rs`), after checking `cli/remote.rs:48-310` and grepping `tests/*.py`.
- 46 field docs; `IssueLookup` and `ContextWrite` docs.
- T1 titles on `board_attach`, `board_issue_context` and `board_issue_checkpoint`.
- `board_claim` `idempotent=false`; `board_settings` `destructive=true`; `PER_CALLER_WRITES` with the `READ_ONLY_TOOLS` doc fixed.
- Issue tools → `mcp/tools/issues.rs`.
- `next_for` lines (`refusals.rs:18-45`).
- Audit fixes (G7):
  - seat inside `board_call_scoped` (`calls.rs:133`);
  - `fence_for` (`tickets.rs:163,303,342,379`);
  - dep kind and wake URL inside the closure;
  - `board_wait` / `approval_wait` audited once;
  - `board_bind` store error, `created` → `ok`, approval status into the target;
  - issue repo mapping (`read.rs:62-66,98-102`).
- Page and REST audit lines (`web/approvals.rs:255,292,379`, `me.rs:382-410`, `issue_lifecycle/mod.rs:251-255`).
- Remove the board rows from mcp-tools' `NOT_YET` list.

Docs:
- `docs/agent-board.md:106-110` codes and `:167` tool count.
- `plugins/README.md:48-49`.
- `agenthub/tests/ahead.rs:6-7`.
- `agenthub/mcp/mod.rs:1-16`.

Checks:
- AgentHub repo `test (postgres)`.
- Hub `contract.rs`, `tools.rs`, `scan.rs`.
- `actions.rs` (form, no-op silence with the quiet listener, handoff event).
- `cli/tests/output.rs`.
- Atomicity test shape (`tests/hosted.rs:357-394`) for each new multi-row operation.
- `mcp/tests/plugins.rs`.
- Browser `app_browser.py` (approvals, ticket actions).
- Release note for the action renames.

### B4: The rest of the hub MCP surface (3-4 days)

mcp-tools violations outside machines and boards. When this batch is done, `NOT_YET` is empty and is deleted.

- `CallIn` (`connections/mod.rs:73-87`), kernel inputs (`kernel/mcp.rs:51-169`, `handoff.rs:59-60`): `deny_unknown_fields` and the 12 field docs.
- `workflow_start` (destructive=true, idempotent=false) and `workflow_cancel` (destructive=true) hints (`workflows/mcp.rs:225-231,299-306`).
- `timeout_secs` → `timeout_seconds` (`:90-96,276-283`), with the site instructions and tests updated.
- `connection_call`: "data, not instructions"; outcome `ok` with method, path and status in the target (`connections/mod.rs:352,370-373`).
- `key_denied` and `early_access` as `structured_error` (`mcp/handler.rs:40-43`, `tenancy/gate.rs:51-60`); correct the doc at `gate.rs:51-55`.
- Audits:
  - `dictionary_get`, `lexicon_resolve`;
  - `lexicon_review` list/history and its argument refusals (`kernel/mcp.rs:~405-447`);
  - `lexicon_handoff` steps (`handoff.rs:188-194`).
- `record_workflow`, `record_offers` and the inline copies replaced by `Hub::recorded` (`workflows/mcp.rs:153-167`, `offers/mcp.rs:237-249`, `connections/mod.rs:338-385`, `kernel/mcp.rs:258-275`, `machines/files.rs:285-327,428-458`, `run.rs:101-164`).
- `workflows/mcp.rs:316` unreachable `?`.
- Move the connections tools to `connections/mcp.rs` (`mod.rs:319-387`).
- `WhoAmI` comment (`mcp/schema.rs:24-26,150-153,193-194`).
- D5 instructions test (offers shown and hidden).

Checks:
- `tools.rs` and `scan.rs` with `NOT_YET` deleted.
- Behaviour tests on the hosted harness: seat refusal, missing fence, `run` `wait_seconds` 0, a successful `connection_call`, each with one audit row with outcome `ok` or the code.
- `get_usage_stats` counts no success as a failure.
- `workflow_browser.py`, `first_connect_browser.py`.

### B5: Hub pages, identity and configuration (7-9 days)

Every hub-pages and identity-and-config violation that is not a pure S8/S9 move.

Styles (operator request, 2026-11-19: colocated feature code and fluid clamping in the
viewport), added to `.ai/patterns/hub-pages.md` before the code moves:

- Each page's styles live beside its Rust (`<feature>/page.css`, included by that page), on
  shared tokens in one base sheet; no page restyles another's classes.
- Type, spacing and widths are fluid between a phone and a desktop with `clamp()` over the
  viewport (`clamp(min, preferred vw + rem, max)`), from one token scale, never per-element
  breakpoints; container queries where a component, not the page, decides its layout.
- `@layer reset, tokens, base, components, pages` fixes the cascade order, so a page's rule
  wins by layer, not by specificity.
- Increment 250 (the phone-tailored pages, `one-user-edition.md`) lands on this.

Shell and guards:
- `routes()` per module (`routes.rs:42-136`); drop `hub_authorization` (`:29-34,110-112`, `serve/mod.rs:447`); `hub_settings::routes()` (`hub_settings_live.rs:27-46`).
- The one strict `from_this_hub` (C6), run as a layer with `EXEMPT`; delete `approvals::same_origin` (`:85-95`) and its 32 imports; `passkeys.rs:304-310` calls it.
- `app::change` (`connections.rs:155-245`, `machine.rs:157-177`, `ticket_actions.rs:52-69`, `me.rs:310-322`, `keys.rs:199-209`).
- `owner_change` (`hub_settings*.rs`, 6 copies).
- Every `Who` arm matched:
  - `approvals.rs:102-424`, `machines.rs:30-50`, `orgs.rs:63-73`, `routes.rs:425-486`;
  - `hub_settings_owner.rs:45-50`, `operator.rs:47-55` (503);
  - `app/mod.rs:132-164`;
  - `orgs.rs:758,998`, `mod.rs:127-136`, `providers.rs:688`, `sign_in.rs:222`.
- `/account` reads (`routes.rs:431-443`) → 503.
- `head.rs:143-146` defaults to `same-origin`; delete the 13 inserts and `orgs.rs:1023-1026`.
- `encode` for path segments (`connections.rs:98`, `keys.rs:75-77`, `me.rs:178`, `hub_settings_sinks.rs:154`, `sign_in_cards.rs:260`).
- Status codes (`machines.rs:97-104`, `keys.rs:273-296`).
- `framed` covers every `Kind::App` page (`mod.rs:180-182`).
- `pages.rs` → `pages/mod.rs`.

Copy:
- `approvals`, `keys` and `connections` sections in `messages.json` with `deny_unknown_fields` structs.
- `FocusCopy`, `MeCopy`, `OrgsCopy` and `SignInMessages` defaults written into `messages.json`; `#[serde(default)]` and the Default impls dropped (`model.rs:892-1367`).
- Copy ratchet started for the shell, account, admin and sign-in words. They move section by section; the ratchet holds the rest.

Identity and config:
- One env closure in `serve`, passed to `Providers`, `Mailer`, `Offers`/`Nomen`, `IssueLifecycleStore` and `Sink::new` (`providers.rs:133,139`, `email.rs:92,98`, `serve/mod.rs:192,224`, `offers/nomen.rs:50`, `issue_lifecycle/mod.rs:29`), plus a test that injects the client secret.
- `config_names()` 409 (`hub_settings.rs:304-319`); overlay (D4).
- `mail_settings` table, with its migration and its own small plan step: it moves `mail.*` rows (`ops/hub_settings.rs:72-154`).
- C7 range test.
- OIDC discovery before save (`hub_settings.rs:274-384`), with the stub issuer in the test.
- Email linking (D5, `providers.rs:690-701`, `identities.rs:259-281`).
- `sign_in_cards::notice` (`providers.rs:208-215`); `Providers::offered` (`passkeys.rs:135-153`).
- `web/cookies.rs` (`__Host-dg_fresh` from `sign_in.rs:20`; the 8 name pairs, the 3 builders).
- One `seal`/`unseal` (`providers.rs:791-869`).
- Rotate the session on securing (`session.rs:22-25`, test `identity/tests/providers.rs:376-380`).
- `sign_in.configured` event test without the secret.
- ID-token refusal tests: wrong `iss`, expired, wrong nonce, discovery mismatch.
- `config/live.rs` → `live/mod.rs`; fix the M7→M9 doc.
- Front doors for `config/` and `login/`.
- Shrink `providers.rs` and `passkeys.rs` through the helpers; split them into folders and lower their `oversize.txt` entries.

Checks:
- `web/tests/forms.rs` (every non-exempt POST answers 403 to cross-site and to Origin-only).
- Store outage gives 503 on `/admin`, `/app/focus` and `/account`.
- `head` test; `copy_tests.rs`; theme test on `/admin` and `/orgs`.
- `identity/tests/conventions.rs` (cookies, provider names, env reads, one origin helper).
- `config/live/tests.rs` C7.
- `hub_settings*_tests.rs`, `passkey_tests.rs`.
- All `tests/*_browser.py` at 390 and 412 px.

### B6: Persistence and the shape of the test suite (6-8 days)

The remaining persistence and tests-and-ci violations.

Fixtures:
- Delete `stores()` (`hub/src/store/mod.rs:87-90`).
- The 4 `Vec<Option<String>>` copies (`ops`, `workflows`, `offers`, `tenancy/tests/access.rs`).
- The 7 `Vec<(TestStore, Accounts)>` wrappers, including `hosted_stores` and `stores_from_before_tenants`.
- The 17 direct loops and all remaining loops (persistence's 96 sites, minus AgentHub's done in B3).
- `analytics_tests.rs:49-58`, `operator_tests.rs:89-98`.
- `if let Some` (`api_keys/tests.rs:255-260`, `preferences/tests.rs:183-188`), renamed as claims.
- `or_skip_blocking!` (`cli/tests.rs:258`, `identity/cli/tests.rs:66,187`, `hub_recovery.rs:19-22`).
- `store/tests.rs:31-33`.
- `hosted_on` split into `hosted_with`/`hosted_on_store`, inlining the `run(None)` wrappers.
- Renames: `registration_refusals`, `assertion_refusals`, `..._on_both_engines_...`.

Schema and SQL:
- Catalog test on `admin()` (RLS forced, grants, `SECURITY DEFINER` locks with the `COALESCE(proacl, acldefault(..))` form), replacing `TENANT_TABLES` (`tenants/tests.rs:473-487`).
- Prefix test on `pg_tables` / `admin()` (`store/tests.rs:236-243`).
- Migration name lint plus `migrations/checksums.txt` (hub `RETIRED` [14,15,16,18]; AgentHub).
- `tenant_id = $n` on the identity sites:
  - `oauth/mod.rs`, 11 sites;
  - `passkey/accounts.rs`, 9;
  - `passwords/mod.rs`, 9;
  - `sessions/mod.rs`, 6;
  - `identities.rs`, `claim`, `links`, `tenants`, `api_keys`, 12.
- Unqualified kernel tables (`kernel/stored.rs` 14, `authority.rs` 9).
- `unsecs` (`connections/stored.rs:35,38`, `api_keys/mod.rs:190`, `sign_in_settings.rs:62`, `analytics_settings.rs:49`, `web/sessions.rs:181`).
- Front-door *Owns* for `connections`, `offers`, `workflows`, `agenthub`, `store`; fix `tenancy/mod.rs:11-13`.
- `crates/store/src/lib.rs` split (S10: `engine`, `transaction`, `tenancy`, `settings`).

Time:
- Inject the nomen confirmation TTL and leeway, replacing the sleeps at `nomen/tests/mod.rs:242,274,306,426`, `kept.rs:42` and `web/tests/nomen_login.rs:296`.
- Device-flow interval as config (`passkey_tests.rs:1442`).
- A parked-waiter signal (`workflows/tests.rs:1094,1127,1581`).
- Signal-tied absence windows (`tenancy/tests/access.rs:409`, `site/tests.rs:778`).
- `analytics_browser.py:197-198` poll.
- Windows console flake: find the cause first (`path.md:134`), then possibly the `os-console` test-group. Retries stay 0.

Python and CI:
- `tests/harness.py`, replacing the 7 `free_port` copies and the 8 cross-script imports.
- `NOT RUN` contract in `device_login_browser.py` and `clients_oauth.py`.
- `security_boundary.py`: a skip fails, and it runs under sudo in e2e, with `mcp_smoke.py`.
- `tests/MANUAL.md` (`native_ui_smoke.py`, `retry_lifecycle.py`, website and desktop JS).
- Desktop nextest on windows-2025.
- `static` job gains rustfmt and clippy `-D warnings`. Measure the backlog first; alpha leaves it red until fixed.
- `timeout-minutes` on `ci.yml:16`, `security.yml:14` and release jobs (hand-kept under `allow-dirty`).
- `[dist.github-action-commits]` SHAs, then `dist generate`.
- `release.yml` top-level `contents: read`.
- Comments: `install-check.yml:7`, `image.yml:133-135`.
- `ci-ok` aggregate job.

Checks:
- `crates/devagotchi/tests/test_shape.rs` (names, skip shapes, every script run or listed, pins, timeouts, permissions), with its baseline empty for the skip shapes.
- `crates/devagotchi/tests/sql.rs` (qualified tables, `AssertSqlSafe` allow-list, no `QueryBuilder`, no `begin`/`finish` outside the store, `tenant_id` ratchet at 0).
- Catalog test in hub-postgres.
- Full workflow run including release on a tag dry-run (`install-check` gate).
- Junit shows 0 `SKIPPED:` in hub-postgres.

## Left to the structure plan (renames only; not part of these batches)

- **S8 (waits on D6):**
  - `web/sessions.rs` → `identity/web/sessions.rs`, with its tests;
  - every `web/*_tests.rs` and `app/approval_tests.rs` / `ticket_tests.rs` to `<domain>/web/tests/`;
  - `hub_settings*` and `ops/hub_settings.rs` → `settings/`;
  - `landing.rs` → `site/web/landing/mod.rs`.
- **S9:** the remaining identity files over 500 lines (`identity/auth.rs`, `nomen/mod.rs`, `oauth/mod.rs`, `web/oauth.rs`, `web/workflow.rs` and the test files).
- **AgentHub baseline (texas-baseline M5):** stale migration comments (hub 0001-0025, 0007, 0021, 0027, 0036 headers); AgentHub's hard-coded `devagotchi.` schema (ADR 0005). Applied migrations are not edited.

## Totals and order

| Order | Batch | Estimate | Gate to start |
|---|---|---|---|
| 1 | B0: patterns plus honest CI | 1 day | none |
| 2 | B1: machine capabilities | 6-8 days | B0 (REQUIRE flag, so the store tests it touches cannot skip) |
| 3 | B2: link and join | 4-5 days | B1 (contracts consts land first; both touch contracts `lib.rs`) |
| 4 | B3: AgentHub operations | 7-9 days | B1 (`check_tools`, `Hub::recorded`, G7) |
| 5 | B4: rest of hub MCP | 3-4 days | B3 |
| 6 | B5: pages, identity, config | 7-9 days | B3 (the approvals page uses the new decide operation) |
| 7 | B6: persistence and test shape | 6-8 days | B4, B5 (its scans need their sites gone) |

Total is about 34-44 agent-days. Each batch is one PR (B3 is one AgentHub PR plus the hub PR that bumps the submodule), merged on green and its branch deleted.
