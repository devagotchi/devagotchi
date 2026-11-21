# Preserved-source execution instructions

Authority: [complete absorption plan](../absorb-preserved-sources.md). These are local
work units on the oldest source's branch, not separate PRs. No CI-triggering pushes
until S1-S5 are authored. The file manifests remain the coverage ledger; this document
does not mark their rows implemented. All paths below are relative to the Rust root
unless explicitly prefixed `source:`. A proposed path is an implementation destination,
not a claim that a file already exists.

## Common execution contract

For each packet, read every assigned source row, its callers, tests and schemas. Compare
the behavior to current code. Populate `target_paths` with exact files (not a future
milestone), and evidence with the behavior and relevant test/probe. Keep uncovered rows
open. Dynamically registered routes/tools must be expanded from the dispatch lists.
Record later-source replacements only when that later implementation is verified.

Write domain code, surface adapters, migrations, refusal paths and focused tests together.
Use the existing PostgreSQL transaction owner, authenticated identity and audit chain.
Allocate random identifiers before retryable transactions; never execute external effects
inside a retried transaction. Persist an outbox intent where a durable external effect is
needed. Every mutable configuration uses optimistic versions; stale edits retain drafts.
Browser actions require a live person and same-origin protection. Human-only operations
cannot be enabled for an agent by a configurable role. Secrets never enter audit payloads,
replay responses, screenshots or committed fixtures.

The named tests below are tests to author or locate and verify, not results. Run only
focused local checks during authoring. Record exact commands, heads and outcomes privately.
Full validation and publication belong to R01, after every source is authored.

## Code plan reconciled on 2026-11-21

This section fixes the execution order and integration seams for the packets below. It
is part of this plan, not a second backlog. `existing` means the named source was inspected;
`new` means the destination must be authored. Neither means source parity is accepted.
Reconciliation began at root checkpoint `ed92ea9`, with AgentHub `7588df2`. A01-A03 have
substantial local implementation and focused evidence in their review ledgers; they are
not complete. A04-A12 and the later deltas have not acquired implementation evidence from
this planning pass. No runtime build or CI was run during this reconciliation.

### Complete authoring batches

| Order | Batch / accounting packets | Finish together before the batch check |
|---|---|---|
| 1 | Coordination completion: remaining A01-A03 | Board/configuration deletion rules, environment roster and role membership, remaining REST/MCP administration, reconnecting event stream, protocol/connect output, shared authority and receipt tests, browser reachability |
| 2 | Catalogs and enforced machine access: A05 + A04 | Catalog schemas/seeds and consumers first; request/narrow/approve/deny/revoke, broker enforcement and sandboxing, policy evaluation/change evidence/reviews, machine/approval UI and all adapters |
| 3 | Providers and executable guides: A06 + A07 | Provider/model data, subscriptions and routing, sealed secrets, immutable guide publications, durable runs and actual broker effects, full editors/run UI |
| 4 | Machine tool parity: A09 + A10 | Every registry operation, PDF/DOCX/XLSX, bounded search/process behavior, Linux GUI sessions/capture/input, generated contracts and both MCP faces |
| 5 | Audit and product integration: A08 + A11 + A12 | Correlated actions, verified archives before retention, all page/action journeys, source-derived settings/install/docs, self-hosted Docker configuration |
| 6 | Later source deltas: B01, then C01, then D01, then E01 | Review and author each complete delta in that order, preserving separate source-boundary commits; no CI between tiny changes |
| 7 | R01 | Final integration validation, oldest-source green delivery, verified branch deletion, actual-main Docker acceptance |

A05 is not dependent on an already finished A04: source `access/check.go` reads capability
schemas and `access/decide.go` reads grant presets. They form one batch with catalogs first.
A08 follows all event producers so retention is not built against an incomplete event model.
Within each batch, author domain, adapters, UI, refusal paths and tests before its batch
check. A complete transport/domain unit may have one focused regression check after its
code and tests are authored; do not compile after each helper edit. Compilation is not a
planning activity. A successful sub-unit test does not finish
the batch or authorize a push. Native-platform acceptance remains in R01 where unavailable
locally; its tests and fixtures must nevertheless be authored in the owning batch.

### Batch 1: complete the existing coordination path

| Source obligation / current seam | Code to author or reconcile | Acceptance at the batch boundary |
|---|---|---|
| `store` project/config removal; HTTP `admin.go` dynamically registers roles, policies, workflows, projects, settings | Existing AgentHub `hosted/registry.rs`, `hosted/settings.rs`, `hosted/shared_workflows/` and hub `agenthub/registry.rs`, `agenthub/mcp/boards.rs`: locate each actual owner, add missing guarded deletion there. In-use workflows/states/roles refuse; deleting a project cannot orphan tasks/history. Add new `agenthub/http/configuration/` adapters after those domain rules. Settings dispatch by owning domain, never table-name SQL from a request. | Used/deleted/recreated IDs, stale versions, dependent rows, concurrent delete/create, foreign account and replay after deletion; all surfaces see the same result. |
| `store/principals.go`, `admin_people.go`, human tokens, role membership; existing `identity/accounts/agents/{mod,configuration,bindings,mutations,directory}.rs` | Complete environment roster and members through existing account/profile owners; add new `agenthub/http/principals/` and corresponding MCP administration using live manager checks. Tokens map to existing API keys: issue once, metadata only thereafter, revoke through existing receipt-aware operation. An agent cannot become a person by editing its kind. Source human token login maps explicitly to current password/passkey/provider browser login; do not introduce shared human bearer login. | Disabled account/profile and revoked key/OAuth binding refuse reads and writes; member changes take effect without restart. Test owner, ordinary person, bound agent, foreign account and stale role/policy snapshots. |
| `httpapi/events.go`, `stream/broker.go`; existing `agenthub/http/events.rs`, `Board::changes`, `Hub::run_scoped` | New `agenthub/http/stream.rs` plus small stream-state/auth modules as needed. GET `/api/v1/stream` requires project. Subscribe before history catch-up, cursor from `Last-Event-ID` or query, reject conflicting/invalid cursors, batch at most 100, emit committed event sequence as SSE id. Reuse identity bearer/session verification on every bounded read/emission, not only handshake. Recheck idle authority; close on revocation, changed identity, inaccessible board or store failure. No unbounded channel or detached per-client task. | Real HTTP stream: backlog and live events in order, resume without gaps, no cross-board events, no synthetic receipt-replay events, key/project/profile/session/grant revocation while open, malformed cursor, slow-reader/drop cleanup. |
| `httpapi/protocol.go`, client instructions and current connector ownership | New `agenthub/http/protocol.rs` delegates connection configuration to `crates/connectors`, the owner already used by `web/connectors.rs`. `identity/connect` is Nomen bootstrap, not client snippets. Use current operation names, explicit project/fence/version/retry rules; derive origin from configured public origin, never arbitrary Host input. No token interpolation or stored-secret return. | Output references existing commands/tools and valid origin; caller display names never enter shell/config syntax. Test structured connector output against the existing catalog and its parsed-snippet fixtures. |
| Source operation filters, dependency readiness, defaults/reaper behavior, browser pages | Extend A01's operation mapping and A02's roster/policy mapping with exact target and tests; preserve authored receipt/CLI/HTTP implementations. Reconcile source defaults against current policy/workflow versions; seed only missing definitions. Existing `web/app/home.rs` and AgentHub pages expose the completed operations. | Complete source operation matrix plus real PostgreSQL edge cases; no duplicate effect or lost newer lease. Roster/configuration operations are reachable by an owner in the browser. |

