# Complete preserved-source absorption before the self-hosted prerelease

Operator instruction, 2026-11-20: plan the whole change, author the code locally, then
validate and merge each source in oldest-first order. **No new CI runs or pushes that
trigger CI until the complete absorption implementation is authored.** Do not disable
checks, weaken assertions, or replace implementation with plans. This execution order
supersedes the one-PR-per-small-milestone schedule in older absorption plans.

This is the active absorption plan. [The packet instructions](absorption/implementation.md)
and [the exact source inventory](absorption/README.md) are part of it. The existing
[release plan](../../../docs/implementation-and-release-plan.md) still owns product
architecture; this document owns completion of the preserved source queue.

## Outcome and boundaries

One public self-hosted/indie prerelease, with AgentHub visibly usable from the browser,
MCP and CLI, running in Docker with PostgreSQL and one environment containing its owner
and users. Commercial hosting and its free tier come afterward; no customer-tenant signup
or organization management is added to the self-hosted UI. Keep existing development
accounts and data. Development credentials stay in private deployment state, never in
commits, example defaults, images, tests that publish artifacts, or release notes.

Retain the approved Rust binary and existing domain owners. Absorption means carrying the
source's behavior into the product and proving it. A copied archive, an ancestry-only merge,
a retired document, a future milestone, or an empty branch list is not that proof. Do not
write a second scheduler, board store, credential system or parallel Go production service.

Hosted account, coordination, board and configuration data stay in PostgreSQL. ClickHouse
is analytics. Keep a narrowly justified machine-local journal/offline queue where required
by ADR0002; audit each SQLite use and document its concrete purpose. No hosted fallback.

## Verified starting point

- Root `main`: `4b01ac949999dc1c962c855aa38d911122caaa08`, PR278. Shared workflow
  browser behavior passed native and official-image Docker/restart acceptance.
- AgentHub `main`: `8a6db52f774a65e984ddf7aa26d6a7a4aa303c56`, PR16. PR and main
  PostgreSQL CI passed; reviewed and merged trees matched; its feature branch was deleted.
  This includes PR15's cross-board principal limit. The parent gitlink is not adopted yet.
- Prior root deliveries include board/task creation and dependencies, search, board history,
  owner activity, local users, versioned board settings and shared workflows. Reuse their
  code and receipts; do not rebuild them merely because old planning rows say pending.
- Latest prerequisite validation: 921 workspace tests with PostgreSQL required, final build,
  Windows process E2E, protocol/contracts/AI/layout/fmt passed. This proves that prerequisite,
  **not** the unimplemented source gaps or the final prerelease.
- Historical branches below remain intact and incomplete. Current local root work began
  as `increment-288/registered-api-key-agents`, now renamed locally to
  `increment-288/absorb-october-baseline`; reuse it for S1, with no separate profile-only
  PR. Local A01-A03 authoring now includes profiles/policy, receipts, CLI retries and task
  HTTP adapters (root `ed92ea9`, AgentHub `7588df2`); their review ledgers record focused
  evidence. None of these checkpoints is a delivered source closure.

## Fixed source queue

Counts are Git-object inventory, not implementation progress. Renames are recorded as
delete/add so no removed path disappears from review. S1 is the complete tree; later rows
are only the delta from the preceding source. S5 is the preserved commit's own delta.

| Order | Source/ref | Exact commit | Commits | Files to account for |
|---|---|---|---:|---:|
| S1 | Go `origin/box-snapshot/2026-10-18` | `46419c697964016eb547c60416a8f2f5e854a95f` | 91 | 737 |
| S2 | Go `master` | `ca626db10456b8597704032e80944b359b892312` | 2 | 17 |
| S3 | Go `feat/devspace-bridge-contract` | `97172d9a3388afb6c8800b4b28d1ecb3487b4eab` | 5 | 79 |
| S4 | Go `preserve/packaging-2026-10-25` | `adca8b75ffb715a137d94166bf2677f13e5da5ab` | 1 | 32 |
| S5 | Rust `preserve/nov6-uncommitted` | `c936f7f5f4d46032a222cfc0431defb373d5ddb5` | 1 | 42 |

The manifests cover **100 commits and 907 file-change records**. Each record has its blob
hash, packet assignment, target/evidence slots and an explicit unverified state. The surface
inventories list literal routes/tools, exported Go functions and Go test names. Dynamic
registrations must also be expanded from their source lists; the extracted list is not a
claim that every route or generated tool was found automatically.

## Authoring order and dependencies

