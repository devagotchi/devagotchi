# Machine access execution detail

Part of Batch 2 in implementation.md, after the catalog storage/editor unit. This is
an execution refinement of the full S1-S5 plan, not a new backlog or completion claim.

## Source comparison completed so far

Read at S1 `46419c697964016eb547c60416a8f2f5e854a95f`:
`server/internal/access/{service,request,check,authorize,decide,sandbox,resume,resume_ack,revoke,reaper,util,defaults,read,policygate}.go`
and `bridge/scope/{scope,narrow,match,scope_test}.go`. Also read `devapi/{mcp_access,access_routes}.go`, migration `004_devagotchi_access.sql`, and `hubtest/access_test.go`. Remaining platform sandbox/proxy, unsandboxed tests and change-policy files
must be read before their code is authored. Current seams read: contracts link frames,
machine broker access checks and hub machine link calls. Broker ownership and keyboard
leases are real existing checks; they do not enforce the source selectors/sandbox.

## Corrections necessary to preserve the intended authority

- The source increments grant uses on every Authorize call and Live rejects a grant
  whose count reaches the maximum. A once-admitted process must retain its operation
  authority while a second start is refused. Add a durable admission identity bound to
  the original canonical request; retries recover that admission without consuming again.
  Continuation authority and permission to admit another operation are separate checks.
- Source narrowing treats empty paths/argv/actions as a subset, while Fits treats those
  empty lists as unrestricted. Use one typed meaning in both checks: an explicit empty
  allowed list permits nothing; absence means unrestricted only for the source's
  documented unrestricted fields. Hosts/environment variables absent or empty permit none.
  Keep tests for empty arrays, malformed scalar/list types, removed numeric limits,
  removed deny keys, argv element boundaries and wildcard host containment.
- Source Strs silently accepts scalars as an empty list, numeric reads default invalid
  values to zero, and unknown equal sandbox strings pass LevelWithin. Reject malformed
  typed selectors, levels and durations at parsing. Catalog JSON Schema remains an
  additional capability-specific check, never the only authority validation.
- Lexical path containment cannot prove OS access. Shared scope code may conservatively
  reject a request; the broker must resolve paths under its own OS, workspace and home,
  check link/reparse escapes, and execute inside an actually available sandbox. Do not
  claim shared string comparison confines filesystem access.
- Source authorization checks task lease identity but not every expiry/policy condition
  in the same query. Admission and continuation both check live principal, machine,
  policy, task generation/fence/lease expiry or guide state. Ending events and notification
  intents commit together. Reaper failures surface; lost notifications cannot extend the
  broker's bounded authority expiry.

## Concrete implementation sequence

Composition decisions grounded in the existing owners:

- `HostedBoard::scope` locks the physical board generation before principal-policy
  initialization. Every task-bound access transaction must use that same order:
  board, principal policy, catalog, access/admissions. Never lock access rows then call
  the board owner. Continuations first read the immutable task binding, acquire the
  board lock, then reread/check the access record. Reapers use the same ordering.
- Add `hosted/access.rs` to return and recheck a typed lease binding inside the caller's
  transaction: tenant, physical board ID, ticket ID, canonical principal, fence,
  lease ID and database-clock expiry. Require a live principal directory even for a
  read; ordinary receipt replay is not authority. Heartbeats preserve the lease binding,
  release/reclaim and project recreation invalidate it. The hub never queries board
  tables to make this decision.
- The broker currently uses account plus MCP session as its caller identity, and its
  IPC service unwraps `WireRequest` before handling it. Grant admission must additionally
  bind the canonical agent and durable operation identity and carry authority through
  link and IPC; a check at the hub alone does not enforce a grant at the broker.
- Catalog request material includes the selected preset version, capability schema,
  shell profile and matching risk text/predicates in one catalog-locked transaction.
  A later catalog edit cannot rewrite that snapshot. OS profile mismatch, a stale
  preset, unsupported executable capability or invalid schema fails the whole request.
- Risk path predicates check overlapping scopes: a grant for `/` includes `/mnt/`.
  Missing unrestricted paths may include the risky path; explicit empty paths do not.
  Source's one-direction prefix check otherwise omits a relevant warning.