Further source comparison: `store/admin_misc.go::DeleteConfig` refuses a project with
ANY referenced task, including completed tasks. Rust `hosted/mod.rs::scope` locks the
project row, and migration `0001_boards.sql` makes history append-only with project foreign
keys. Removal must use that same lock and retain audit/receipt history; never cascade-delete
history to make a DELETE route work. Current hub board creation also seeds a welcome task
(`agenthub/mcp/boards.rs::make_owned`); it counts as a task under this rule. Empty-project
creation/deletion needs a real source-derived test, not an assumption that a newly bound
board has zero tasks. Cached handles must fail after removal, and name reuse must not revive
old grants or make retired request keys reusable. The subsequent domain and browser/HTTP
implementation is recorded in [a03-projects.md](a03-projects.md).

Batch 1 implementation order: stream and protocol as one complete transport unit; then
configuration domain gaps and administration as one complete unit; then roster and remaining
source behavior comparison. These are local coding units, not separate PRs. Keep A03 open
until all units and the complete batch checks pass. Do not rerun the already passed CLI
suite unless a shared receipt/identity change invalidates it.

Principal administration source dispatch was expanded during authoring: S1 has HTTP
principal/token/role/policy administration, not corresponding MCP admin registrations.
The concrete authored routes, roster and existing-domain mapping are recorded in
[a03-principals.md](a03-principals.md). Do not add speculative MCP administration for
that source; inspect the actual Devagotchi Extra registries for their separate packets.

Project lifecycle implementation detail (authored; focused evidence in [a03-projects.md](a03-projects.md), full batch acceptance remains open):

1. Extend `board_projects` with a retired timestamp, display name and positive version.
   Retain the row and every history/receipt record. Make the owner/name uniqueness rule
   apply only to active projects. A later project using that name gets a different physical
   board ID; never revive the retired row or reset its fence counter.
2. Factor the existing initialization transaction into reusable in-transaction helpers.
   Creation must commit defaults, optional shared-workflow binding, project metadata,
   receipt and event together. Do not initialize a board first and save its metadata in
   another transaction. Explicit project creation creates an empty project; the existing
   onboarding bind path retains its welcome task. Both use the same registry and limits.
3. Resolve active owner/name to the physical board ID in `agenthub/registry.rs` through
   an AgentHub-owned query. Lock the namespace before catalog and project rows, matching
   existing shared workflow locking. Namespace races must select one created board rather
   than return a duplicate error for an idempotent bind. Existing directly constructed
   physical handles must never be repointed at a new generation.
4. `HostedBoard::scope` refuses retired projects, including reads through cached handles.
   `Boards::open_account` validates cached generations before returning them and evicts
   only the stale entry it checked. Active registry lists and plan limits exclude retired
   projects. Audit export retains retired rows and their stable identities.
5. Retirement is a human account-owner operation. In the same transaction, check current
   identity/policy, version and all tasks (completed tasks count). Refuse when the project
   is the retained audit anchor of a live shared definition. Remove its own workflow
   binding only after these checks. Record the retirement event and receipt before setting
   the tombstone; no cascading history deletion.
6. Account-owned project removal must also refuse active explicitly project-scoped API
   keys naming it, through the host identity directory on the same transaction. This
   prevents reusing a name from reviving a narrower old credential grant. Account-wide
   credentials already intentionally cover future projects. Shared/configured/org boards
   retain their existing lifecycle; an account route cannot retire those namespaces.
7. A deletion retry may resolve the last retired generation when no active one exists;
   current authority must precede receipt replay. With a newly created generation, an old
   key must conflict rather than retire the new project. Creation/deletion keys are never
   reusable after retirement. Explicit owner-only history access must keep the retired
   generation identifiable without reopening it for writes.
8. Author domain plus HTTP/browser tests together: empty create/delete/recreate, stale
   handle/write, request replay, version mismatch, active-key/catalog/task in-use guards,
   completed-task guard, concurrent create/delete, unchanged history and distinct new
   identity, foreign account and policy refusal. Then one focused lifecycle check.

Aggregate configuration implementation detail:

1. Add `/api/v1/config?project=NAME` in the existing HTTP configuration module, composing
   the current account-owned board configuration/catalog, owned project listing and
   environment-owner principal configuration/roster. Require a live person and reject
   seats, agent keys, foreign boards and org/shared namespaces. Do not introduce another
   config store or reproduce the source's ignored read errors.
2. Keep source section names workflows/projects/roles/principals/settings/policies and
   state_kinds, using current typed records. Distinguish board versus principal settings
   and their versions. Page potentially large project/principal lists at 100 with explicit
   independent continuation cursors; bounded roles/rules/catalog use their existing limits.
   Reads are independently versioned snapshots, not one claimed atomic cross-owner state.
3. Test real authenticated HTTP against individual owner routes, missing project and bad
   cursor refusal, no partial success on a failed read, person/agent/seat scope and ordinary
   user refusal for environment administration. Record the prerelease API differences.

Task selection comparison (read S1 `store/{tasks,claim,sort}.go`):

1. Ordinary task lists sort by priority/creation/id. Assignment preference belongs to
   actor-specific ready/claim selection; change `hosted/reads.rs` to select that order
   only for those calls, preserving existing assignment tests.
2. `ClaimNext` source SQL filters requested states and all normalized labels before its
   25-candidate limit. Current Rust state filtering occurs after the limit and supports
   one label only. Pass the complete typed selector into `list_in`; constrain state and
   label intersection before limiting, then retain post-expiry state/policy checks.
3. Add `labels` to NextFilter and typed claim input, preserving singular `label`. Thread
   through HTTP, MCP, local and remote CLI; repeated CLI `--label` means intersection.
   Omit an empty added field from canonical receipt serialization so old single-label
   retry identities remain stable. No handwritten generated schemas.
4. Test >25 earlier nonmatching states/labels, ordinary versus assignment ordering,
   all-label normalization, HTTP/MCP and CLI selection/replay. Preserve deliberate Rust
   literal-wildcard search and expired-lease lazy recovery; document source differences.

### Batch 2: catalogs, grants, policy and machine enforcement

Catalog authoring and evidence: [a05-catalog.md](a05-catalog.md). Access source comparison
and concrete next steps: [a04-access.md](a04-access.md). Both packets remain open until
their actual consumers/enforcement and source closure are verified.

Existing seams inspected: `Hub::call` in `hub/src/mcp/hub.rs` sets caller and routes/audits;
`machines/link/calls.rs` has a bounded pending-call queue; `machine-engine/src/broker/access.rs`
protects operation ownership/writer leases. These do not implement source grant selectors,
use counts, expiry, narrowing or sandbox levels. `broker.rs` explicitly says executable
grants are not a sandbox. Preserve those checks and add the missing enforced layer.