These are accounting packets, **not separate PR/CI cycles**. The [reconciled code plan](absorption/implementation.md#code-plan-reconciled-on-2026-11-21)
groups them into complete authoring batches with concrete owners, contracts and acceptance.
Finish each batch before its check; A05 catalogs precede the A04 grant consumers within
one batch, and A08 integration follows all event producers. Existing equivalents need
source-specific verification and a receipt, not a rewrite.

| Order | Packet | Complete work unit | Depends on |
|---:|---|---|---|
| 1 | A00 | Complete source/commit accounting, requirements and approved replacement decisions | none |
| 2 | A01 | Close task, workflow, settings, presets and original board edge-case parity | A00 |
| 3 | A02 | Credential-bound agents, human linkage, roster, kinds, roles and board policy | A01 |
| 4 | A03 | Mutation replay, REST/protocol/stream and CLI/MCP parity | A02 |
| 5 | A05 | Versioned catalogs, schemas and seeded configuration | A03 |
| 6 | A04 | Enforced machine grants, sandbox scope, change review and policy decisions | A02, A03, A05 |
| 7 | A06 | Provider rows/models, subscriptions, role routing and write-only secrets | A02, A05 |
| 8 | A07 | Editable/versioned/signed guides, updates and real run steps | A04, A05, A06 |
| 9 | A09 | Remaining original file/document/process tool behavior | A04 |
| 10 | A10 | Original GUI sessions/capture/input and their authorization | A04, A09 |
| 11 | A08 | Action feed, category retention, archives and verification | A03-A10 |
| 12 | A11 | Finish all corresponding user journeys and failure states | A01-A10 |
| 13 | A12 | Baseline packaging, configuration, docs and release integration | A01-A11 |
| 14 | B01 | S2's 17-path delta, including later product requirements | S1 authored |
| 15 | C01 | S3's 79-path enrollment/auth/connectivity/security delta | S2 authored |
| 16 | D01 | S4's 32-path packaging/readiness/shutdown delta | S3 authored |
| 17 | E01 | S5's 42-path capabilities/provenance/retry/file/Windows delta | S4 authored |
| 18 | R01 | Full validation, sequential green merges, deletion and prerelease acceptance | all authored |

No new commercial features, branding redesign, generic architecture cleanup, framework
migration, or speculative roadmap work is added to these packets. Source-backed missing
behavior is in scope. If a file's implemented behavior contradicts a historical roadmap
deferral, keep that behavior in this absorption queue; do not silently erase it as deferred.
A genuinely unresolved authority/privacy conflict remains open in A00, with code/source
evidence; it cannot be marked absorbed by an executor's guess.

## Git and CI procedure

1. Preserve original dirty checkouts. Record source SHA/tree and all live refs before work;
   inspect any additional newly discovered refs and append them chronologically. Do not
   overwrite the preserved working tree or force-update source refs.
2. Reuse the current local branch for S1. AgentHub library changes belong on one matching
   local submodule branch; parent changes use its local gitlink. Small local commits are
   allowed. Each commit says source SHA/paths and packet ID. No pushes, PR creation or CI
   dispatch during authoring; no independent PR for a query, index or form component.
3. At a source boundary, record the completed commit and tree for BOTH repositories in
   the ledger. Continue the next source's **delta** on the same local integration branch;
   do not create checkpoint/child branches during authoring. Keep S1-S5 order and separate
   coherent commit ranges so each source can be reviewed and delivered in that order.
   Do not start the next source while the preceding source still has unauthored rows.
4. `authored` requires domain implementation, migration, every applicable surface, error
   handling, tests/probes and documentation actually present. TODOs, a route without an
   implementation, unsupported placeholders for supported source behavior, or a future
   milestone fail this state. It is distinct from `validated` and `merged`.
5. **Only once S1-S5 are authored**, run the complete integration validation described below.
   Resolve failures locally. Do not push a known failing tree to outsource debugging to CI.
6. Publish the oldest ready source. If it changes the AgentHub submodule, validate/merge
   that repository first, verify its reviewed tree on main, and update the parent gitlink
   to the merged commit. Publish the parent source checkpoint, wait for all required checks,
   fix genuine failures, merge with the exact reviewed head, verify remote main and main CI.
7. Delete only that completed integration branch, verifying its current remote head first.
   Original historical refs are eligible only after their entire file/behavior/commit ledger
   is closed and their source history has a durable archive/tag or Git bundle. This preserves
   provenance through cross-repository ports without pretending ancestry alone proves parity.
8. Apply the next already-authored source commit range onto the verified new main for its
   delivery; inspect every conflict against both source contracts, run affected checks,
   and merge/delete its delivery ref by the same procedure. Preserve the complete local
   authored history and recorded trees before moving any delivery ref.
   Do not use blind `ours`/`theirs`, blanket squash-away conflict resolution, or force pushes
   to main. Keep any original ref carrying unique unverified work.
9. Finish with a fresh local and remote ref inventory for both canonical repos and every
   preserved source checkout. Account for every feature ref; distinguish archival tags from
   active development branches. Main is the sole active delivery branch when done.

This batches authoring without removing verification. Existing CI workflows/protections
remain enabled. Normal status reads are allowed; manual workflow dispatches and pushes that
start them are held until step5. No new CI was started while authoring this plan.

## Local checks while writing; full checks after authoring

During implementation, run the smallest relevant compile/test or direct probe after a
coherent behavior change. PostgreSQL tests use isolated disposable data with the required
flag, never the development database. Do not repeatedly rebuild/test the entire workspace
after an index, helper or copy change. Do not modify source files during a running suite.

After all authoring: build the final binaries; run the default workspace suite with real
PostgreSQL required, the dependency audit, contract generation, layout/fmt/AI checks and
Windows process/MCP probes. Run the native browser journeys in the packet instructions.
Use outside-checkout scratch on D:. Do not run the target executable while Cargo replaces
it. Keep failed logs; never turn a missing service into a passing skip.

Platform-specific Linux IPC, sandbox, GUI, macOS/ARM and ClickHouse checks run in their
actual supported environments when CI resumes. A Windows pass is not their proof. After a
failure, rerun the failing/affected checks; repeat broad validation only when the change
invalidates its evidence. One successful full run on the final authored tree is the baseline.

Exact existing entry points (run from repo root, adapted to the native OS):

```text
cargo build --locked --bins
cargo test --locked --workspace -- --test-threads=4
cargo deny --all-features check
cargo fmt --all --check
node scripts/generate-contracts.cjs --check
target/debug/devagotchi.exe ai check
py -3.11 tests/e2e.py --bin-dir target/debug
py -3.11 tests/mcp_protocol.py --devagotchi ABSOLUTE_EXE --socket ABSOLUTE_UNUSED_PATH
```

Set `DEVAGOTCHI_TEST_POSTGRES_URL`, `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1`, and a separate
empty `DEVAGOTCHI_HUB_STORAGE_URL` for the E2E harness. Add the new source-specific probes
as each packet is implemented; do not claim these existing commands cover missing tests.
Linux additionally runs `tests/security_boundary.py`, `tests/mcp_smoke.py`, the source
grant/sandbox/GUI probes and deployment tests. Existing CI is the platform matrix authority.

## Docker and prerelease acceptance

After the integrated source commits are green on main, build/use the official image for
that exact SHA, verify its OCI revision, and pin the digest. Update only the development
hub service and its intended configuration; preserve PostgreSQL volumes and existing users.
Also test a separate fresh install so preserved development data cannot hide bootstrap gaps.

The owner and an ordinary user must complete these real browser journeys at 390 and1280
pixels: sign in; create/manage a board and tasks/dependencies; register/connect an agent;
observe/limit its real work; edit workflow and policy; approve/revoke machine access; inspect
actions/history/archives; manage provider/subscription and run a guide. Source-supported GUI
and document operations must run, not just render controls. Restart the hub and verify
accounts, settings, bindings, grants, tasks and audit continuity. Revoke a credential and
confirm an existing session cannot continue. No organization UI in this edition.

Prepare release notes against actual code, installation/update/recovery instructions,
supported-platform matrix, license notices, artifact checksums and unresolved limitations.
Never label a missing absorbed capability as a harmless limitation to pass closure. Keep
stage `alpha`/prerelease. Do not move an existing public tag or claim production readiness
as a side effect of this consolidation; release publication follows the repository's
existing release procedure after these results are concrete.

## Completion ledger and stop conditions

Every manifest row must end in one of: `implemented` (targets + acceptance evidence + merged
commit), `equivalent` (existing targets + source-specific evidence), `superseded_in_source`
(the exact later source change and its verified replacement), or `archival` (non-runtime
evidence/build output/license material preserved with a reason and immutable source ref).
`unverified`, `planned`, `deferred`, `partial`, missing targets and missing evidence all keep
the branch open. A runtime capability cannot be categorized archival to evade implementation.

Each source receipt records original SHA/tree, file and commit coverage, implementation
heads, tested heads, check URLs/results, merged SHAs, deployed image where relevant and
deleted refs. A source is closed only after its receipt has no open runtime or requirement
row. Raw credentials/logs/screenshots stay private; public receipts contain only safe facts.

Plan: source/target execution batches reconciled on 2026-11-21, including catalog-before-grant
ordering, exact integration seams and later-delta distinctions. Implementation: A01-A03
partially authored locally; A05 catalog administration and A04 access foundations are
also authored with focused evidence. Complete source absorption remains unfinished. Validation:
focused results are recorded in the packet review ledgers, not final integration evidence.
Push/PR: no consolidated change published. Main: no consolidated source delivered. Continue
Batch 2 on the existing integration branch, retaining remaining Batch 1 acceptance;
CI remains held until S1-S5 are authored.

**DELIVERY INCOMPLETE: FULL SOURCE ABSORPTION IS NOT YET IMPLEMENTED OR MERGED.**