1. `contracts/src/grants/{mod,selector,narrow,matching,risk,tests}.rs`, re-exported from
   `lib.rs`: closed capability/level vocabulary, parsed duration, typed selector values,
   immutable risk context and per-call effective authority. One shared scope evaluator
   serves hub and broker. Derive wire schemas where exported; add the actual family to
   the contract export root and regenerate once the complete wire shape is authored.
2. Catalog owner exposes composable reads/validation inside the access transaction;
   access does not query catalog tables directly. Add reference checks for actual access
   request/grant consumers before considering A05 covered. Replace the current
   `catalog/references.rs::default_selection` refusal with the live environment-wide
   unsandboxed policy and typed-confirmation check from source `access/defaults.go`;
   apply it on selection and edits that escalate an already selected default. The catalog's generic human
   CRUD never grants execution by itself.
3. `hub/src/machines/access/` owns request groups, decisions, grants, admitted calls,
   unsandboxed policies, resume acknowledgments and ending notification intents in
   PostgreSQL. Allocate identifiers outside retried transactions. Share identity/policy,
   machine directory, board lease and guide state through their owners. Every write has
   a version and canonical receipt; current authority precedes receipt replay.
   Restore the pinned source access policy/role preset through the existing AgentHub
   policy owner before exposing requests. A per-environment seed mark applies this
   previously missing family once, preserving existing IDs and later edits/deletions.
   Keep source actions (`access.request`, `grant.*`, `policy.unsandboxed.*`, operation
   actions) and CLI/service indefinite-grant denials. Policies do not replace the
   independent-person, account, machine, lease or native-enforcement checks.
4. Request expands preset items, validates all selectors/levels/duration/use bounds,
   requires a substantive reason and valid live task/guide binding for task duration,
   snapshots the matching risk explanations, and applies denial cooldown/expiry settings.
   GUI input requires observation authority; broad input/unsandboxed access needs a live
   explicitly acknowledged policy. Independent person approves/narrows or denies.
5. Admission chooses the least privilege covering grant and stores its canonical
   operation identity atomically with use consumption. Broker authority binds that
   identity, machine, principal, grant/version, selector/level, expiry and task fence.
   Keep the link version at 1: capability-negotiate extended frames and fail closed for
   a peer that cannot enforce them. Do not trust tool-supplied authority fields.
6. Integrate platform sandbox/proxy and broker operation admission/continuation/revoke
   before enabling the corresponding level. Existing local trusted IPC remains a
   distinct authority path. Actual platform tests must prove process termination,
   network host restriction and filesystem escape refusal.
7. Add the source HTTP/MCP access operations and owner/approval/machine browser flows
   after the domain contract is complete. Resume alerts are scoped to the person,
   deduplicated while open, and allow continue/revoke/pause; pause must stop applicable
   ongoing authority, not only alter a status label. Live checks and lifecycle endings
   must agree across all adapters.
8. Complete source change-policy documents, evaluation, evidence, independent reviews
   and bounded external decision provider in the separate owner already specified in
   implementation.md. Both internal and configured external decisions must permit.

## Acceptance

### Durable lifecycle implementation contract

The next domain unit uses `machine_` tables under forced PostgreSQL RLS and the existing
store/receipt/audit owners. Request IDs, group IDs, grant IDs and admission IDs are
allocated before retryable transactions. Keep request input separate from resolved
catalog snapshots so retries cannot silently become a different request after a default
or preset changes. Settings own bounded pending expiry/cooldown, offered durations,
required risk IDs and the broker authority refresh window; no hidden constant overrides
an editable source setting.

- Requests retain account and canonical principal, machine enrollment binding, optional
  physical task/lease binding or guide binding, reason, immutable prepared catalog data,
  duration/use limit, pending deadline and independent decision/version. Group expansion
  is atomic. A preset duration override narrows; `once` always means one admission.
  Task duration requires an actually live task or guide owner. Missing guide integration
  cannot be accepted as a free-form ID. Denial cooldown compares the original capability
  and selector and collapses duplicates without approving them.
- Decisions load the stored binding without locking it, acquire the bound board lock,
  then current identity/policy/catalog/machine/access locks and reread the request.
  Approver is a currently authenticated person and differs from requester. An approved
  item passes `GrantItem::within`, duration/use narrowing and current policy. GUI input
  requires live observation authority, not merely a historical observation row. Both
  original risk explanations and the chosen effective scope remain inspectable.