1. Add `hub/src/catalog/{mod,kinds,validate,stored,seed,http,mcp}.rs` (new) and its own
   PostgreSQL migration through the existing store bootstrap. Allocate migration numbers
   from the actual migration list at authoring time. Every table uses the existing environment
   scope/RLS; unique IDs include that scope. CAS writes, audit and replay finish in one
   transaction. Kind dispatch is a closed enum with typed columns, not arbitrary SQL.
2. Port the eight self-hosted catalog kinds from `catalog/kinds.go`: capabilities, risks,
   capability-risks, shell-profiles, grant-presets, docker-runtime-presets, router-endpoints,
   chat-apps. Record the ninth, `tenants`, as replaced by the already accepted one-environment
   edition boundary, not a customer-management page. Provider rows keep their own validator.
   Validate the merged row on PATCH: `full_access` requires `requires_policy=unsandboxed`;
   referenced capabilities, levels, risk IDs and selector schemas must exist. Refuse deletion
   of referenced definitions. Seed source JSON without overwriting edited values.
3. New `hub/src/machines/access/{mod,requests,decisions,grants,scope,resume,stored,http,mcp}.rs`:
   versioned access requests, grants, unsandboxed policies and resume alerts. Persist principal,
   machine, capability, selector, sandbox level, duration, uses, task lease/fence or guide run,
   approver, immutable risk snapshot and ending reason. Human approval cannot approve its own
   request. Narrowing can only reduce paths/hosts/argv/apps, duration, use limit and sandbox
   privilege. `once` means one admitted operation, not one transport attempt.
4. New contract family `contracts/src/grants.rs`: typed effective authority attached to a
   routed call, bound to machine, canonical principal, request identity, grant version and
   short expiry. Wire extension must be capability-negotiated in existing link hello/session
   code; keep protocol version 1. A peer without enforcement support cannot receive a call
   requiring it. Local trusted IPC authority is distinct from hub-supplied authority.
5. Extend `mcp/hub.rs` dispatch and `machine-engine/src/broker.rs`; new broker `grants/`
   owns selector validation and active-operation grant membership. Reauthorize before
   admission and before subsequent read/input/cancel operations as appropriate. Consume uses
   atomically with a durable admission identity, not again on retries. Existing journal and
   retry owner still own external-effect deduplication. Disconnect/reconnect must not retain
   unchecked authority; revocation terminates affected process/GUI grants and is reconciled
   from durable state, with expiry enforced even if notification is lost.
6. Port source `bridge/scope`, `policy`, `sandbox`, `netproxy` behavior into existing execution
   platform owners. Read-only/workspace-write/desktop/unsandboxed must describe actual OS
   enforcement. Linux implementation and real probes precede enabling a sandbox level.
   A host with no working sandbox refuses requests requiring it; do not rename executable
   allowlisting as sandboxing. Preserve network host restrictions and path/link escape refusal.
7. New `hub/src/machines/change_policy/` with `document`, `evaluate`, `facts`, `tickets`,
   `reviews`, `external`, `http`, `mcp` modules. Source `policyrules/types.go` fixes the rule
   vocabulary: classifiers/regions; event/tool/class/region/command/runner matching; deny,
   require, allow, audit, collect effects; ticket/field/list/number/approval/review/runner
   checks. Parse/compile bounded expressions when saving, not during every invocation.
   Store immutable policy versions, decision evidence, ticket content digest and independent
   reviews. Edits invalidate old approvals. The optional external `/decide` client has a
   three-second timeout, bounded body and fail-closed malformed/unavailable handling;
   effective permission requires BOTH internal rules and configured external approval.
8. Add server-rendered `web/app/access/`, `web/app/catalog/`, `web/app/change_policy/` pages
   (new), linking existing machine detail and approvals inbox. Preserve source REST families
   `/access-requests`, `/grants`, `/sandbox-policies`, `/resume-alerts`, `/access-summary`,
   `/policy/*`, `/change-tickets` under `/api/v1`; expand dynamic catalog routes from the eight
   kinds. HTTP/MCP/browser call one domain. No approved-looking UI for a declined broker call.

Batch acceptance: concurrent one-use grant, narrower approval, own-request refusal, lease
change, revocation/expiry during running process, reconnect with stale grant, missing sandbox,
path/network escape, internal deny/external allow, external timeout, changed-ticket review,
catalog invalid cross-reference and edited-seed reopen. Real process effects and refusal at
the broker must be observed, not only HTTP status. Preserve original process outcome if an
audit write subsequently fails and expose the existing audit-failure alert.

### Batch 3: providers, subscriptions and guides

`connections/stored.rs` currently stores service/name/sealed key/hint; it is not the source
provider matrix. `workflows/stored/mod.rs` stores onboarding steps and browser participation;
it is not the editable guide interpreter. Extend these owners without changing onboarding's
existing meaning or adding another process executor.

| Unit | Concrete implementation | Required integration/failure behavior |
|---|---|---|
| Provider data | New `connections/providers/{types,validate,stored,seed,http,mcp}.rs`; source `providers/types.go`, adapters/enums/checks and seeds. Store schema version, vendor/product, auth kinds, runner/editions, primary/fallback adapters, model source/cache, usage basis, session-state source/parser, OS detection rules and nine explicit capability flags. | Reject unsupported schema versions and invalid adapter config; preserve null versus false capabilities. Detect via bounded broker file/version/status operations under grants. Model refresh is explicit/bounded, and stale/error status stays visible. |
| Subscriptions and routing | New `connections/subscriptions/{stored,secrets,bindings,resolve,call,http,mcp}.rs`; source `subs/*.go`. Versioned owner/provider/kind/worker/machine/budget/enabled rows and ordered primary/fallback role bindings. Reuse `identity/sealed.rs`; return hint/fingerprint only. | `cli_login` requires an enabled agent worker and machine; never stores a consumer token. `api_key` requires compatible enabled provider and live secret. Route checks enabled, budget and actual online evidence; returns skipped reasons. Retry does not double-charge or duplicate provider calls. Uncertain external outcome is recorded as uncertain, not repeated blindly. |
| Guide definition | New `workflows/guides/{types,validate,lint,render,diff,publish,import,stored}.rs`. Port `guidekit/step.go` shape, flow/refs, OS command refs, published versions and signed bundles. Store draft version/CAS, immutable publication hash, signer/key ID and update decisions. | Validate duplicate/missing step IDs, branches, retry bounds, command refs and links before publication. Unsigned/untrusted/tampered bundle refuses. Seed updates never overwrite an edited guide. Import/export and diff use the actual immutable content. |
| Guide run | New `workflows/guides/{runs,step,dispatch,reconcile,http,mcp}.rs`; persisted run/version/current step/vars/retries/log/status plus durable step intent and operation ID. Use existing broker process handles and A04 grants. | Operator/confirm/resume requires a live person. Persist intent before external effect, reconcile same operation ID after crash, then CAS advance. Missing machine/hook cannot take the source `runstep.go` default-success path. Concurrent submit, lost reply, pause/retry/cancel/restart never execute a step twice. |
| Browser and surfaces | New `web/app/providers/`, `subscriptions/`, `guides/` reachable from current connections/machine/AgentHub pages. Source REST provider/schema/model, subscription/secret/test/binding/route, guide/version/publish/retire/diff/import/update/run/step families and MCP use the same domains. | Draft retention on validation/stale-version error; secrets never returned in edit forms; real connection test/run status; human prompt and rejected approval visible; read-only users cannot mutate through direct routes. |

