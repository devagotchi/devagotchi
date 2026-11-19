# B1: Desktop Commander core follows its pattern

Branch: increment-209/desktop-commander-pattern. Base: a61cfb7 (#259).
PR: pending. Implementation: in progress. Validation: pending. Push: pending. Main: not merged.

## Objective and authority
Every existing machine capability follows contract → broker → local tool → hub tool, with shared descriptions/defaults, local/hub parity and probes covering every local tool. Follow ../../patterns/machine-capability.md and ../../patterns/mcp-tools.md; reconcile their B1 known violations.

Operator decision D1, confirmed in this request: after execution, return the real result even when auditing fails; log the audit failure and raise audit.write_failed as an operator alert. Do not retry an effect to repair auditing.

## Non-goals
No new Desktop Commander capabilities, board rewrite (B3), general hub/domain rewrite (B4), link protocol rewrite (B2), production promotion or merge without approval.

## Milestones and dependencies
1. Canonical contracts, descriptions, defaults, bounds and exhaustive dispatch tables.
2. Thin local and hub machine adapters, flat machine inputs, truthful hints and coded invalid arguments.
3. One audit completion helper, real outcomes on failure, operator signal and fault tests.
4. Shared tool checks, full machine parity, broker tests, complete real-binary catalogs/probes and CI static checks.
5. Generate artifacts, focused checks and applicable pre-push validation; one reviewed PR.

## Acceptance and validation
- Every REQUEST_METHODS entry except operations_changed maps to a broker tool.
- Shared tools have identical descriptions, hints, field docs, types and defaults after removing machine routing.
- Every catalogued local tool has an exercised probe case; unknown tools stay protocol errors and known invalid arguments return invalid_arguments.
- Audit failure preserves successful/failed real results, logs and emits an independent operator alert, with exactly one effect attempt.
- Run focused Rust tests, native Windows probes, generated contract check, ai check and applicable delivery suite. Record unavailable Postgres/Linux checks honestly.

## Risks and delivery
Flat operation inputs intentionally change alpha MCP schemas; update callers, probes and changelog. Keep wire RequestBody unchanged. Audit alerts must work when the audit store fails and must not leak arguments/output. Existing operator-policy command currently fails with OS access denied; resolve or report before push.