- Policies are either environment-wide or tied to a machine enrollment generation.
  Enabling requires all configured risk acknowledgments and a future optional expiry;
  record the actual acknowledged explanations. CAS changes, audit, affected grant endings
  and notification intents commit together. A machine-specific enable never authorizes
  the environment-wide full-access default. Default selection/escalation needs the
  typed preset ID as well as the current acknowledged environment policy.
- Admissions use a durable operation identity plus canonical input digest, unique for
  principal/account and machine enrollment. Check live identity/policy/lease/machine
  before recovering an admission; changed input is a conflict. Fresh admission and use
  consumption are atomic. Retries/continuations do not consume again. Exhausting a
  once grant stops additional starts but does not kill its already admitted operation.
  Operation completion/retirement remains owned by the broker journal, with a durable
  correlated receipt at the hub; do not create another process scheduler.
- A short-lived authority envelope binds admission, principal, machine generation,
  grant/version, effective item and task fence/lease, with expiry no later than the
  live lease/grant and refresh window. The broker derives the actual `GrantCall` from
  the operation. Remote requests cannot choose an identity, authority or trusted-local
  IPC path. Unsupported peers fail before executing; refresh/reconnect cannot revive a
  revoked grant. Explicitly separate already-completed retry/history reads from admitting
  another effect, retaining current account/caller authorization for either path.
- Endings update the grant once and create a unique durable machine notification intent
  in the same transaction. Dispatch retries outside the transaction, acknowledges only
  the exact delivered generation/version and reports failures. Lost delivery is bounded
  by broker expiry. Policy disable can end rows without acquiring board locks; task-aware
  reaping acquires each board first and never reverses the lock order. Resume alerts are
  per person/grant/session, deduplicated while open; pause also ends ongoing authority.

These are acceptance obligations for the domain. Migration 0041 defines the lifecycle
tables; the implementation receipts below distinguish the subsequently authored domain
operations from the still-unfinished dispatcher integration, broker enforcement and UI.

Port the source scope fixtures plus the corrected widening cases. Real PostgreSQL:
duplicate/retried once admission, concurrent last-use starts, task renewal/reclaim/end,
principal disable, current owner/policy on replay, independent approval, atomic groups,
decision narrowing, risk snapshot immutability, expiry/revocation, lost notification,
default preset and resume lifecycle. Real broker/native tests: no forged authority,
unsupported peer refusal, path/link escape, denied environment/network/argv, expiry and
revocation of active processes/GUI, continued read/input under admitted authority only.
Real UI/API/MCP checks cover owner/user/agent, exact origin and preserved refused drafts.

## Authored foundation and local evidence

Shared contracts now implement typed selectors, item/level/duration narrowing, call
matching and risk predicates. Catalog writes validate that vocabulary as well as their
capability schema. Request preparation snapshots all selected preset/capability/profile
versions and matching risk text/predicates in one transaction, refuses stale presets and
duration widening, and preserves the original source's 77 seed rows unchanged.

AgentHub's `TaskLeaseBinding` composes current principal and physical board generation,
holder/fence/lease identity/expiry checks into the access transaction. Tests cover
heartbeat, release/reclaim, changed identity, wrong generation, expiry and a concurrent
release blocked until the transaction finishes. Identity's `machine_actor_in` refuses
seat aliases, spoofed people, stale roles, foreign accounts and disabled accounts/profiles.
Machine registry migration 0040 gives every enrollment a distinct generation; current
machine bindings cannot revive after revoke/re-enroll with the same name and key.

Focused evidence: 26 contracts tests passed; three AgentHub access-binding PostgreSQL
tests passed; 28 hub catalog/access-identity/machine-binding/store tests passed, zero
ignored. Logs are `target/absorption-grant-scopes.log`, `target/absorption-task-access.log`
and `target/absorption-access-boundaries.log`. Eight layout checks passed. These verify
the foundation, not executable grants. Durable requests/decisions/admissions, live policy,
native enforcement, adapters and UI remain open and are the next implementation work.

