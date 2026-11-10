# AI development contracts and local session capture

- Request: repository-neutral `.ai`, canonical instructions, provider adapters, alpha
  lifecycle, configurable operator autonomy, actual private runtime capture.
- PR: not_verified; no PR created for this packet.
- Observed verification checkout: `increment-117/signal-contract`,
  `483e9380ec4a49f5f27f3ed53975c615b4dc9fba` plus dirty work. This shared checkout
  also contains independently changing, unrelated work; this packet does not own its
  branch or authorize publication of those changes.
- Stage: **alpha**, [DG-LIFECYCLE-1](../../docs/lifecycle.md).
- Broader product authority: [release plan](../../../docs/implementation-and-release-plan.md).

## Objective and approved decisions

One provider-neutral development contract with thin native adapters; managed default
permits validated branch pushes, requires merge approval; advanced capabilities require
explicit operator acknowledgment, never inferred competence. Capture is local/private and
opt-in, content consent separate. Implement controls now, not merely future documentation.
Use existing hub library/binary and local SQLite journal, never a second hosted authority.

Non-goals: automatic production promotion, automatic merging, hidden reasoning extraction,
global provider history scraping, cloud transcript upload, guaranteed complete chat
capture, automatic training eligibility, private WardenMesh fork creation.

## Milestones / tasks

| Milestone | Tasks | Dependencies | Acceptance | State |
|---|---|---|---|---|
| M1 | Canonical instructions, `.ai` structure, PR/plan templates and alpha contract | approved requirements | consistent provider links and machine stage | implemented; local checks passed |
| M2 | Native operator policy and private journal | M1 | managed/advanced consent, durability/replay/concurrency tests | implemented; 17 targeted tests passed |
| M3 | Claude/Codex/Copilot native stubs, bounded incremental import/export | M2, official payload research | fixture mappings, privacy/gap/refusal tests; actual binary without board | implemented; 3 new native tests passed |
| M4 | Validate, reconcile documentation and report actual delivery state | M1-M3 | schema drift, tests, scoped diff and evidence receipts | local tests passed; actual-client admission remains operator acceptance |
| M5 | Scoped validated push, reviewed PR, operator-approved merge | M4, actual approval | remote state confirmed | not_started |

## Risks

Local config/attestations are not authentication or branch protection. Hooks can be
unavailable, untrusted or time-limited. Raw content may contain secrets; explicit consent
and restricted storage are essential. Transcript formats are unstable/opaque. Native
fixture tests do not establish end-to-end admission in a client that was not launched.
Windows custom state directories inherit ACLs and require operator privacy verification.

## Verification receipts

Local Windows verification on 2026-11-10 (dirty checkout, not a release candidate):

- `cargo build --locked -p devagotchi --bin devagotchi`: passed.
- `cargo test --locked -p devagotchi-hub --lib ai::tests`: **17 passed**, zero failed/ignored.
  Covers opt-in, metadata exclusions, policy acknowledgment/revocation, stale-policy refusal,
  all configured native mappings, schema/lifecycle drift, source/record limits, exact source
  hashes, partial JSONL tails, stable retries/conflicts, concurrent writers, reopen durability,
  bounded contention and integrity checks.
- `cargo test --locked -p devagotchi-contracts`: **2 passed**, zero failed/ignored.
- `cargo test --locked -p devagotchi --test ai_workflow --test one_binary`:
  **3 new + 6 existing passed**, zero failed/ignored. Actual subprocesses, paths with spaces,
  provider configs, policy controls, authorized transcript following, unsupported formats,
  no initialized board and preservation of existing native board Stop semantics.
- AI-document check: **13 documents and 9 evaluation definitions passed**; local links
  are checked against actual files.
- Scoped Rust formatting and tracked patch-whitespace checks: passed.
- Canonical broker schema/desktop type drift check and `devagotchi ai check`: passed.
  A final broker-schema recheck initially hit the shared Cargo build-lock timeout;
  repeating against an isolated task-owned target directory passed. This was an
  infrastructure contention failure, not a suppressed test failure. AI capture schema
  was generated from Rust and checked by the actual binary test.

This is not a full pre-push/product release certification. No CI dispatched. No actual
Claude Code/Codex/Copilot application session or cloud sandbox was admitted/launched here.
The binary was not initially on this environment's PATH; follow the
[source-testing steps](../../docs/adapters.md#testing-this-checkout-from-source).
Capture remains off unless the operator deliberately opts in. No local chat data or
operator consent was fabricated to make a demonstration appear complete.
Hosted project transcript publication, desktop GUI policy controls and automatic curated
dataset generation are not claimed by this local CLI capture packet.

## Delivery

Implementation: implemented for this packet. Local validation: passed as scoped above.
Live-client acceptance: pending operator admission. Push: not performed for this packet.
PR: not_verified; none created here. Main: not_verified.
**DELIVERY INCOMPLETE: THIS PACKET IS UNCOMMITTED / NOT PUSHED / NOT CONFIRMED MERGED TO MAIN.**
