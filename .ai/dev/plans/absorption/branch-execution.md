# Branch execution detail, 2026-11-21

Part of [the active plan](../absorb-preserved-sources.md), not an alternative backlog.
Plan all five sources now. Execute **S1 fully -> validate -> green merge -> verify main ->
delete eligible refs**, then S2, S3, S4, S5. The original Git objects and dirty checkouts
remain read-only throughout porting. No source is closed by this document.

Publication update, 2026-11-22: the active plan's publication correction supersedes the
older hold-publication statements below and in packet ledgers. Publish the validated
authored integration through AgentHub PR17 and its parent PR; continue source completion
oldest-first. Partial integration does not close S1 or permit deleting preserved sources.

## Starting evidence and remaining work

Reconciliation began on `increment-288/absorb-october-baseline` at `a78b70f`; AgentHub is at
`db1fb9d`. Both were clean before this reconciliation. Root main was last verified at
`4b01ac949999dc1c962c855aa38d911122caaa08`; AgentHub main at
`8a6db52f774a65e984ddf7aa26d6a7a4aa303c56`. Re-read remote heads at publication.
The local maintenance/expiry work has focused evidence; no complete source has been
published by this absorption branch. The manifests' open rows remain open.

Inventory: S1 737 paths/91 commits; S2 17/2; S3 79/5; S4 32/1; S5 42/1.
Exact filenames, deletions, blobs and commit lists stay in `s1-files.json` through
`s5-files.json`. The units below refine their packet destinations. Source tests are
requirements to reproduce, never current passing evidence.

## S1: finish the October baseline