Additional source comparison: read `hubtest/unsandboxed_test.go`,
`bridge/sandbox/{spec,linux,windows,darwin}.go`, `bridge/netproxy/{proxy,forward}.go`.
The source Windows sandbox is explicitly a TODO that starts an ordinary process while
an environment variable can make its probe advertise confinement. Do not copy that
claim: implement native confinement and prove it before advertising it. Source Linux
silently skips absent bind paths and source path expansion falls back after symlink
resolution failure; executable authority must refuse unresolved/escaping targets.
Source proxy cancellation closes its listener but not established tunnels. New proxy
ownership must close active connections on revocation/expiry and bound connections,
header parsing and timeouts. These findings refine the existing native-enforcement task.

Status: A04 remains partial and open. No enforced sandbox, source closure, CI, remote
publication, branch deletion or main delivery is claimed by these local checks.

## Policy defaults and durable schema/settings unit

AgentHub checkpoint `a7c14ec` adds migration 0008 and the byte-exact source access policy
preset: 27 rules and builder/researcher roles. Its per-environment seed mark preserves
existing record IDs, edits and deletions. Stable mapped IDs are `source-access-01` through
`source-access-27` in pinned file order. Focused policy/lease/catalog checks passed:
18 AgentHub and 20 hub tests, zero ignored (`target/absorption-access-policy.log`).

Hub migration 0041 defines seven lifecycle tables with forced RLS and composite
environment/account foreign keys. `machines/access/settings.rs` implements validated,
versioned access settings, permanent request receipts and one audit event per mutation.
`/api/v1/access-settings` is owner/person-only with exact origin and request identity;
required risk references block catalog deletion. The seven-table RLS test uses the actual
non-superuser, non-bypass `devagotchi_hub` role, proves foreign rows cannot be read/changed/
deleted/moved, and checks that a foreign environment's account cannot be attached.

Validation: the 33-test settings/catalog/access/store slice passed 32 and exposed two
HTTP-fixture assumptions in the remaining scenario: duplicate Origin headers are a 400,
and the outer middleware's foreign-origin refusal is not the JSON adapter response.
Both fixture corrections preserve the expected refusal. The focused HTTP scenario now
passes (`target/absorption-access-settings-http.log`), with all other 32 scenarios already
passing in `target/absorption-access-settings.log`. Eight layout checks and diff checks
pass. This is not runtime acceptance of request approval or machine execution.

Current next implementation: `machines/access/{requests,decisions,grants,admissions,policies}`
against the above schema and transaction contract, followed by durable ending dispatch,
broker authority/sandbox enforcement and complete source adapters/browser flows. The
settings API has no browser editor yet; author it with the full access interface.

Remote read-only check during this work: root `main` is still `4b01ac9`, remote integration
is `ed92ea9`; AgentHub remote `main` is `8a6db52`; both repositories have no open PR.
No remote push, CI run, merge, branch deletion or Docker development-image update occurred.

## Request, decision and admission authoring

The next code unit is authored under `crates/hub/src/machines/access/`, with current
identity reconstruction in `identity/accounts/machine_access.rs` and current task project/
labels in AgentHub's `hosted/access.rs`. No HTTP/MCP/browser adapter exposes these new
operations yet. They do not claim native confinement or successful broker execution.

- `requests.rs`: original-input permanent receipts, atomic groups, bounded reasons and
  use limits, immutable capability/preset/profile/risk snapshots, pending expiry and denial
  cooldown. Identity, task, policy and machine generation are checked before receipt replay.
- `terms.rs`, `records.rs`, `decisions.rs`: independent owner/person approval and denial,
  CAS, narrower-only effective scope/duration/use count, fresh requester profile, original
  task generation/fence/lease, live policy and enrollment checks, and one transaction for
  the grant, request decision, receipt and audit. A disabled requester can still be denied.
- `policies.rs`: environment or enrollment-bound unsandboxed policy, CAS and versioned
  risk acknowledgments. Changed risk text must be reviewed again. Disabling one policy
  preserves grants covered by another live policy; uncovered broad GUI, unsandboxed and
  policy-dependent preset grants end atomically. Re-enabling cannot revive ended grants.
