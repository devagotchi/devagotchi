# B3: AgentHub board operations follow their pattern

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: the operator, 2026-11-19: carry out the rewrite plan batch by batch, B3 next
  ([rewrite](rewrite.md), B3).
- PR: agenthub: the store work went to its main directly (`d149bdc`, `3866d18`, the operator's
  push), then devagotchi/agenthub#8 (`f4af7f9`, formatting). This repository: not_created.
- Branch / baseline: `increment-273/board-operations` in both repositories, from devagotchi
  main `3e2e5b9` and agenthub main `5c1499c`.
- Objective: every board operation is one transaction with one event per action, every surface
  forwards one call and leaves one audit line, and the board tools keep what
  [board-operations.md](../../patterns/board-operations.md) and
  [mcp-tools.md](../../patterns/mcp-tools.md) promise, each held by a check.
- Non-goals: the rest of the hub MCP surface (B4); pages, identity and configuration (B5); the
  SQL scan and the test-suite shape (B6); new board features.
- Canonical architecture / related plan: [board-operations.md](../../patterns/board-operations.md).

## Milestones and tasks

Each item of the rewrite plan's B3 section, and where it went.

| Milestone | Task | Acceptance evidence | State |
|---|---|---|---|
| M1 store layout | `hosted.rs` and `hosted/operations.rs` split by subject into `hosted/{mod,tickets,leases,notes,graph,approvals,standing,settings,inbox,issues,reads,registry}.rs`, each under 500 lines; `migrations.rs` to `migrations/mod.rs`; hook tests to `cli/hooks/tests.rs` with a `//!` front door | layout ratchet green, both entries gone from `oversize.txt` | done |
| M2 single-transaction operations | `ask` with a shared `decide_in`; `decide_and_allow`; `grant_standing`, `revoke_standing`, `revoke_own`, `live_standing`, `suggestions` in the store on its clock; `delegate`; `undo_close` (`undo_too_late`); `set_wake`; create's default repo in `create_in`; `check_wip` shared; `Board::now` and `edit_settings` gone | `tests/operations.rs`: each, with atomicity by a refused outbox row | done |
| M3 the chain | `chain::actions` (23 names, the five D7 renames); handoff records `assignee`; no-ops write nothing (`add_dep`, `set_project`, `put_settings`, `put_workflow`, `set_wake` of nothing, `revoke_own` of nothing); `put_settings` names its changed keys | `tests/actions.rs` | done |
| M4 refusals | `decode` for cursors and wakes (`board_damaged`); `invalid()` in handoff; MCP `next_for` and CLI `next_for` lines for every reachable code; `parent_closed` and `request_conflict` are conflicts | `contract.rs` scan; `cli/tests/output.rs` scan | done |
| M5 issue tools | `Board::issue_context`, `issue_source`, `write_issue_context(&Actor)` in `board/issues.rs`; tools to `mcp/tools/issues.rs` with titles; the REST checkpoint takes an `Actor` | hub suite | done |
| M6 hub without board SQL | `hosted::registry::{names, boards, has_board, lease_delays}`; the hub registry and the board signal sink use them and `events_page` | no `board_*` SQL left in `crates/hub/src` outside tests (grep) | done |
| M7 tool form | `deny_unknown_fields` on the 22 inputs; 46 field docs, the issue inputs' docs; T1 titles; `board_claim` not idempotent; `board_settings` destructive; `READ_ONLY_TOOLS`, `PER_CALLER_WRITES`, `viewer_may_call`; every board tool's return type spelled out so it declares an output schema; list answers as objects; wait bounds as consts; host names out of tool docs | `tools-not-yet.txt` holds no board tool; new rule O4 | done |
| M8 audit (G7, D1) | `board_call_scoped` audits after the call, refusals included (the seat, a missing fence, a dep kind, a webhook URL); `board_wait`, `approval_wait`, `board_bind` and an unmapped issue repository audited once; outcomes `ok` with status or `created` in the target; approvals page, ticket actions, `stop_own` and the REST checkpoint write one line each through `record_completion` | `audit-not-yet.txt` holds no board tool | done |
| M9 docs | `docs/agent-board.md` codes and tools; `plugins/README.md`; `agenthub/tests/ahead.rs`; `agenthub/mcp/mod.rs`; AgentHub's README; the patterns (board-operations rewritten, mcp-tools O4 and B3 items marked, layout rule 2's check, README table); CHANGELOG | review | done |
| M10 validate, publish, merge | full workspace suite with Postgres required; e2e on a fresh database; the agenthub PR, then this PR | receipts below | pending |

Also in this batch, found on the way:

- The layout check for rule 2 (`<x>.rs` beside `<x>/`) with a shrink-only list of the 26
  modules outside AgentHub that break it today; each goes in the batch that owns its crate.
- `suggestions` read only the first 200 approvals, oldest first, so a busy board stopped
  suggesting; it now asks for the last 30 days directly.
- `platinum`, a private host name, came out of the hub's tests, its add-a-machine placeholder,
  the turnkey video and the agenthub standing test; `Texas` and `dabo` came out of the
  approval tool's docs. The docs still name hosts (`deploy/linux/README.md`,
  `docs/product-and-platform-plan.md`, `docs/documentation-map.md`): for the operator.
- `web/approvals.rs` became `web/approvals/` with the approve-ahead handlers in `ahead.rs`, so
  it shrank under its ratchet entry.

## Risks and approved decisions

- D1 (operator, 2026-11-19): after the effect, return the real result, log, raise the alert.
  Applied to every board surface here.
- D7 (renames allowed in alpha, noted in the release notes): the five action renames.
- C17 (wire changes in alpha): list answers become objects (`{tickets}`, `{events}`,
  `{deps}`); inputs refuse unknown fields. The command line's remote face, the probes and the
  hub tests read the new shapes in this batch. An MCP client that cached the old tool list
  sees structured content where it saw text before.
- A board read in the signal sink now takes the board row lock, as every board read does.

## Verification receipts

2026-11-19, Windows 11, devagotchi at this branch with `crates/agenthub` at `f4af7f9`:

- `cargo fmt --all -- --check`: clean. `cargo build --locked --bins`: ok.
- `cargo test --workspace --features devagotchi-store/testing` with
  `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1` on the test Postgres: 32 binaries, 861 passed, 0
  failed, nothing skipped.
- `python tests/e2e.py` on a fresh database: E2E PASSED, including the board probe (three
  subscriptions on one board, reading the new `{tickets}`, `{events}` and `{deps}` answers),
  the signals probe (the board sink on `events_page`) and the orgs probe.
- AgentHub alone (`cargo test -p devagotchi-agenthub`): 87 passed. Its own CI, `test
  (postgres)`, passed on `3866d18` and on #8.
- Not run here: `tests/app_browser.py` (the approvals page and ticket actions in Chrome) stages
  the Linux installer through bash and runs only on Linux; CI's `ubuntu-24.04` job runs it.
- Found while validating: the web tests' world configures a broker named `lab`, so the
  add-a-machine test's machine is `studio`.

## Delivery

Implementation: done. Validation: pending. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