Use [implementation batches 1-5](implementation.md#complete-authoring-batches) for all
A00-A12 rows. Reconcile already-authored modules using the A01-A05 review ledgers; do not
recreate them from older paragraphs that label their paths "new".

| Unit | Exact next implementation work | Acceptance before S1 closure |
|---|---|---|
| A00 + A01-A03 | Finish each open operation/commit comparison in `a01-review.md`, `a02-review.md`, `a03-*.md`; existing `agenthub` hosted policy/task/receipt owners, hub HTTP/MCP/CLI and SSR forms remain destinations. Audit intermediate removed behavior across all 91 commits, not only head filenames. | Every source operation and refusal has an adapter and source-specific receipt; task fences, configuration CAS, API-key/OAuth principal binding, replay and stream revocation remain consistent across surfaces. |
| A05 | Existing `hub/src/catalog/` and catalog web pages are authored. Finish schema consumers, referential deletion, edited-seed preservation and dynamic kind/route coverage. | All eight accepted kinds validate their actual consuming runtime; source customer-tenants catalog maps to the accepted one-environment/users decision. |
| A04 ordinary dispatch | Source `machineops/ops.go` routes ordinary calls through `access.Authorize`. `mcp/hub.rs::call` now uses `mcp/callers/start.rs` for account-backed operation starts. `machines/run.rs` and Get/Read/Wait now use the same dispatcher and admission-bound observation. Input/resize/cancel now also use action-bound control. Remaining file/other effect requests, list/history surfaces and direct sends in `machines/files/mod.rs` still need grant enforcement. Extend the same authenticated dispatch owner to every account-backed machine request. Keep explicitly configured local trusted IPC separate. | Actual MCP run/start/file/process calls with no grant, foreign principal, revoked key/profile/grant, stale task, exhausted use and changed enrollment refuse before effect. Owning an account or omitting a grant field cannot bypass. No alternate batch-file/image route escapes. |
| A04 selection and admission | Ordered, bounded candidate selection for starts is now authored in `machines/access/broker/selection/`; task/credential filters are locally validated; finish other capability projections. Query only the credential's account/principal/machine; choose the source's least-privileged matching grant deterministically, never a caller-supplied principal. Existing retry key stays bound to its original grant even when a more permissive grant appears. Recheck selection in `current_in` and atomically consume in `admit`. | Concurrent last-use admissions; duplicate retry consumes once; changed canonical args conflict; no arbitrary first-row selection; matching grant survives pagination; no fallback on database/canonicalization failure. |
| A04 broker requests | Extend existing `broker/remote/` Prepare/Execute for each non-start machine request; derive paths/argv/hosts from validated canonical inputs at the broker. File effects retain existing serialization/CAS/journal owner. Process follow-ups resolve original admission and operation; observation uses separate current read permission and never renews execution as a side effect. | Exact broker target, operation digest and grant scope match actual effects; changed path/symlink/host, cross-principal operation and unknown adapter refuse. Historical result retrieval cannot restart or renew ended work. |
| A04 credential race | Authored `prepare_start`/`execute_prepared` and MCP post-wait refresh now replace `Access::execute_start`. Carry that mandatory refresh into the remaining adapters. Verify prepared operation key/action/level before storing. Tokens remain request-local; maintenance continues using durable identity, never a retained bearer. | Revoke/rotate key or disable/rebind agent while Prepare is outstanding: no Execute and no use consumed. Mismatched prepared request refuses. A valid current request reaches the real broker. |
| A04 lifetime | Reuse authored `continuation`, `renewal`, `maintenance`, `admission_completion`, broker clocks and Status receipts. Add bounded active-admission query/index as needed. Specify pending/no-ack recovery and retirement only with authenticated no-effect evidence; never invent success on not-found. | Start ack lost, reconnect, restart, expiration, revocation and terminal outcome persist one completion. Authority expiration still terminates native work if renewal/revoke delivery fails. |
| A04 native confinement | Port source scope/sandbox/netproxy under allowed native execution owners. Read-only/workspace-write and network limits require real OS confinement; current remote broker supports only Unsandboxed. No advertised sandbox level until its actual backend passes. | Linux native file/namespace/network/child escape probes, symlink traversal, unavailable confinement and process-group termination; Windows supported unsandboxed lifecycle separately verified. |
| A04 change policy | New `machines/change_policy/` as specified in batch2: bounded compiled rule vocabulary, versioned evidence/tickets/digests, independent reviews, combined internal/external authorization. Expose HTTP/MCP and SSR review pages. | Internal deny cannot be overridden externally; 3s external timeout/malformed response refuses; ticket edits invalidate approval; independent review cannot self-approve. |
| A06 + A07 | Batch3 provider/adapter/model matrix, subscriptions/budget/fallback routing, sealed secrets, signed versioned guide definitions and durable runs in existing connections/workflows owners. Wire all source REST/MCP/forms and seeds. | Source fixture matrix plus real broker guide steps; missing hook never succeeds; restart/lost reply never repeats an uncertain effect; stale edits preserve draft. |
| A09 + A10 | Batch4 registry-by-registry mapping, PDF/DOCX/XLSX operations, bounded search/process sessions and Linux GUI capture/input under existing broker. Generate contracts from Rust. | Every registry input/result/refusal mapped; document reopen and unrelated package-part preservation; atomic failed edits; native X11 probes, Wayland capture-only. |
| A08 + A11 + A12 | Batch5 correlated redacted actions, verified archives before prune, source page/action journeys, installers/config/license and Docker. Finish AgentHub navigation with boards/tasks/agents visible to ordinary users. | Archive tamper/crash/prune probes, actual browser domain actions, fresh and existing-data Docker restart. Hosted storage remains PostgreSQL; SQLite only documented machine-local journal/offline uses. |

The first implementation now splits preparation/admission, selects grants in bounded
pages with original retry affinity, and wires enrolled `operation_start` through live
credential refresh and durable admission. Account-backed starts cannot use configured
local sockets to evade that admission. Existing configured non-account authority stays
separate. Six new tests cover these paths; see the A04 ledger for exact run evidence.
Run and admission-bound Get/Read/Wait are now locally validated below. File/batch/image
calls, operator stop, list/history surfaces and native confinement remain open. Explicit
process heartbeat is implemented with evidence below.

Next execution order within A04: finish unified dispatch, including `run` and original
admission-bound process observation without renewing execution; complete broker file/process
enforcement; native confinement and change policy; finish corresponding UI and tests.
This is one S1 integration branch, not one PR per helper. Remaining batch1 comparisons
and batches3-5 are mandatory before S1 CI/publication.

The authored run/read unit uses a distinct closed `GrantObservation` and remote Observe
request for Get/Read/Wait only. The broker checks the exact stored admission and short
observation deadline, never its execution clock. Hub lookup binds the original account,
principal, enrollment and operation; current `proc.read` policy and current task labels
are evaluated under the board/identity/machine lock order. Expired execution leases do
not fabricate fresh execution authority. MCP refreshes bearer/scopes before and after a
broker wait; `run` uses the same start/read dispatcher. Native tests prove observing
does not prevent expiry and cannot cancel/start work; real MCP tests prove `run`
has no plain-frame bypass and a revoked reader receives no output. See the A04 ledger
for local test evidence and its limits. Input/cancel now use the separate control path below.

Authored and locally validated: input, resize and cancellation use a distinct action-bound control envelope, the
original immutable admission and current `proc.write`/`proc.kill` permission. Agent control
must retain the original live grant/task generation and cannot renew its execution clock or
consume another use. Pending input carries a monotonic permission deadline through the
existing native input queue so an expired queued request cannot be applied later. Preserve
existing writer takeover, size/text limits and explicit uncertain-delivery errors. Reuse
the broker's actual input/resize/cancel implementations; move them out of the capped broker
file rather than duplicate behavior. The legacy Execute envelope must not bypass the new
effect action check. Tests must exercise real native pipe input/cancellation, mismatched
action/admission, expired/revoked permission, stale task and current-policy denial; actual
MCP uses the admitted route and refreshes credentials after preparation waits. Exact local
checks and uncovered probe limits are recorded in the A04 ledger. Operator
stop across principals remains a separately authenticated human-only path; never accept an
agent-supplied operator flag. Do not mark the whole process row closed until that path and
the source heartbeat surface are verified too.

Next selection unit: apply credential project limits before any broker preparation;
resolve each candidate's original board through the existing board registry, then use
`current_in` to check its task lease, policy, machine and live grant without consuming a use.
Skip only explicitly identified ended/expired/stale candidates or policy denials; propagate
storage, malformed-record and identity failures. Recheck the matching candidate after
broker preparation so a released task cannot win during that wait. Retain bounded pages,
one preparation per confinement level, and original retry affinity (a retry never moves
to another grant; a newly project-restricted key refuses before contacting the broker).
Acceptance: real MCP picks a later valid task grant over a released/reclaimed one; a
project-limited key ignores unrelated/unbound grants and refuses an out-of-scope retry;
release during Prepare chooses a still-eligible candidate; database failure never becomes
grant-required fallback. Existing last-use, canonical retry and 101-candidate tests remain
required. No source closure follows from this unit alone.

### Explicit process heartbeat: implemented and locally validated

Source `server/internal/devapi/mcp_proc.go` exposes `proc_heartbeat`; its
`machineops/proc.go::ProcOp` requires the original holder/fence and running lease.
Current immutable admission/broker binding replaces that separate process lease.
Expose `operation_heartbeat(machine, operation_id)` in the existing operation router.
It is account-backed and requires `machines:run`; it cannot mint a grant, select another
one, renew a task lease or revive expired execution. Return only the acknowledged
operation ID and authority deadline, never an execution envelope to the caller.

Extend `continuation.rs`'s existing transactional renewal with an explicit owned-target
check and `proc.heartbeat` authorization using current task labels. Keep the existing
maintenance API and original-action check. The adapter resolves the original board,
asks the original broker for supported levels, refreshes bearer/scope/project limits
after that wait, renews atomically and sends the existing authenticated Refresh frame.
Refresh credentials again before exposing its receipt. The broker's immutable binding
and expired-clock refusal remain the enforcing native boundary. No new scheduler,
local IPC heartbeat or parallel lease table.

Acceptance: actual MCP heartbeat sends only Capabilities/Refresh for the original
admission; no new Execute/use/admission; revoked key during capability wait causes no
Refresh or deadline change; released task, changed policy, foreign/unknown operation,
noninteractive admission and expired authority refuse. A mismatched broker ack fails.
Existing real native refresh/expiry tests and PostgreSQL renewal tests remain required.
This is another local S1 unit; file enforcement and all other source work remain open.

### File dispatch: source-grounded implementation constraints

Read before authoring: S1 `server/internal/devapi/mcp_ops.go`, `mcp_tool.go`,
`machineops/ops.go`, `access/authorize.go`, `bridge/runner/{fs,tool}.go` and
`bridge/tools/{read_file,write_file}.go`. Current owners inspected:
`hub/src/machines/files/mod.rs`, `mcp/callers/start.rs`, `machines/access/{current,
admissions}.rs`, `machine-engine/src/{broker/files,files,files/write,files/manage}.rs`,
`machine-engine/src/journal/grants.rs`, and `mcp/src/{files,image}.rs`.

Concrete corrections to the generic file packet:

- Source `fs_delete` is a real operation, absent from current RequestBody/tools. Include
  a guarded delete contract, native implementation and both local/hub surfaces; recording
  only existing read/write tools would leave this source operation unabsorbed.
- Source `fs_write` requires the current SHA-256 or explicit absence; `fs_move` and
  `fs_delete` require the source SHA-256. Current write guards are optional and move has
  none. Specify these preconditions in the account-backed adapter and enforce them at
  the broker under its existing mutation gate. Failed preconditions change nothing.
  Keep exact-text/count edit semantics. Cover source changes, destination conflicts,
  directory refusal and both source/destination grant boundaries.
- Source authorization permits fs.write grants to cover fs.read. Current selector
  `GrantCapability::covers` already expresses this, but current_in requires capability
  equality and start_candidates queries exact capability. Extend these existing owners
  for file selection; do not fork a second authorization algorithm. Preserve the exact
  fs.read/fs.write policy action and current task/credential checks.
- Current `read_multiple_files` and `read_image` directly call Hub::send. Move every
  underlying read through the authenticated file invocation. Source machine_tool counts
  one invocation against the grant and includes all requested paths. Do not accidentally
  consume a once grant once per image page, or leave a raw image/batch bypass. Preserve
  ordered per-file results, image identity consistency, bounds and credential refresh
  before disclosing bytes. The bounded composite invocation/receipt must be designed
  together with ordinary file admission before implementing the transport adapter.
- Current broker file tools explicitly have **no journal/retry identity**. The existing
  mutation gate serializes broker writers and preserves CAS, but does not provide crash
  recovery or protect against external filesystem races. Do not claim a journal already
  exists for these effects. Any durable admission/receipt belongs in the existing private
  machine journal, not a second scheduler/database or a hosted SQLite fallback. An
  uncertain mutation must never be replayed blindly to recover a lost response.
- Prepare must not call `Scope::target(..., create_dirs=true)` or `make_dirs`: those
  create directories. Canonical preparation is read-only. Recheck the actual source,
  destination and original authority after waiting for the mutation gate, before effect.
  A changed symlink between Prepare/Execute must refuse. Native path-handle/confinement
  probes must also cover races during the operation; lexical/canonical checks alone are
  not evidence of OS confinement. Do not advertise a new sandbox level from this adapter.

Acceptance is the complete ordinary + batch + image + guarded-delete flow, not merely a
helper compiling: valid grants reach native files; no-grant/current-policy/task/key/enrollment
failures produce no effect or disclosure; concurrent CAS has one winner; replay/lost reply
cannot duplicate an uncertain mutation; one composite consumes one use; all source registry
entries have explicit destinations. Native platform and full S1 publication checks remain.

## S2: documentation and retired packaging delta

Read both commits `ab616ee` then `ca626db`. No new Go runtime is added by this delta.
Canonical `docs/devagotchi/README.md` assigns chat page/rooms/routines to Chatmaton's
separate repository. Preserve proposals and their ownership without calling them shipped.

| Source paths (all 17) | Destination/action | Check |
|---|---|---|
| `.gitignore`, `.gitleaksignore` | Compare current root ignores and scanner configuration. Retain precise generated/private-state exclusions; inspect each suppression against its actual non-secret fixture. | `git check-ignore` representative outputs; no broad credential suppression; clean public artifact inspection. |
| Deleted `.goreleaser.yaml`, `.release-please-manifest.json`, `release-please-config.json`, `deploy/docker/{bridge,hub-server}.Dockerfile` | Record intentional retirement of Go-only delivery, map useful packaging behavior to current Rust workflows/deploy files. Do not delete current Rust release files by name. | Source previous blobs plus current artifact commands and license/public configuration coverage. |
| `docs/devagotchi-spec.md`, `docs/devagotchi/27-gap-tracking.md`, `39-chatmaton-gold-build-plan.md` | Reconcile current design index and accepted release decisions; preserve unfinished requirements explicitly. | Every changed requirement has owner/status; no historical completion claim treated as current proof. |
| `39a-research-app-shells.md`, `39b-research-offline-sync.md`, `39c-architecture-on-gold.md` | Preserve research with dated status; map public hub interfaces to existing owners, keep private Gold configuration outside public runtime. | Links resolve; source recommendation remains a proposal unless accepted elsewhere. |
| `39d-p1-chat-page.md`, `39e-p2-approvals-audit-pwa.md`, `39f-p3-assignment-stats.md`, `39g-p4-p5-shells-update-sync.md` | Record chat ownership and exact hub requirements (approval signatures, notification/privacy, updates, sync conflict/authority). Existing hub `notify`, PWA, update and AgentHub owners are comparison targets. Implement any independently accepted missing hub requirement; do not manufacture all planned Go/Tauri files as source runtime parity. | Requirement-by-requirement status, current target/evidence or canonical ownership decision. Docs-only ownership is not runtime implementation. |

Run document/ignore/package checks and required CI for the complete S2 PR; no needless
full local runtime rebuild for an unchanged runtime. Merge/verify/delete before S3.

## S3: enrollment, authenticated transport and browser boundary

Review the five commits in manifest order and apply these groups to S2 main:

| Source group | Current owner and exact change | Required tests |
|---|---|---|
| `bridge/config/{config,enrollment,state_unix,state_windows}*`, `bridge/cli/{enroll,run}.go` | `runtime/link/src/state.rs`, `run/`, current private-file platform owner. Save origin+identity+credential as one generation; private directory/temp file, fsync/atomic replacement; invalid new state never falls back to old token. Source Windows explicitly refuses enrollment, so prove current Rust ACL behavior rather than claiming a source Windows implementation. | Source enrollment interrupted-write/generation tests; Unix modes/symlink refusal; real Windows ACL and replacement tests; malformed state refuses without contacting hub. |
| `bridge/hubclient/{client,transport}*`, `bridge/runner/{grant,ws}*`, `bridge/mcpserve/server.go` | Existing link HTTP/WebSocket and broker grant owners. HTTPS origin or literal loopback HTTP only; no userinfo/path/query/fragment; valid port. Refuse redirects on both HTTP and WS. Exact grant version, no stale live-cache fallback after refresh failure. | Source URL/redirect table including 307/308 credential leakage; TLS hostname/CA; unavailable authority and stale version refuse; reconnect does not widen grants. |
| `server/internal/{auth,browserauth}/`, `config/{config,oidc}*`, `db/migrations/011_browser_identity.sql`, `store/principal_id.go` | Existing hub identity/accounts/browser sign-in and session owners; optional OIDC config in existing config. Stable identity, bounded state/nonce/code flow, atomic consumption, cookie-only browser boundary, current session/role checks during streams; no bearer/proxy-header impersonation. Keep local self-hosted login usable; no mandatory private identity provider. | Translate atomic, boundaries, guard, login, provider, requests, sessions and stream cases. Reject replay, wrong issuer/audience/nonce/host, encoded/unclean paths, CSRF and revoked idle stream. |
| `server/internal/transport/*`, `server/cmd/hub-server/main.go`, `devapi/{build,mcp_access}.go`, `httpapi/server.go` | Hub `serve/`, public routing and machine-access adapters. Optional authenticated edge TLS with dedicated client CA and TLS1.3; startup validates complete config; separate public asset/health allowlist from API auth. | Source startup/mTLS tests: absent/wrong client certificate, invalid config, source namespace bypasses and malformed paths. No diagnostic credential exposure. |
| `web/src/{App.tsx,lib/api.ts,pages/Login.tsx,pages/Shell.tsx}`, static HTML/new JS/deleted JS | Current SSR web/sign-in and site copy. Reproduce login/logout/error/navigation behavior; current build generates its own assets. | Fresh browser login and expired session, inaccessible routes, no stale token in client storage; generated asset references resolve. Do not copy hashed JS. |
| `.gitattributes`, `.gitignore`, `README.md`, `bridge/agentguide/*`, changed docs08/26/27/39c | Existing root attributes/docs and public connect guide. Origin-configurable instructions, actual shell line endings and current truthful support/status. | Executable Unix scripts LF; guide URL/parser tests and public install instructions. |
| `deploy/gold/*`, `server/go.{mod,sum}`, all `test-evidence/*` | Review private edge/service/env/site configuration for portable requirements; keep private deployment data outside public repo. Go dependency metadata and evidence receive provenance dispositions; current Rust dependency/license review owns implementation choices. | Public artifact disclosure check; historical evidence remains dated and cannot count as a current run. |

Close all 79 rows after actual comparison/testing, green merge and main verification.

## S4: packaging, readiness and shutdown

| Source group | Exact work | Acceptance |
|---|---|---|
| `server/internal/health/ready*`, `httpapi/server.go`, browserauth guard/boundaries | New hub `ops/readiness.rs`, existing route registration: bounded real DB query, 1s total timeout, HTTP503/200 with only `{ok}`, no-store; allow health/readiness without exposing API data. Preserve detailed diagnostics separately. | Reachable, unreachable, slow and cancelled PostgreSQL; exact body/cache/auth boundary; no error string leak. |
| `server/internal/lifecycle/http*`, `server/cmd/hub-server/main.go` | Existing `serve/mod.rs`: stop acceptance, drain HTTP in 5s, explicitly close stream/link tasks, then dependencies; preserve actual startup/drain errors. | SIGTERM during finite request and persistent connection; bounded forced close; no detached task uses closed store. |
| `deploy/package/*`, README, packaging docs | Current Rust image/deploy ownership: nonroot runtime, read-only code, intended writable private state, embedded public assets and no private build context. Windows storage-next-packet doc remains explicit source limitation until current implementation proven. | Actual image UID/permissions, fresh-volume owner setup, restart, immutable revision and public image contents. |
| `devapi/build.go`, `httpapi/embed_test.go`, `providers/enums_ts_test.go`, `tests/ui/packaging-smoke.mjs` | Current build metadata, generated provider contracts and browser harness. Assert assets contain current UI, enum contracts agree and fresh browser can login/create/read board/task on desktop/mobile. | Actual built binary/image and responsive browser, no permissive harness retry masking broken auth. |
| All `test-evidence/*` including PNG/JSON | Immutable source provenance only. Preserve referenced requirement/known failure in current tests. | Old screenshot/log cannot close current runtime acceptance. |

All 32 paths accounted before green merge/main verification/deletion, then S5.

## S5: November Rust delta against 6282071

| Source group | Current counterpart and behavior to compare/complete | Required trigger |
|---|---|---|
| `crates/core/{Cargo.toml,src/broker.rs,src/broker/file_mutation_tests.rs,src/journal.rs}`, file-mutation/retry docs and probes | `machine-engine` broker/files/journal and current retry lifecycle. Serialize conflicting effects with durable identity; permanent retired-key tombstones survive history cleanup; no hidden dependency/runtime fallback. | Concurrent same/different-key edits, CAS failure, crash/retry/cleanup, unknown effect does not repeat; reopen journal. |
| `crates/core/src/execution/platform/windows.rs` | Existing Windows execution owner and Job Object/startup tests. | Child tree/reuse/early failure and long-running root lifecycle on Windows, not merely compilation. |
| `crates/hub/src/board/{cli,mcp,mod,provenance,provenance_tests,remote,tests}.rs`, tenant tests, docs agent-board/project-agent-delivery, ticket probe | `agenthub/src/provenance.rs`, hosted board and hub AgentHub CLI/MCP/HTTP owners. Original source/revision provenance is validated and persisted; cannot fabricate or overwrite on retry/update. | Source provenance tests, foreign-account references, changed source revision, CLI/MCP parity, durable PostgreSQL restart. |
| `crates/hub/src/{hub,link}.rs`, `web/landing/popups.rs`, `crates/link/src/session.rs`, `crates/mcp/src/{adapter,capabilities,lib}.rs` | Current machine inspection/capabilities, hub link/session/adapter and documentation. Query actual live broker, bound wait, report unavailable/unsupported truthfully; reconnect updates capability data without broader permission. | Live old/new/faulted broker, malformed/version-mismatched response, timeouts and reconnect; source adapter/hub/protocol probes. |
| `crates/contracts/src/{lib.rs,bin/export-contracts.rs}`, generated desktop types/schema | Current family modules plus generator. Add only missing canonical fields/variants; regenerate outputs together. | Strict round-trip/unknown-field tests, contract generation check, parity across broker and adapters. |
| CI, README, `tests/{adapter_probe,e2e,hub_probe,mcp_protocol}.py`, five verification documents | Port missing assertions into current harness/workflow without weakening existing tests. Retain historical provenance/status. | Run exact translated source trigger on supported platform; CI required checks pass for reviewed tree. |

Account for all 42 rows and the increment-96 base/ref provenance. An unchanged base ref
is not permission to erase unique work. Merge/verify/delete, then combined Docker acceptance.

## Publication dependency and deletion procedure

AgentHub's current Git dependencies point at root `main`, which lacks new `store::receipts`.
Root workspace patches hide this when testing inside the monorepo. Therefore:

1. Finish S1 and validate root/shared libraries plus AgentHub locally. Record exact heads.
2. Run the repository's push policy check with real validation evidence. Publish the
   completed root integration commit and the required AgentHub integration history; branch
   pushes currently do not dispatch CI, but recheck workflows before publishing.
3. Pin AgentHub's root Git dependencies (including testing store dependency) to that exact
   published root commit. Use a D: standalone AgentHub checkout outside root workspace patches;
   resolve/lock and run its required PostgreSQL checks there. No future-root/gitlink cycle.
4. Publish validated AgentHub head, PR, green required CI, merge under existing user
   authorization/policy gate, verify remote main tree and main checks. Retain pinned root
   commit ancestry in root merge; do not squash away its provenance.
5. Update parent gitlink to actual AgentHub merge SHA. Validate affected integration and
   required root gates; publish root PR; green reviewed-head checks; merge; verify main.
6. Record complete source file/commit dispositions with merged target SHAs and test receipts.
   For per-source closure, checker must enforce every row/commit of the selected source;
   final default closure still enforces all five. No excluded open row is silently closed.
7. Archive source Git history durably (tag/bundle with verified objects) before deleting
   eligible historical refs. Delete only refs whose current SHA matches reviewed source and
   whose complete behavior is closed. Preserve dirty/uncommitted source files. Verify deletion.
8. Start next source from verified main. End with fresh ref inventory of all four repositories,
   required main CI and exact-main-image fresh/existing-data Docker acceptance.

Planning coverage, implementation, local validation, publication, PR, main and deployment
are separate states. This document establishes execution instructions, not completion.