- `grants.rs`, `notifications.rs`: versioned individual and enrollment-scoped revocation,
  durable ending notifications, bounded claims, retry backoff, crashed-claim recovery and
  exact enrollment/version/attempt acknowledgments. External broker I/O is still to be
  attached; an outbox row alone is not delivery. Notification failure codes cannot contain
  transport URLs or arbitrary error text.
- `grant_record.rs`, `admissions.rs`, `admission_action.rs`, `admission_completion.rs`:
  admission identity binds account/principal, enrollment, grant version, broker input
  digest and actual call projection. Current exact operation policy and scope are checked;
  an unavailable proved sandbox refuses before use consumption. Duplicate starts and
  continuation consume once; different concurrent starts compete for the last use.
  Authority expiry is bounded by settings, grant time and current task lease. Completion
  and permanent retirement preserve the retry identity. These are trusted adapter APIs;
  actual request-to-call projection, platform proof and broker enforcement remain required.

Focused local evidence, required PostgreSQL and two test threads: the 22-test lifecycle/
notification slice passed (`target/absorption-access-notifications.log`), then the expanded
26-test admission slice passed (`target/absorption-access-admissions.log`). The injected
outbox-failure fixture initially attempted DDL using the restricted hub role and correctly
failed with permission denied. Fixture installation now uses the test database's migration
connection; revocation itself still runs under the restricted role and proves full rollback.
The earlier narrowing fixture's argv shape was corrected; both narrowing tests pass.

The expanded exact-operation-policy and task-bound admission slice also passes: 27 tests,
zero ignored (`target/absorption-access-authority.log`). It proves a newly installed
`proc.start` deny blocks continuation and that release/reclaim invalidates the original
admission lease. Eight layout ratchet tests and the absorption inventory/object check
pass. AgentHub's resource-binding checkpoint is `045e837`. Unused operation-action variants
remain visible compiler warnings until their planned adapters are authored; no lint was
suppressed and no full workspace/CI gate has been claimed.

The next local reaper unit is now authored in `reaper.rs` and `reaper_tests.rs`.
It pages live grants and pending expiry, composes the current identity and machine owners,
and takes task lease locks before policy/machine/access locks. AgentHub's separate
`check_access_lease_in` supplies lease evidence for a trusted reaper without pretending to
authenticate a caller. Expected disabled/missing/expired identity or lease states end the
grant; unexpected storage/domain failures propagate. Each ending uses the same durable
outbox. A once grant stays live while its admission is ongoing and ends after the broker
records completion. Task release/reclaim ends its admission's grant even when a process
was still admitted. Focused access regression: six AgentHub and 45 hub tests passed,
zero ignored (`target/absorption-access-reaping.log`). Runtime scheduling and notification
transport are not wired by these domain tests; guide-bound reaping remains for A07.

Remaining A04 obligations are explicit: configured/shared-machine resolution; machine
pause/resume adapter integration and reaper runtime/guide integration; full policy evaluation/change/review
domain; request/grant display enrichment, request/decision REST and MCP/browser actions; authenticated wire and
IPC authority propagation; actual broker refresh/ending delivery; native Linux/Windows/macOS
confinement and network enforcement; source-complete regression/real-OS acceptance. In
particular, an already-exhausted once grant must not be reaped while its admitted operation
is continuing. No source row or commit is closed by the above local unit tests.

Resume/control implementation detail for the next unit: registry-owned control versions
are separate from enrollment generation. Pausing a machine (or stopping GUI only) blocks
new admissions, retires affected active admission identities, bumps affected grant versions
and queues interruption notices atomically. Resuming clears the control flags; it never
restarts retired operations. Approval remains required for new grants. Resume alerts use
the immutable grant/risk snapshots, one open alert per person/grant, bounded pages and
current person/policy checks. Continue only acknowledges an alert; revoke and pause compose
the existing access/registry owners in the same transaction as the acknowledgment. Browser
session labels are noncredential identifiers, not authentication. Agent resume alerts target
the approving person and cannot confer human authority. The source idle interval is bounded
to 1–60 minutes. Runtime callbacks and the actual resume UI remain part of the adapter unit.

