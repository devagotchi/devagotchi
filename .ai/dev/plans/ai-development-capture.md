# AI development contracts and local session capture

- Request: repository-neutral `.ai`, canonical instructions, provider adapters, alpha
  lifecycle, configurable operator autonomy, actual private runtime capture.
- PR: [#164](https://github.com/devagotchi/devagotchi/pull/164). Its live checks/merge
  state are authoritative; this file records candidate execution receipts, not an
  automatically refreshed remote-status mirror.
- Initial verification checkout: `increment-117/signal-contract`,
  `483e9380ec4a49f5f27f3ed53975c615b4dc9fba` plus dirty work. This shared checkout
  also contains independently changing, unrelated work; this packet does not own its
  branch or authorize publication of those changes.
- Delivery branch: `increment-118/ai-development-capture`, isolated worktree based on
  remote `main` (`738338d52d6dc4baf2426e165eb465d8755a13ac`). No signal, organisation,
  ClickHouse packet commits or unrelated dirty hunks are included.
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
| M5 | Scoped validated push, reviewed PR, operator-approved merge | M4, actual approval | remote state confirmed | approval received; publication in progress |

Operator explicitly requested integration into main after review/validation, with
pre-release development retained and no production promotion. The project stage
describes alpha development; machine state now separately marks `pre_release` mode
and `alpha_readiness: unconfirmed`. Actual alpha acceptance is not claimed.

Research extension: [cross-project training-data standard](../../docs/cross-project-training-standard.md).
This inventories the target data, reusable standards packaging, rights/redaction/review
pipeline, dataset versions/splits and verification gates. The document explicitly
separates the implemented capture foundation from unimplemented curation/export products.

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

### Isolated main-based candidate verification

The delivery candidate excludes the original worktree's unrelated changes and uses the
remote-main baseline above. Verification on Windows:

- `cargo build --locked --bins`: passed. Candidate executable SHA-256:
  `97114ba2dd8ce1648e8d9fa593657ef7a6b1dc7719768fb04bbe8d346b96f32c`.
- `cargo test --locked`: **587 passed, zero failed/ignored**, including **472 hub
  library tests**, all native-binary tests and the capture/governance tests. A separately
  labelled disposable Postgres 18 instance enabled real database tests; no shared
  database content was used. Nextest is not installed; this is a Cargo test receipt.
- `cargo deny --all-features check`: passed, with existing duplicate-dependency warnings.
- `python tests/e2e.py --bin-dir target/debug`: **E2E PASSED**, real broker, MCP, hub,
  link and isolated Postgres. Existing Linux machine-update/deploy-script cases are
  explicitly skipped by this Windows harness; no cross-OS certification is claimed.
- `python tests/mcp_protocol.py` with the absolute candidate and unique unused named
  pipe: passed, **11 checks** covering protocol/catalog, input rejection and shutdown.
- `security_boundary.py`: unavailable on Windows (`os.geteuid` at module import).
  `mcp_smoke.py`: unavailable on Windows (`select` on subprocess stdout).
  These existing Linux-only harness limitations are recorded, not silently fixed,
  suppressed or reported as successes. The Windows E2E/process tests passed separately.

All commands used isolated local fixtures. No actual provider API/chat session or
training upload was performed. Raw capture remains opt-in.

After the operator's main-integration/pre-release clarification, the **17 targeted hub
tests and 3 + 6 native tests were rerun and passed** for explicit release-mode/readiness
gates and accurate automatic-transcript-following status. Final Windows candidate
SHA-256: `df1fd34e20e648624c7622e0d4df73a5d9aee1cccd8472994a7cae0257072311`.
Canonical schema checks, 14 instruction/contract documents and 48 local links passed.
Worktree-generated broker artifacts initially differed only in checkout line endings;
regenerating with the existing canonical tool resolved drift with no semantic/generated
Git diff. The task-labelled disposable Postgres container was stopped and removed.

First CI run: macOS completed **525/526 tests**, failing only the new contention test's
four-second wall-clock assertion (observed 4.57 seconds under parallel runner load).
The regression now verifies `PRAGMA busy_timeout` equals **2000 ms** exactly and checks
the actual lock refusal/no-mutation/recovery with a separate ten-second hang guard.
The contract explicitly distinguishes SQLite's configured timeout from OS scheduling;
this is not a suppressed failure or a hard real-time guarantee. Remaining CI is required
before merge.

The corrected candidate's Windows 2025 run completed **586/587 tests**: a stress-test
writer received the documented explicit `DatabaseBusy` result after its configured
wait. The fixture now performs at most five attempts per canonical event, retries only
the typed busy error with visible diagnostics, and still verifies all **41** unique
events after reopen. Other errors and retry exhaustion remain failures; runtime lock
timeout, consent serialization and hook error reporting are unchanged.

## Delivery

Implementation: implemented for this packet. Local validation: passed as scoped above.
Live-client acceptance: pending operator admission. Publication: in progress.
Main integration: explicitly approved by the operator, pending reviewed passing CI.
**DELIVERY INCOMPLETE: PUBLICATION / MERGE STILL REQUIRE REMOTE CONFIRMATION.**
