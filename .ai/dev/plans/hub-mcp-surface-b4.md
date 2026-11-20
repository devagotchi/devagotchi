# B4: the rest of the hub MCP surface follows its pattern

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: the operator, 2026-11-19: carry out the rewrite plan batch by batch
  ([rewrite](rewrite.md), B4).
- PR: #264, merged as `726af4e`
- Branch / baseline: `increment-274/hub-mcp-surface` from main `ec69ed6` (B3).
- Objective: every hub tool outside the machines and the boards follows
  [mcp-tools.md](../../patterns/mcp-tools.md), so the `NOT_YET` lists are empty and deleted.
- Non-goals: pages, identity and configuration (B5); the SQL scan and test shape (B6).

## Milestones and tasks

| Milestone | Task | Acceptance evidence | State |
|---|---|---|---|
| M1 inputs | `deny_unknown_fields` on `CallIn`, `DictionaryInput`, `ResolveInput`, `ProposeInput`, `ReviewInput`, `HandoffInput`; the 12 undocumented fields documented | `check_tools` I1, I2 | done |
| M2 hints and names | `workflow_start` destructive and not idempotent, `workflow_cancel` destructive; `timeout_secs` to `timeout_seconds` (with `WAIT_*_SECONDS`) and its tests | `check_tools` A2; workflow tests | done |
| M3 audit | `dictionary_get`, `lexicon_resolve`, `lexicon_review` (list, history, refused arguments), `lexicon_handoff` (every step) audited once through `Hub::recorded`; `lexicon_propose` too; `record_workflow` and `record_offers` gone for `recorded`; `connections` and `connection_call` through `recorded`, `connection_call` outcome `ok` with method, path and status in the target; `workflow_cancel`'s early `?` gone | `scan.rs`; `connections/tests.rs` one `ok` line | done |
| M4 refusals | `early_access` as `structured_error` `{code, message}` (`key_denied` already was); `gate.rs` doc corrected | `tenancy/tests/access.rs`, `workflows/tests.rs` | done |
| M5 layout and docs | connection tools to `connections/mcp.rs`; `WhoAmI`/`Machine` comments say `skip_serializing_if` keeps a field out of `required`; `connection_call` says its answer is data | `check_tools` D3; review | done |
| M6 checks | `tools-not-yet.txt` and `audit-not-yet.txt` deleted; `tools.rs` and `scan.rs` require no violation; behaviour tests over HTTP-MCP: a bad seat, a missing fence, a bad dependency kind, a refused wait, a bind, a wait that answered, each one line with `ok` or its code; `get_usage_stats` counts no success a failure | `agenthub/tests/audited.rs` | done |
| M7 validate, publish, merge | full workspace suite; e2e on a fresh database; PR | receipts below; CI 18 of 18 on #264 | done |

Found done already: B1 had moved the machine tools' inline audit copies (`machines/files.rs`,
`run.rs`) onto `record_completion`; the D5 instructions test exists in `offers/tests.rs`
(offers shown, and nothing of them shown when off); `key_denied` was already a structured
refusal. `workflow_browser.py` and `first_connect_browser.py` run in CI's ubuntu job.

## Risks and approved decisions

- C17 (wire changes in alpha): `timeout_seconds` replaces `timeout_secs`; `early_access`
  answers with structured content. In the CHANGELOG.

## Verification receipts

2026-11-20, Windows 11, test Postgres required, at this branch on main `ec69ed6`:

- `cargo fmt --all -- --check` clean; `cargo build --locked --bins` ok.
- `cargo test --workspace --features devagotchi-store/testing`: 32 binaries, 864 passed, 0
  failed, nothing skipped. `mcp::tests::tools` and `mcp::tests::scan` pass with no baseline.
- `python tests/e2e.py` on a fresh database: E2E PASSED.
- The hub's tool list (71 tools) against the MCP TypeScript SDK's own `ListToolsResultSchema`
  (`@modelcontextprotocol/sdk` 1.32.2, locked in `tests/clients`): accepted.
- Not run here (Linux only): `workflow_browser.py`, `first_connect_browser.py`, the real-client
  OAuth step; CI's ubuntu job runs them.
- Layout: the kernel tools' types moved to `kernel/schema.rs` (`kernel/mcp.rs` was 524 lines);
  `workflows/tests.rs` is 1764 lines, under its ratchet entry.

## Delivery

Implementation: done. Validation: done (local suite, e2e, SDK check; CI 18 of 18). Push: done.
PR: #264. Main: merged as `726af4e`.