The control/resume/observation domain unit is now authored. Migration 0042 stores machine
pause and GUI-stop state separately from enrollment; migration 0043 preserves existing
settings and adds the idle interval. Resume actions compose revocation or machine control
atomically with acknowledgment. Existing GUI observation grants must match live principal,
machine enrollment and the original task lease, and are checked again at input admission.
The completed machine slice passed 83 tests; the expanded access slice passed 56 tests,
zero ignored, using required PostgreSQL (`target/absorption-machine-control.log` and
`target/absorption-access-resume-observation.log`). These are domain proofs, not evidence
of wired browser callbacks, native GUI target matching or broker notification delivery.

Default-selection integration is authored in catalog mutations/references, access policies,
HTTP and browser catalog adapters. Both selection and editing an existing default require
the exact typed preset ID and live environment policy when metadata or typed items require
it. Machine-only policy is insufficient. Receipts bind confirmation and recheck live policy.
Selection checks the source `preset.default` action. The confirmation remains empty on a
fresh browser render and preserves escaped input on refusal. Required-PostgreSQL validation:
12 catalog/API/browser tests passed (`target/absorption-catalog-defaults.log`), followed by
three `access_default_` policy-boundary tests using the same compiled hub test binary.
All passed with zero ignored. Machine-only policy, expired-policy replay, confirmation
receipt conflicts, selected-default edits and unlabeled unsafe items are covered.

Read-surface implementation contract: port `access/read.go` into the access owner before
adapters. Every page and long-poll iteration revalidates the canonical actor. The owner
can review the environment; other people see their account and agents see only their own
principal. Explicit projections exclude tenant internals and broker retry material.
Requests/grants use bounded stable timestamp/ID cursors, validated filters and current
expiry; request/group lookup returns the actual granted terms and historical decision.
An approval or listing is never an execution permit. Lists and lookup remain usable for
ended/revoked history; admission still proves current task/policy/machine authority.
HTTP reads delegate this owner with existing scope/session boundaries. Tests cover group
lookup, narrowed decisions, expiry without reaper, pagination, foreign/disabled principal,
owner versus agent visibility and no state changes from reads. Resume/control/policy
write adapters then compose the previously authored transaction owners.

Read domain and HTTP adapters are authored in `machines/access/reads.rs` and
`agenthub/http/access_reads.rs`; `identity/accounts/machine_access.rs` resolves only
middleware-authenticated people or credential-bound registered agents. Machine-limited
keys must select an allowed machine; project-limited keys cannot read environment history.
The required-PostgreSQL `access_read_` slice passed all five tests with zero ignored
(`target/absorption-access-reads.log`), including revocation during HTTP long polling.
Source display-name/task-title enrichment and MCP/browser reachability remain open.

Human policy/revoke/resume/control HTTP adapters are authored in `access_admin.rs` with
strict bodies, original retry keys, expected versions and enrollment-generation checks.
Registry lookup is composed by `administration.rs` after current environment-owner proof;
configured/shared machine resolution is still explicitly unavailable. Read helpers use
shared machine locks, avoiding an unnecessary lock upgrade during inspection. Policy lists
report obsolete machine-generation policies as non-live. The final required-PostgreSQL
`access_` slice passed 66 tests with zero failed/ignored
(`target/absorption-access-surfaces.log`). This includes all new read/administration HTTP
tests and existing access/control/admission/identity regressions. During authoring, the
static-SQL API rejected dynamic lock-clause formatting; the registry helper now selects
one of two fixed statements. The cross-origin fixture now supplies exactly one Origin
and checks the middleware response directly. No refusal or authorization assertion was
weakened. These results do not establish native broker confinement or delivery.

Next complete unit remains authenticated broker integration, before request/decision
adapters can use broker facts or claim enforcement. Inspected actual current handoff:
`machines/link/calls.rs` creates `WireRequest`; `link/session/mod.rs::handle` forwards only
its body through IPC; `ipc/lib.rs::exchange` reconstructs a body-only envelope;
`machine-engine/service/mod.rs` again passes only the body to `Broker::handle`.
Thus authority must survive every one of these seams. Capability negotiation stays on
link protocol 1, and must gate new frames. No unsupported peer may receive a new shape.
The source Windows sandbox is a plain-process TODO that can advertise confinement via
an environment flag; preserve its intended refusal requirement, never that false claim.
The source Darwin profile and Linux namespace/proxy paths still need their actual native
acceptance and per-operation integration before their levels are advertised.