Acceptance: source provider/adapter fixtures; disabled provider, stale version, bad secret,
offline worker, exhausted budget, fallback selection; signed guide edit/publish/import;
real granted process step and human step through restart/cancel; audit and receipt identity
for each surface. A screenshot or fake hook result does not establish execution parity.

### Batch 4: source tool registry and GUI

Expand every entry of `bridge/tools/registry.go` before closing A09. Current operation,
file, search and system families are existing destinations; source naming may map to a
documented current equivalent, but its inputs/results/refusals need source-specific checks.

| Source registry group | Destination and remaining contract |
|---|---|
| get/set config; usage/recent calls/feedback | Existing broker settings/history and adapter system tools; reconcile editable knobs and redacted history, and explicitly map feedback to its owned persisted destination. Do not silently drop an operation. |
| read file/multiple; write file/PDF; edit block; info; create/list/move/delete | Existing `machine-engine/src/files/{given,lines,write,manage}.rs`, broker files and contracts files. New `files/formats/{mod,pdf,docx,xlsx,package}.rs` and contract document family. Read PDF page ranges/text and bounded images; write markdown PDF and insert/delete pages into a DIFFERENT output. DOCX markdown creation, outline at offset zero and exact XML replacement including header/footer fallback. XLSX grid/sheet/range read, grid creation, range edit preserving other cells, styles, formulas and unrelated package parts. |
| start/more/stop/list search | Existing `files/search.rs`, contracts search and both adapters; add owned bounded search sessions if existing stateless cursors cannot express stop/list/source behavior. Bind session to caller/scope/query, cap lifetime/results, preserve ignore rules and cancellation. |
| start/read/interact/terminate/session/process list/kill; run command/code | Existing broker operation lifecycle, interactive contracts, execution platform and launch modules. Map source timeouts/cursors/buffer limits/kill-tree semantics; retain stdout/stderr separation for structured client protocols. Do not create a second process runner. |
| GUI session/observe/screenshot/input/tree/window/app | New `contracts/src/gui.rs`, broker `gui/`, hub `machines/gui/`, local adapter GUI tools and `web/app/gui/`; Linux backend under approved `execution/platform/linux/`. X11 capture/input/window/app and panic stop, Wayland capture-only as the source implements, useful unsupported results elsewhere. No unimplemented Windows/macOS GUI claim. |

All binary/document operations use the existing file-mutation serialization, revision/CAS
and durable retry owner. Read inputs and nested PDF/image paths under the same grant, cap
compressed AND expanded data/entry count/depth, reject encrypted/invalid input, preserve
original on any failure, fsync and atomically replace only a fully validated output.
Do not rebuild a workbook to change one range if that loses unrelated package content.
GUI authority binds session/display/apps and requires observe authority for input; recheck
grant and displayed-frame geometry before input, bound action count, and end on panic stop,
disconnect/expiry/revocation. Screenshots never become default audit payloads.

