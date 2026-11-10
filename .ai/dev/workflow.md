# DG-DELIVERY-1: plan, implement, verify, deliver

## Plan before changing or adopting a PR

Create or reconcile a [plan](plans/README.md) with the actual issue/PR and branch.
Use measurable objectives, milestones, tasks/dependencies, acceptance tests, non-goals,
risks, decision approvals and current delivery state. Existing architecture/release
documents remain their subject's authority; link rather than fork them.
A draft PR may be incomplete but must say so. Opening/pulling a PR is not approval
to implement every suggestion, push unrelated work, or merge.

## Operator policy

Managed defaults: branch push after applicable validation is permitted; merge requires
explicit operator approval. `devagotchi ai policy show` reads effective per-checkout
operator configuration. `policy set` records local changes and their acknowledgment.
An advanced operator explicitly acknowledges consequences; do not infer knowledge,
identity, age or competence from conversation, model choice or paid subscription.
Expose fundamental choices plainly; advanced capabilities are opt-in, not a judgement
that less technical people may not use the product.

`policy check --action push|merge` evaluates the local permission with explicit
`--validated` and `--approved` attestations. It does not execute Git, verify test
receipts or authenticate the person holding the terminal. The agent must establish
the evidence and actual approval; flags cannot manufacture either.
Agents must run this gate before any push or merge, supplying attestations only after
the corresponding checks/approval actually exist.
Use protected branches/required review/checks on GitHub for server-side enforcement.
Local policy is a cooperative workflow gate, not a security sandbox.
Even advanced mode never overrides platform permissions or silently promotes production.

## Validation

Iterate with the smallest meaningful existing tests, then run applicable pre-push gates:

```text
cargo build --locked --bins
cargo nextest run --locked
cargo deny --all-features check
python tests/e2e.py --bin-dir target/debug
python tests/security_boundary.py --devagotchi target/debug/devagotchi
python tests/mcp_protocol.py --devagotchi target/debug/devagotchi --socket UNUSED_ENDPOINT
python tests/mcp_smoke.py --devagotchi target/debug/devagotchi
node scripts/generate-contracts.cjs --check
devagotchi ai check
```

Use the platform's binary/endpoint spelling. Process hub tests require isolated Postgres;
analytics tests require the configured isolated ClickHouse. Read harness help/setup;
never aim tests at a user's real data. Contract generation needs the desktop's locked
dev dependencies. Mark unavailable environments unverified, not passed.
Documentation-only edits need link/schema validation, not a needless full binary build.
Changing hooks/capture requires the actual-binary and provider-fixture tests.

## Delivery state is not optional

Run `devagotchi ai delivery status` for local Git facts; it does not fetch or query GitHub.
Report: implementation, validation, commit/branch, push/upstream, PR URL/state, merged
to `main`, and remaining gates. Check remote PR/main state before claiming delivery.
Local refs alone cannot prove the current remote merge state or a squash/rebase merge.

**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN** must be prominent when applicable.
Do not claim "done" merely because code exists. Never auto-merge to suppress this warning.
No arbitrary 15-minute push timer or permission to publish broken/unrelated WIP.
Persist coherent checkpoints; push only the scoped validated branch allowed by policy.

When recommending a correction: name the mismatch, explain the reliability/security/UX
reason, suggest the simplest alternative, and obtain agreement for consequential changes.