Dependency choices during planning: use `jsonschema` without default network/file resolution
for A05 and an offline retriever; compile/cache bounded schemas by hash/version. Official
[validation options](https://docs.rs/jsonschema/latest/jsonschema/struct.ValidationOptions.html)
document offline refusal. For document work, `lopdf` is the candidate Rust PDF owner
([upstream operations](https://github.com/J-F-Liu/lopdf/blob/main/pdfutil/README.md)); package
editing needs raw-copy preservation ([zip raw-copy implementation](https://docs.rs/zip/latest/src/zip/write.rs.html)).
These are design choices, not proof of compatibility: pin a repository-compatible version,
inspect license/MSRV/dependency effects, then run the source round-trip and malformed-file
fixtures before accepting the dependency. Keep parsing offline; no new MCP HTTP client.

Acceptance: every registry entry has an input/result/error mapping and source-derived test;
document reopen confirms actual contents and untouched parts, failed edits leave original
hash unchanged, concurrent edits/retries do not overwrite newer content. Both MCP faces
and generated schemas agree. Linux real GUI probes prove the supported backend; Windows
process tests prove Job Object and terminal behavior separately.

### Batch 5: audit, reachable product and packaging

1. Extend existing `ops/audit/{mod,history,read,completion}.rs` with correlated action
   records (request, task, operation, guide run, grant, category, status), bounded redaction
   and source action projections. Keep core append-only audit distinct from optional output
   capture. New `ops/audit/retention/` owns versioned category settings, scheduled/manual
   flush/prune and manifest verification; a PostgreSQL migration owns checkpoints/segments.
2. Port source category retain-days/max-rows/max-bytes/archive/capture-output/max-output;
   zero limits mean unlimited, not delete-all. Archive gzip JSONL in bounded segments with
   row chain, SHA256, previous manifest hash, optional Ed25519 signature/key ID. Serialize
   each category's archival claim across hub instances; persist progress, write/flush/
   verify/atomically publish archive and manifest, then record segment and only then prune
   eligible rows. A crash between filesystem and DB commit must reconcile the same segment.
   Missing/corrupt sink or signature never permits pruning. Retain chain anchors and A03
   retired-key tombstones forever; clearing receipt payloads cannot make a request reusable.
3. Add `web/app/audit/` pages using the current owner-role machinery, source `/actions`,
   `/actions/a2ui`, `/audit/retention`, category edit, flush/prune/download/verify operations.
   Historical output is private by default; redaction precedes capture and archive writing.
4. Finish A11 by enumerating `web/src/App.tsx` and every page/component's action. Record
   source page -> Rust route -> visible navigation -> domain mutation/read -> acceptance.
   Existing `web/app/mod.rs`, `home.rs`, AgentHub board/task/config/profile pages remain the
   shell. Add grants, change review, catalogs, providers, subscriptions, guides, GUI, actions
   and retention links with plain-language labels and empty/error/conflict states. No dead
   placeholder cards. AgentHub boards/tasks/agents must be discoverable on the main screen.
5. A12 maps source installers/service/config to existing Rust join/install/deploy owners;
   no parallel Go daemon. Review every source setting against its runtime consumer and UI.
   Public defaults contain no private hostnames/tokens/development passwords. Keep private
   development bootstrap intact. Audit all SQLite callsites and justify only broker-local
   durable/offline storage; hosted state remains PostgreSQL. Preserve notices and source
   provenance, and update support claims only from implemented paths.

Acceptance: concurrent archive writers, sink failure, tampering, restart and prune boundary;
chain verification after retention; owner/ordinary-user browser journeys at 390/1280,
keyboard and JS-off critical forms; source machine/document actions through actual broker;
fresh and existing-data Docker runs, restart persistence and credential revocation. A11
cannot be closed by merely finding HTML route handlers.

### Batch 6: exact later deltas, oldest first

| Source | Grounded change and code owners | Closure evidence |
|---|---|---|
| S2 / B01, 17 paths | Two commits: Chatmaton design documents, then ignore/packaging changes. No new runtime room implementation in this delta. Map docs 39d-g to accepted application ownership in `docs/product-and-platform-plan.md`; preserve requirements and status. Existing hub workflows/approvals, `notify/push/`, `site/pwa.rs`, link update and connector owners supply hub interfaces; do not manufacture a Go chat sidecar from a proposal. Inspect deleted Go release/Docker files against current Rust equivalents, and each secret-ignore entry against its fixture. | Each path gets exact disposition; accepted missing hub behavior remains open and is authored before S2 checkpoint. Private Gold/APNs/FCM/signing-host proposals are not silently represented as shipped self-hosted code. |
| S3 / C01, 79 paths / five commits | `bridge/config/enrollment*`, transport/grant changes -> `link/src/{state,run,session}`, machine enrollment and A04 grants. Browser auth/OIDC/TLS changes -> existing hub identity, sign-in/session, provider and serve owners. New TLS configuration stays with transport, private state ACLs with existing platform owner. Source tests define one-use enrollment, atomic/private storage, dedicated CA+hostname checks, redirect refusal, cookie/session/stream boundaries. | Translate each changed test, including Windows interrupted state writes and cached-grant revocation. New public agent guide is origin-configurable; private Gold deployment files remain outside public runtime. Source line-ending changes map to actual Unix artifacts. |
| S4 / D01, 32 paths | Source `health/ready.go` checks dependency in 1s with coarse `{ok}`/no-store; existing `ops/health.rs` is a detailed diagnostic, not that endpoint. Add `ops/readiness.rs` and `/readyz` using real DB query with a 1s total bound, keep `/healthz` diagnostics. `serve/mod.rs` graceful shutdown needs source 5s drain and explicit stream/link lifecycle before closing dependencies. Current deploy image gets nonroot/read-only code and writable private-state checks. | Real unreachable/slow PostgreSQL readiness, SIGTERM during request and persistent stream, embed/static-resource and browser auth regression. Historical screenshots/logs are source evidence only. |
| S5 / E01, 42 paths | Current owners already include machine-engine broker tests for capabilities/retry/file_mutations, approved Windows execution, AgentHub `provenance.rs`, MCP capability helper and link session. Compare every hunk from `62820713747ba3b2c572e5642a8bbe8e779d769f` to preserved `c936f7f`; map old core/board paths to these real owners. Regenerate generated contracts rather than copy source output. | Reproduce the source concurrency/retry/provenance/live-capability/Windows-startup triggers. A renamed module or prior PR title is not equivalence. Record tested head before closing each row. |

No row is closed merely by this plan. A00 still requires every preserved commit/file,
including removed behavior and newly discovered refs, to acquire its own disposition.
Planning has selected destinations and complete batches; runtime-specific API/dependency
details still get checked against source as each batch is authored, without reopening the
overall product architecture or turning each helper into a new CI cycle.

## A00 — immutable source and behavior accounting

Inputs: all five manifests and their exact Git objects; the original dirty checkouts
remain read-only. S1 documentation, schemas, seeds and test evidence contain requirements
for other packets even when their primary accounting owner is A00.

1. Verify file mode/blob/path against `git ls-tree` and the delta against `git diff
   --no-renames`; verify the commit sequence against `git rev-list --reverse`. Preserve
   deleted files using the previous tree's blob. Never generate over manual dispositions.
2. Read the 91 S1 commits in ancestry order. Add each runtime requirement to the owning
   packet and row. Inspect changes removed before the final tree: classify intentional
   source supersession using the replacing commit, not simply absence from the head.
3. Audit live local/remote refs in both source repositories and both canonical repos.
   Append any unique source work not represented by these five heads; do not assume a
   previously written inventory is current. Keep release tags and source archives separate
   from unfinished development branches.
4. Reconcile stale 'absorbed' claims in the plan registry and design index. Keep historical
   evidence dated; distinguish implemented, tested and merged. Do not delete old designs to
   make the remaining work look smaller.
5. Record decisions required by the current product: Rust replaces the Go runtime; hosted
   storage is PostgreSQL; the self-hosted environment has users, not customer organizations;
   source-supported Linux GUI behavior is included. Existing production-readiness claims
   remain unconfirmed. Commercial-only implementation stays outside the public repository.

Acceptance: the inventory checker verifies 100 commits/907 file-change rows at the recorded
heads, immutable fields and valid packet references. Coverage remains explicitly incomplete
until every row and every commit has source-specific disposition evidence.

## A01 — tasks, workflows, configuration and presets

Active comparison and local authoring: [A01 review ledger](a01-review.md).

Source: `server/internal/store/` task, dependency, note, lease, settings and workflow
implementations; `server/internal/presets/`; corresponding source HTTP/MCP/CLI tests and
`web/src/pages/Board.tsx`, `NewTask.tsx`. Principals are A02; replay is A03.

Existing owners: `crates/agenthub/src/hosted/`, its transaction/graph owners and tests;
`crates/hub/src/agenthub/`; `crates/hub/src/web/app/agenthub/` and `home.rs`.

1. Compare each source task field, filter, transition, dependency mutation, comment and
   lease operation with the current API and serialized contract. Produce a per-operation
   mapping; do not rewrite already accepted creation/search/history/shared-workflow code.
2. Fill missing domain behavior in AgentHub, then expose it through the existing MCP,
   CLI and browser adapters. Preserve version/fence checks, completion evidence, graph
   cycle rejection, atomic chain/outbox writes and database-clock lease expiry.
3. Compare seeded states/settings/presets with source data. Port useful data as versioned
   seeds through the owning domain, without overwriting user edits on reopen. Binding a
   shared workflow must validate existing tasks and preserve board-specific authority.
4. Test removal/renaming of used states, dependencies completing/reopening, stale edits,
   simultaneous claims, expired leases and zero/unlimited limits. Include read-only and
   foreign-account requests and storage failures. Do not equate a board alias with a
   registered credential-bound principal; A02 supplies that identity.

Acceptance: every A01 source operation has an implemented equivalent and focused evidence;
browser/MCP/CLI observe the same persisted task/configuration state. Existing PR receipts
can be reused only for behavior and code unchanged since their tested heads.

## A02 — principals, credentials, roster, roles and policy

Source: `store/principals.go`, `admin_people.go`, `lease.go`, `policy/policy.go`, principal
schemas and Agents/AgentEditor browser components. Full packet includes OAuth, kinds,
roles and policy; the earlier API-key-only plan is superseded.

Existing owners: hub `identity/accounts/`, `identity/auth.rs`, `identity/bearer.rs`, OAuth
verification; hub `agenthub/mcp/{calls,access,notices}.rs`; AgentHub `hosted/registry.rs`
and lease transactions. New profile code: `identity/accounts/agents/`. New pages:
`web/app/agenthub/agents/` and `policies/`, with site-owned copy.

1. Add identity-owned `auth_agents` migration: tenant/account/id identity, immutable id,
   display name, enabled, kind, roles, nullable WIP, last-seen, version and audit metadata.
   Composite account ownership, forced RLS and tenant predicates are mandatory. Kinds and
   roles are validated against the configuration catalog; profile human status is always
   false. Account ownership and authority cannot be changed through profile editing.
2. Add optional API-key binding and explicit OAuth binding keyed by account plus stable
   OAuth client id. Explain in UI that this binds all that account's sessions for that
   client, including refreshes. Never bind by display name or caller-supplied seat. Verify
   binding/profile live for every request; revoked/disabled/missing bound rows fail closed.
   Preserve binding on key replacement; make OAuth client id survive verifier mapping.
3. Decode stored key scopes/projects/machines fallibly. Malformed restrictions refuse
   verification, rotation and listing; they never become an empty unrestricted set.
   Exclude token issuance from secret replay. Render a successfully issued one-time secret
   even if the subsequent metadata list read fails, without caching it for a later GET.
4. Use canonical `@agent:<account>:<id>` for actor, fence identity, lease holder, inbox and
   notices. Reserved syntax cannot be a caller seat alias. Both changed and omitted headers
   must resolve the same bound agent. Keep explicit account/project/machine restrictions
   in addition to profile rules. Unbound credentials retain their existing contract.
5. Wire profile WIP to the library's cross-board principal limit and workload query. Null
   uses the configured principal default; zero means unlimited. Current claims can renew
   after lowering a limit; new claims cannot exceed it. Use authorized visible boards for
   claim details and authorize aggregate disclosure separately. Last-seen writes are
   throttled to 15 seconds and use the database clock; read failures are visible.
6. Implement source roles and policy in AgentHub's domain: `board_roles`, versioned
   `board_policy_*`, typed matcher and validation API. Default deny, ordered priorities,
   deny wins at a tied priority, subject kind/role/principal/wildcard matches and project/
   label/action selectors. Evaluate resource selectors against current rows inside the
   mutation transaction, not a stale host preflight. Host provides verified identity;
   policy cannot bypass hard account/project boundaries or human-only guards.
7. Library role validation accepts the caller's transaction so profile edits can validate
   role ids atomically without host-owned board SQL. Hub migrations run before AgentHub
   migrations: do not reference a not-yet-created board table from a hub migration FK.
   Role removal must refuse or atomically update references, never silently grant access.
8. Build roster/editor, kinds/roles/policy forms and key/OAuth binding controls. Owner sees
   environment administration; ordinary people manage only permitted agents. Show actual
   enabled state, last seen, WIP and visible live claims. Labels are not identity. Prevent
   privilege escalation and self-lockout in human administration; audit all changes.

Acceptance: two keys and OAuth refreshes for one agent share identity and cross-board WIP;
different accounts with identical names do not collide; disable/revoke blocks an existing
MCP session; stale edits cannot overwrite; malformed restrictions fail closed; a registered
agent is never reported human; hidden tasks are absent from roster details. Policy tests
cover deny precedence, selectors, concurrent label changes and role deletion. Browser
390/1280 without JavaScript must register, issue, bind, edit, disable and inspect claims.

## A03 — mutation receipts and public surface parity

Source: `store/store.go` receipt semantics, `httpapi/`, `mcptools/`, `stream/`, CLI and
protocol tests. Source request keys are principal-scoped across operations, not per board.

1. Add one reusable receipt facility under `crates/store/src/receipts/`, with its own
   migration history and `store_request_receipts` prefix. Update the store's ownership
   documentation and prefix test explicitly. Run this additive migrator after caller
   migrations so the hub role exists when present; standalone AgentHub also installs it.
   The facility scopes rows by tenant + canonical principal + request key (1..200 bytes),
   stores operation plus canonical body digest and nonsecret response, and uses forced RLS.
2. Expose begin/complete operations on the caller's `AnyConnection`. In the same serializable
   transaction, reserve/check the key, perform the domain mutation, append events/outbox and
   save the response. A concurrent identical request waits/retries then returns the saved
   result. A different operation/body with the same key refuses; no external effect runs
   twice. Check current authority before disclosing any saved response.
3. Wire all source replayable mutations, not only ticket creation. Keep key/token issuance
   excluded. A lost connection after commit must allow safe retry; rollback leaves no
   receipt. Do not prune keys in a way that permits an old request to execute again.
4. Map each original REST route, MCP tool/resource, CLI command and SSE event to a supported
   Rust surface. Keep typed handlers as adapters around domain functions. Body limits,
   pagination, useful refusal codes and replay indication belong in contracts. Expand
   dynamic source registrations. Document renamed operations with an exact mapping.
5. Stream from committed history with reconnect cursors, authorization rechecks and bounded
   backpressure. A disconnect must not lose committed mutations or leak another account's
   events. Preserve current stronger origin/CSRF controls.

Acceptance: concurrent duplicate requests produce one mutation/event/outbox; changed body
and cross-operation reuse refuse; crash-before/after-commit and revoked-authority replay
are covered. REST/MCP/CLI tests exercise real handlers; reconnect resumes in order without
cross-account data or a fabricated success response.

## A04 — machine grants, sandbox and change policy

Source: `access/`, `changepolicy/`, `policyrules/`, `policygate/`, `machineops/`, machine
and bridge grant/sandbox code, and `devapi/build.go` wiring.

Owners: hub `machines/` and existing approvals; `crates/link`, typed `crates/contracts`
and `crates/machine-engine` execution/service. Extend those owners, not a second runner.

1. Port access requests, grant presets and versioned grants with principal/machine/tool/
   path/network scope, expiry, revocation and human decision evidence. Persist PostgreSQL
   state, publish changes through the existing link, and reject unavailable grant state.
2. Enforce effective scope at dispatch AND broker execution. Narrowing/revocation/expiry
   must reach live sessions and processes as the source contract requires. Cached grants
   have explicit expiry/version; reconnect cannot revive revoked authority. Intersect
   grants with OS sandbox capability; unsupported isolation refuses with a precise reason.
3. Port policy documents and rule matching: classes, regions, event/tool/command selectors,
   allow/deny/require/audit/collect effects, ticket/field/approval/review/runner checks.
   Validate regexes and bound input before storing. Internal policy always runs; optional
   external policy requires BOTH decisions to allow and fails closed on timeout/malformed
   response. It cannot replace the internal gate.
4. Implement change tickets, evidence and independent reviews in the existing board/review
   owner. Approval binds the exact operation/content digest and expires on relevant edits.
   Revalidate at execution; reject author-as-reviewer and stale approvals. Evidence links
   refer to actual captured output, never model assertion alone.
5. Wire browser requests/approval/revocation, MCP request/decision/exec/proc operations and
   CLI through those same authorities. Observe each attempt in the action feed without
   recording credentials. No network client in MCP or unsolicited machine-engine network.

Acceptance: expiry, revocation, narrower paths, symlink escape, network denial, reconnect,
offline stale cache, simultaneous approvals, changed payload, external gate outage and
unauthorized reviewer tests. Run real Linux sandbox and Windows Job Object probes later
in R01; an advisory board approval alone is not enforced access.

## A05 — schemas, catalogs and seeded data

Source: `catalog/{crud,kinds,seed,values}.go`, `schemacheck`, presets and devapi catalog
routes. Source schemas, not hardcoded browser options, define the data contracts.

1. Add a hub catalog domain with `catalog_` PostgreSQL tables, versioned kind/schema/value
   records and explicit prefix registration. Use a maintained JSON Schema implementation
   already compatible with dependency policy; if absent, document and add a bounded
   validator dependency after checking upstream documentation. Never accept unvalidated
   arbitrary executable configuration.
2. Seed source-defined runtime presets, shell profiles, router endpoints, capabilities,
   risk definitions and guide/provider-related catalogs idempotently. Preserve user edits;
   record seed version. Validate cross-record references and prevent deleting in-use rows.
3. Expose paginated reads and authorized CAS CRUD via domain APIs, browser and MCP. Machine
   consumers resolve approved versions through the existing link; catalogs do not execute
   commands merely by being viewed or imported. Include safe schema/error rendering.

Acceptance: invalid/unknown fields, malformed schemas, stale versions, dangling references,
seed reopen, secret-shaped values and cross-account CRUD tests. Every source catalog kind
has an explicit target and its consumers are actually wired.

## A06 — provider matrix, subscriptions and routing

Source: `providers/` schema v1, adapters/enums/model catalog; `subs/` subscriptions,
bindings, calls and routing; `secretbox/`. Extend hub `connections/` and `identity/sealed.rs`.

1. Persist provider rows/models in the connections domain with the original schema's
   auth kinds, runner, adapters/fallbacks, host bindings, model source/refresh, usage/state
   readers, OS detection and explicit capability booleans. Validate capabilities and enum
   combinations before save; do not hardcode behavior by vendor label.
2. Preserve the source seed matrix as data. Detect local clients on the broker using
   bounded argv/readers and granted paths. Model refresh uses the configured provider
   endpoint and validated response; failures do not erase a known catalog or claim success.
3. Add versioned subscriptions, role bindings, budgets/usage and route resolution under
   existing connection ownership. Disabled providers/subscriptions cannot route. Bindings
   obey account, worker and machine restrictions and deterministic fallback order.
4. Seal API credentials through the existing encryption owner; API/UI accept replacement
   only and return fingerprint/status. `cli_login` requires a linked machine and agent
   worker and stores no consumer credential at the hub. Never copy browser subscriptions
   or OAuth sessions into a shared API-key pool.
5. Implement actual connection testing, bounded provider calls and usage attribution.
   Route execution through existing broker/link when machine-local. UI exposes provider
   rows, subscriptions, bindings, test results and actionable errors.

Acceptance: schema enum combinations, disabled routing, fallback exhaustion, wrong worker,
budget concurrency, key replacement/restart/decryption failure, local-login constraints,
and secret redaction. Use controlled provider fixtures for deterministic tests; do not
spend money or send customer data merely to test a route.

## A07 — editable guides and real execution

Source: `guides/`, `guidekit/`, guide schemas/seeds and devapi/MCP hooks. Existing owner:
hub `workflows/`; current fixed onboarding is not a signed editable guide catalog.

1. Add guide/version/run/update storage to the workflows owner with versioned immutable
   published snapshots. Implement draft edit, lint, diff, import/export, publish and
   signature verification. Preserve source OS reachability and bounded-step validation.
2. Run the exact published version. Persist current step, variables, retries, logs and
   status with CAS. Human-only operator/confirm/resume steps require a live person.
   Request-access steps create/check real grants; agent operations invoke the existing
   execution path and record its actual result. Missing hooks are errors, never success.
3. Handle pause/resume, retry, cancellation, process restart and concurrent step submission.
   External work uses durable operation identity and completion reconciliation, not an
   effect inside a retrying SQL transaction. Reject secret-bearing notes and redact output.
4. Wire updates, signed bundle trust/key rotation and version comparison. Do not silently
   replace a running guide's version. Expose editor, run history and step actions in browser,
   CLI and MCP resources/tools with current access checks.

Acceptance: tampered signatures, incompatible OS, unreachable steps, missing execution
hook, denied access, duplicate step requests, paused human step, restart after dispatch,
retry limits and secret notes. A real guide must perform an authorized operation, retain
its evidence and resume correctly after restart.

## A08 — action feed, retention and archives

Source: `actionfeed/`, `auditkeep/`; extend hub `ops/audit/` and existing board history.

1. Map action categories and filters to existing audit events; add missing machine/guide/
   provider observations without credentials. Preserve actor, correlation and outcome.
2. Add versioned category settings: enabled, retain_days, max_rows, max_bytes, archive,
   capture_output, max_output_bytes, description. Zero retention limits mean unlimited.
   Keep output capture consent and bounds explicit; visit analytics is a separate store.
3. Implement gzip JSONL archives, chained manifests and optional Ed25519 signatures.
   Archive to a temporary file, flush/close, verify hash/count, then atomically publish and
   record the manifest before pruning. Failures preserve source rows. Bound disk/memory.
4. Preserve chain continuity through signed/checkable archive anchors; never delete audit
   history in a way that makes verification falsely pass. Concurrent archivers claim a
   deterministic range; retries cannot create inconsistent manifests or prune twice.
5. Add retention/archive browser pages and verification/download controls with owner
   authorization, path containment and sanitized filenames. Background work uses existing
   service scheduling; no independent scheduler.

Acceptance: archive-before-prune ordering; write/disk-full/signing failures; restart at
each boundary; concurrent workers; truncated/tampered archives; zero limits; chain verify
across live and archived rows; ordinary-user denial and bounded output capture.

## A09 — file, document, search and process parity

Source: bridge tool dispatch, file/search/process/document implementations and tests.
Targets: `crates/contracts`, `crates/machine-engine`, broker service and existing MCP/CLI
adapters. Consult `.ai/docs/desktop-commander-parity.md` but verify each row against code.

1. Expand the original tool registry and map every operation/schema to current contracts.
   Verify file read/write/edit/move/delete/stat/list/search, process start/read/write/kill
   and session semantics. Preserve path grants, encoding, atomicity and output bounds.
2. Fill source-supported PDF read/write/modify, DOCX create/edit/outline and XLSX read/grid/
   edit/write behavior with real format implementations. Scope is the source behavior;
   the separate proposed PDF/font platform does not enlarge this packet automatically.
   Validate library licensing and upstream format APIs before adding dependencies.
3. Reject malformed/oversized/zip-bomb/password-protected inputs with useful errors. Enforce
   access on every contained path and output. Use private temporary files and atomic
   replacement; failed transformation leaves the original intact.
4. Preserve session/cursor behavior or demonstrate a documented equivalent with acceptance
   covering the original use case. Stateless search alone is not proof of equivalent
   pagination, cancellation or bounded results. Wire tool availability to real capabilities.

Acceptance: source fixtures and adversarial documents, round-trip output opened by an
independent reader, Unicode/binary boundaries, search continuation/cancel, concurrent edit,
process output/termination and denied path tests. Never advertise a stub tool as supported.

## A10 — original GUI sessions, capture and input

Source: bridge Linux X11/Wayland implementations, GUI tool registry and tests. Preserve
the original supported-platform boundary; source stubs do not imply Windows/macOS support.

1. Add typed GUI contracts and broker-owned sessions/capture/input. Platform-specific and
   unsafe implementation remains under machine-engine execution/platform. Use existing
   IPC/link dispatch; no direct browser access to a desktop transport.
2. Enforce grants for session, display, capture and input on every operation, including
   expiry/revocation. Bind coordinates to captured display geometry; reject stale frames,
   missing sessions and disconnected displays. Bound image dimensions and payload size.
3. Surface platform/session availability accurately, retain operation audit, and document
   explicit capture/input consent. Do not record screenshots by default as ordinary logs.
4. Expose source-supported GUI operations via existing MCP/CLI and corresponding browser
   session controls. Unsupported platforms return an explicit capability refusal.

Acceptance: actual X11/Wayland test session capture/input, display resize, stale target,
revocation during session and unauthorized capture. Windows compilation cannot certify
Linux behavior; R01 runs its native fixture before source closure.

## A11 — complete browser and client journeys

Source: all source web routes/components/styles and shared harnesses. Existing owner:
server-rendered hub `web/app/`, site copy and progressive enhancements.

1. Map every source page and action to a reachable Rust page; maintain a page/action
   matrix in evidence. Shared shell files support every relevant packet, not only A11.
   Integrate AgentHub navigation for boards, agents, policies and administration.
2. Finish create/edit/delete/confirm/loading/empty/error/conflict states for A01-A10.
   Show actual authority, scope, persistence and outcomes in plain language. Preserve
   failed form drafts; no success notice before the domain operation commits.
3. Verify owner and ordinary-user journeys at 390 and 1280 pixels, keyboard-only and with
   JavaScript disabled where server forms suffice. Escape source/user labels and errors.
   Add browser probes for real effects, not screenshots of static controls alone.
4. Include the matching MCP and CLI journeys so the same actor and stored records appear
   across surfaces. Remove accidental organization/customer-tenant flows from self-hosted
   navigation without deleting internal isolation boundaries.

Acceptance: every source page/action mapped with outcome evidence; no dead navigation,
unimplemented controls, hidden errors, cross-account data or inaccessible critical action.

## A12 — baseline packaging and integration

Source: S1 deployment/configuration/installers/docs and source license notices.

1. Reconcile runtime settings with one Rust binary, PostgreSQL and existing installer/
   Docker ownership. Port required behavior, not obsolete Go build commands or private
   deployment hostnames. Keep secrets out of defaults and generated images.
2. Verify clean installation, owner bootstrap, ordinary users, persistence, configuration
   edits and supported broker enrollment from actual public instructions. Preserve the
   existing development credentials/data through the separate deployment acceptance.
3. Update capability/support documentation from implemented code, source mappings and
   measured limits. Account for all SQLite uses: only justified local offline journals;
   no hosted fallback. Preserve required licenses/notices from adopted algorithms/assets.
4. Check service lifecycle and artifacts locally without triggering release workflows.
   Keep alpha status and avoid claims unsupported by platform acceptance.

Acceptance: all S1 rows and commits have authored dispositions; no original runtime feature
is hidden behind a later milestone. Checkpoint S1 locally, then author S2's delta.

## B01 — S2 delta, ca626db

Read all 17 changed/deleted paths, especially docs 27 and 39/39a-39g. Separate implemented
source behavior from proposed later requirements and map the latter to accepted product
owners. Chatmaton remains its separately owned application where the canonical product
decision says so; required hub process/approval/connectivity interfaces remain in scope.

Implement missing accepted hub approval/PWA/passkey decision signing, notification delivery,
shell update/sync and diagnostics requirements in their current owners. Bind signed
decisions to exact payload/version and reject replay. Offline queued actions require fresh
authority and conflict handling on sync. Do not add a second chat runtime in this repo.
Reconcile deleted Go packaging with the Rust release pipeline and secret-ignore rules;
never copy broad secret-scanner allowlists without checking each fixture.

Acceptance: each of 17 rows explains its exact effect or approved product ownership;
pending requirements are not called absorbed. Focused signing/replay/offline-sync/update
tests accompany applicable implementation. Checkpoint S2 before starting S3.

## C01 — S3 delta, 97172d9

Read all 79 delta paths. Apply enrollment/configuration, bridge connectivity, browser auth,
transport and dedicated-CA changes to the existing Rust counterparts.

1. Enrollment/config writes are atomic and private on Unix and Windows; verify ACL/mode
   and failed-write behavior. Tokens stay out of argv/logs; enrollment is one-use and
   principal-bound. Inspect URL redirects and transport changes for credential forwarding.
2. Compare grant cache invalidation with A04; fail closed after expiry/revocation or missing
   state. Preserve principal ids through reconnect and auth mappings.
3. Exercise OIDC/session fixation, logout/revoke, origin/CSRF, stream authorization and
   reconnect guards. Dedicated trust roots must not disable hostname or certificate checks.
4. Translate the source agent guide and private deployment configuration into public,
   origin-configurable instructions; retain private host details outside Git.

Acceptance: tests derived from each changed source test, including cross-origin requests,
redirect credential leaks, invalid CA, partial enrollment writes and revoked cached grant.
Do not re-port unchanged S1 files. Checkpoint S3 before S4.

## D01 — S4 delta, adca8b7

Read all 32 delta paths. Verify nonroot reproducible image/runtime permissions; readiness
must perform a bounded real database check (source timeout 1 second), distinct from process
liveness. Graceful shutdown stops admission and drains in-flight work within the source's
5-second budget, then closes resources. Preserve browser auth boundaries.

Port missing provider enum coverage and fresh-browser smoke assertions; historical images
and logs remain historical evidence. Public image contents contain no private state.

Acceptance: isolated Docker readiness with reachable/unreachable PostgreSQL, SIGTERM during
work, nonroot writable-state/read-only-code checks and fresh-browser login/navigation.
Checkpoint S4 before S5.

## E01 — S5 delta, c936f7f

Read all 42 changed paths and the previous versions. Audit earlier claimed ports against
the actual diff: live capability reporting, broker file mutation/retry/journal behavior,
Windows Job Object/startup lifecycle, board provenance, reconnect and MCP capability data.

Reproduce source bug triggers before classifying equivalents. Preserve permanent retired
retry keys, exact operation identity and visible failures. Capabilities reflect the running
broker's supported operations; reconnect refreshes them without widening authority.
Provenance binds original source/revision and survives updates without being fabricated.
Translate preserved probe scripts to the canonical harness and run focused local probes.

Acceptance: each delta row links actual current code and the source-specific check; missing
behavior is implemented, not inferred from prior PR titles. Checkpoint S5. Only now may
R01 begin full validation and publication.

## R01 — validate, merge oldest first, then delete

Follow the parent plan's exact validation and Git procedure. Run full tests on the final
authored tree with required PostgreSQL and actual supported platform fixtures. Fix failures
without weakening checks. A validation receipt must name the tested tree; rebases/conflict
fixes invalidate affected evidence.

Merge each completed source checkpoint oldest first, library changes before parent gitlink.
Require green reviewed-head CI and verify main's resulting tree/checks. Delete completed
integration branches only after verifying their remote heads. Close preserved source refs
only with complete row/commit coverage and durable source archive provenance.

Finish fresh-install and existing-data Docker acceptance against the official main image
digest. Verify AgentHub, users, agents, policy, grants, providers, guides, archives and
source-supported machine/document/GUI operations through real user journeys and restart.
Re-inventory remaining refs. Record implementation, validation, push, PR, main and deployment
separately. Until these conditions hold, delivery remains incomplete.
