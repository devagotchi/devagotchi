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

Broker integration contract (grounded before edits): define a closed remote IPC envelope
separate from the trusted local request, with explicit prepare/execute/refresh/end verbs.
The link supplies origin; tool payloads cannot select it. Preparation derives canonical
input and call projection at the broker. Execution repeats normalization and compares the
digest before any admission. Authority binds origin, enrollment, account/principal,
admission, grant/version, durable retry key, effective item and original task lease.
Its database-clock validity is capped at 30 seconds; the broker converts the remaining
time to a monotonic deadline so clock rollback cannot extend an admitted process.
Refresh cannot change immutable binding or revive an expired/interrupted admission.
Interruption notifications establish a minimum allowed grant version: a delayed pause
notice for version N interrupts older versions, without killing new work approved at N
after resume. Persist that floor and admission identity in the existing private broker
journal, atomically with operation admission. This machine-local SQLite use is specifically
the existing crash-safe execution/retry journal, never a hosted coordination store.
Broker operation ownership must include the principal even across MCP session changes.
Unsupported confinement levels refuse before admission; an executable allowlist is not
reported as a sandbox. Native sandbox support remains part of this batch, not substituted
by a successful authority-envelope test. No new remote capability is advertised until its
actual broker and transport handlers are implemented and checked.

Broker integration authored on 2026-11-21: `contracts/grants/{authority,remote,action}.rs`
defines the closed authority envelope. `ipc`, `link/session/remote.rs` and
`hub/machines/link/remote.rs` preserve it across authenticated peers, enrollment checks
and bounded shared capacity. Only the actually implemented native unsandboxed process
path is advertised. `machine-engine/broker/{normalize,start,remote/}` repeats canonical
input/selector checks, binds retries to principal/enrollment/admission and clears remote
inherited environment. Monotonic expiry and interruption serialize with native process
creation. The existing private journal persists admission identities and revocation floors;
recovery never restarts an old process or signals a recovered numeric PID. Native sandbox
levels and other machine operation families remain open.

`machines/access/broker.rs` composes prepare, current PostgreSQL admission and execution;
task authority hashes the immutable original lease identity, not heartbeat expiry or
mutable labels. `delivery.rs` acknowledges only broker-confirmed interruption delivery.
`runtime.rs` and `identity/accounts/maintenance.rs` page environments, requests and grants,
resolve original task boards, reap ended authority and deliver durable outbox records.
The runtime continues other environments after an error and reports the failure. This
does not yet wire ordinary machine tool dispatch, refresh running admission leases, or
provide post-expiry execution history; those remain explicit work in this batch.

`agenthub/http/access_decisions.rs` adds human approve/deny through existing narrowing,
CAS and receipt owners. Approval resolves the request's original account/project; tool
arguments cannot redirect it. Credentials are refreshed before the domain transaction.
Agents, seats, foreign origins, unknown fields, self-decisions, widening and stale versions
refuse. Source request creation, MCP access tools and approval browser UI remain open.

Local evidence (required isolated PostgreSQL where applicable, zero ignored): the broker
and contracts regression passed 126 + 28 tests (`target/absorption-broker-regression.log`);
transport passed 4 hub + 2 link + 2 authority tests; bridge/outbox passed 8 hub + 2 link +
2 authority tests. The latest complete contracts and link binaries passed 28 and 21 tests.
Existing hub link behavior passed all 22 tests (`target/absorption-links-regression.log`).
The combined access check passed 74 tests with one test-helper failure: cross-origin
middleware responses precede the API's cache header. Its fixture now checks the actual
403 response directly, as existing cross-origin tests do; rerun result follows below.
The runtime fixture also now respects the database expiry constraint. No production
constraint, denial assertion or failed test was disabled to obtain these results.

New hub accepts legacy links, but new link Hello carries the remote-access field; source
protocol 1 alone is not evidence that an older strict hub accepts that shape. Keep the
hub/link installation pair consistent in prerelease packaging and test the actual pair.
No push, CI, merge, branch deletion or main deployment is established by this checkpoint.

Decision rerun: all three real HTTP decision tests passed, zero failed/ignored
(`target/absorption-access-decisions-tests.log`). The self-decision fixture explicitly
adds a human `access.request` policy first: the default permits agent requests only.
That custom policy still cannot authorize self-approval or self-denial. Layout passed
all eight checks; broker.rs shrank from 1,153 to 878 lines and its ratchet was tightened.

Next adapter unit: request creation resolves only the caller's enrolled machine and
original task board, obtains facts from that authenticated broker, and rechecks current
credential/profile before recording the request. Expose the broker's actual available
sandbox levels separately from catalog levels. Never accept OS/display/owner/generation
facts from tool or HTTP arguments. Offline or unnegotiated peers return an explicit
unavailable refusal; a pending request is never a permit. Preserve request-key replay,
source risk snapshots, principal policy and exact task lease checks. Author discovery,
creation, refusal and credential/enrollment race tests before the unit check.

MCP adapter contract: mount `request_access`, `wait_for_access`, `my_grants`,
`capabilities_list` and `machines_list` on the existing hub router. Keep canonical typed
request selection, explicit retry keys and closed schemas. Supply live bearer verification
through a server-created request extension, using the same verifier as HTTP; never store
a credential on a persistent MCP session. Recheck it after broker waits and every long-poll
iteration, and reapply machine/project key restrictions. Grants returned by `my_grants`
are the credential-bound principal's, including when the account is an owner. Catalog
discovery must read all bounded catalog rows plus current durations/default in one owned
snapshot, never suppress store errors. Machine listing uses the fallible registry and
distinguishes connection presence from successful broker execution. Configured/shared
authority and provider `role_route` remain their planned owner integration work.

HTTP request/discovery and all five MCP access tools are authored. The combined required-
PostgreSQL access regression passed 84 tests, zero failed/ignored
(`target/absorption-access-adapters.log`). New coverage includes cross-surface request and
decision replay, exact task project/fence, credential revocation during HTTP broker waits
and an existing MCP long poll, enrollment replacement, principal-only grants, closed typed
schemas, and one audit append per MCP call. The HTTP test server now uses the same store
for machine enrollment and account/access records, as `serve/mod.rs` actually does.
Unknown broker display facts conservatively retain display-dependent risk warnings.
Catalog discovery reads bounded complete catalog collections and settings under their
domain locks; no source ignored-error fallback was copied. Requests can remain pending for
a currently unavailable confinement level, but report actual broker availability separately;
admission still refuses an unsupported level before consuming a use.

The MCP regression passed 22 tests and found a stale source-scan rule: receipt-aware
`board_mutation` already delegates to audited `board_call`, but the scan did not recognize
it. The rule now names that actual helper. API-key middleware regression passed both tests.
The cross-tenant test reached the new typed selection schema and exposed its fixture
generator's missing JSON Schema `const` support; it now generates the literal discriminator,
retaining the requirement that every call reaches its handler rather than passing on a
deserialization error. Follow-up results are recorded below when complete.

Browser integration contract, grounded in S1 `AccessCard.tsx`, `NarrowEditor.tsx`,
`Grants.tsx` and `risk.tsx`: add an owner-access review page within the existing app, linked
from Home and Approvals. Show actual requester/machine, reason as escaped untrusted text,
requested capability, bounded scope, duration/use limit, task and frozen risk explanations.
Human actions use same-origin forms with original request IDs and versions; refresh the
session immediately before mutation. Narrowing must preserve untouched selector fields,
distinguish an empty path list from unrestricted paths, and offer only levels related by
the typed `within` check (the source's array-order level shortcut is insufficient).
Preserve the user's entered draft and retry key on refusal. Show request/grant history,
expiry and ended reason, with revoke delegated to the durable domain owner. Keep severity
as text, not color alone. Catalog and policy administration remain linked to their owners.
Add localization through site copy, including the neutral fixture. A stale displayed form,
another origin, a forged principal/seat, revoked session, widened selector and repeated
decision/revoke must be exercised through the actual browser routes before UI acceptance.

Adapter follow-up evidence: audit registration/dispatch scan and the real cross-tenant
all-tool/page regression both passed after their fixture corrections. All 29 contracts
tests passed, including unknown-display warning preservation. Layout passed all eight
checks after moving the schema argument generator into `tenancy/tests/schema.rs` and
tightening the existing oversized fixture's limit; no size limit was raised. This is a
local authored checkpoint only; the browser/native/full integration work above remains.

Browser review implementation is now authored on the same S1 integration branch. Home,
Approvals and the app badge expose owner review; owners retain the management link when
there are no pending requests. Review shows requester identity, original immutable scope,
risk text, duration and task. Decisions use the existing scoped PostgreSQL transaction,
version and retry receipt. Narrowing retains untouched selectors and distinguishes no
allowed paths from an unrestricted selector. Refusals retain submitted scope, note, key,
version and invalid duration. History shows the actual granted scope; grant listing shows
requester names, limits, expiry, paused/ended state and revocation. Ended filtering occurs
before the cursor limit, with a regression covering live grants ahead of ended records.

Required-PostgreSQL access regression passed 90 tests, zero failed/ignored, using eight
threads (target/absorption-browser-access-regression.log). The preceding unrestricted
run exhausted PostgreSQL connections in one fixture setup; it was not counted as a pass.
All five new browser tests and the ended-pagination regression passed in the final run.
Site/neutral-copy tests passed 11/11 and the existing real-browser-route board approval
regression passed 1/1. The extracted Approvals renderer reduced the oversized original
module below its cap; layout passed eight checks without raising any limits. These are
local validation results, not a merge or full native/browser/Docker acceptance claim.
Source UI coverage remains partial until the remaining machine controls, policy/resume
surfaces and actual execution integration are completed.

Branch validation found and corrected a Windows executable stack overflow when loading
the expanded site catalog: `Site.messages` and `Messages.app` now own boxed values.
The actual-binary startup regression loads both the default and neutral sites on the
native main-thread stack. That regression passed, as did Windows process E2E including
the linked broker, sign-in and the assertion that hosted hub state contains no SQLite.
The E2E catalog expectation now includes the five mounted access tools without changing
the expected 24 board tools. Raw MCP protocol checks passed against the built executable.

AgentHub migration replay exposed a non-repeatable policy seed table/policy and a stale
expected migration list. AgentHub commit `db1fb9d` fixes both; all 162 library tests
passed with PostgreSQL required. The dependency audit passed after admitting MIT-0 for
the specific `borrow-or-share` transitive dependency, following inspection of its license.

The subsequent complete `cargo test --locked --no-fail-fast` run used isolated PostgreSQL
and ClickHouse with PostgreSQL required and four test threads: 1,148 passed, two failed,
none ignored. The failures were the CLI settings test comparing mutation receipt fields
with a plain settings read, and the landing page omitting the five newly mounted access
tools. The test now explicitly validates the receipt before comparing every setting;
the public tool list and documentation now describe those access tools. The affected
regression rerun passed all 31 settings, landing-page and site tests, none ignored.
Logs remain private under `target/absorption-branch-tests.log`
and `target/absorption-final-regressions.log`; no source closure, CI, push, merge or branch
deletion is established by these local checks.

Execution continuation now shares admission's current identity, original task lease,
exact action policy, machine controls, unsandboxed policy and grant checks in `current.rs`.
`admissions.rs` stores the verified action with the existing canonical call hash, never
the raw command. `continuation.rs` binds the authenticated broker's operation ID once
and renews only that admission: no second use, no expiry revival, no replacement enrollment,
and no renewal after completion. `broker.rs` records the actual start response;
`renewal.rs` obtains current broker confinement capabilities, rechecks authority after
that wait, sends the original admission binding and requires the same operation ID back.

All 59 machine-access domain tests passed with required isolated PostgreSQL, four threads,
zero failed/ignored (`target/absorption-access-continuation.log`). New coverage includes
one-use renewal, mismatched broker receipts, revocation during broker waits, disabled
principals, enrollment replacement, unavailable confinement, expiry, completion, and no
raw command in admission material. Existing task and exact-operation-policy tests now
exercise renewal too, including lease release/reclaim and a newly denied `proc.start`.
Automatic renewal/completion maintenance and ordinary MCP execution adapters remain
unwired; this domain/transport result does not establish their acceptance or source closure.

Automatic renewal/completion maintenance is now authored through the same access owner.
The broker's new private `Status` request accepts only the exact stored grant binding
and returns operation ID, lifecycle state and exit code. It neither refreshes authority
nor exposes commands/output; both link endpoints check the authenticated origin. This
lets the hub collect termination evidence after a grant or principal has been disabled.
`Admitted::binding` is separate from live authority issuance. The completion transaction
is shared with the existing receipt path and retains owner checks, immutable operation
identity, replay and one audit append. Successful evidence requires exit code zero.

The runtime checks due admissions at half their configured authority lifetime, in pages
of 100, with at most 16 concurrent broker calls and a three-second bound per admission.
It recovers a missing start receipt from authenticated lifecycle evidence, rechecks the
live principal/task/policy before renewal, and never refreshes a terminal operation.
Lost connectivity or failed checks leave native expiry in force; no cached permission
fallback was introduced. Dropping the runtime stops both maintenance loops and workers.

Focused validation passed 77 tests: contracts 2, hub 64, link 2, native broker 9; zero
failed/ignored (`target/absorption-access-maintenance.log`). The new test starts the actual
maintenance loop and checks receipt recovery/renewal, then disabled-principal/revoked-grant
completion, mismatched receipts, malformed success and repeat observation. The native
expiry test exercises the real remote status envelope before/after process termination
and rejects a different principal. Initial compile caught a test calling a private helper;
the corrected test uses the real envelope handler. Generated contract check passed.
Ordinary MCP execution adapters, confinement platforms and remaining A04 source surfaces
are still open; these results do not close S1 or authorize deleting its preserved source.

## Ordinary operation start and source grant selection, 2026-11-21

Compared S1 `server/internal/access/authorize.go`, `machineops/ops.go` and
`devapi/mcp_ops.go` at `46419c697964016eb547c60416a8f2f5e854a95f`. Enrolled,
account-backed MCP `operation_start` now uses canonical broker preparation, automatic
grant selection, live bearer/profile/scope refresh after waits, then the existing durable
admission transaction and bound broker Execute. A pending request, an unbound account key,
or a configured local IPC socket is not an account grant. Explicit non-account configured
authority remains separate. No request fields accept caller-selected principal identity.

`machines/access/broker/mod.rs` separates `prepare_start` and `execute_prepared` with
owned, non-serializable preparation bound to account/principal/grant/client/enrollment.
The latter still rechecks current domain authority before consuming a use. Selection in
`broker/selection/mod.rs` reads at most 100 candidates per SQL page, follows source
confinement/approval order with deterministic ID ties and prepares each encountered level
at most once. Original operation keys stay with their original grant, including exhausted
one-use grants. Malformed scope data fails instead of becoming unrestricted scope.

Local evidence before the module-layout move: five new tests passed with required real
PostgreSQL and no ignored tests (`target/absorption-start-selection-fixed.log`): actual MCP
revocation during Prepare causes zero uses/admissions/Execute; unapproved/unbound callers
refuse; transport uncertainty retains admission; exhausted retry affinity; 101-grant
pagination with one canonical preparation and different-client preparation refusal.
An initial borrow-check error was fixed before these passes. The 106 existing access
regressions then passed (`target/absorption-start-regression.log`). Additional real MCP
local-socket bypass coverage and the three MCP cases passed together (4 tests,
`target/absorption-start-mcp-final.log`). The pagination fixture takes about two minutes
because it creates/approves all 101 records through real domain methods; no test is skipped
as missing infrastructure. Both PostgreSQL required flags and isolated port55441 are used.

Layout initially rejected file/folder pairs. They were converted to normal `mod.rs`
modules without increasing caps; all eight layout tests and formatting passed afterward.
The same source inventory still verifies 100 commits/907 records. The per-source closure
option correctly refuses open S1/S2 and default full-queue closure; it does not close rows.

Still open: ordinary `run` and file/batch/image routes, input/cancel and historical output
authority, task/credential-filter-aware selection refinements, native confinement, policy
change review and the remaining A04 surface behavior. This is a local partial source port,
not a green complete S1, published PR, main merge, or branch-deletion receipt.

After the module-layout correction, the final access run passed 110 tests with zero
failed/ignored (`target/absorption-start-final-regression.log`). The two selection tests
were already passed on unchanged implementation and were explicitly filtered from that
rerun; six new cases plus 106 existing cases have passing local evidence. Final layout
passed all eight checks; formatting and inventory checks passed. No CI was dispatched.

## Ordinary run and admission-bound result reads, 2026-11-21

Compared the S1 process adapter and lease behavior in `server/internal/devapi/mcp_proc.go`,
`server/internal/machineops/proc.go` and `bridge/runner/proc.go`. `run` now uses the same
granted start dispatcher as `operation_start`; its wait/read and ordinary Get/Read/Wait
resolve the authenticated principal's original admission and enrollment. A separate
closed `GrantObservation` permits only observation. It cannot start, cancel, refresh a
process clock or consume another use. Native Execute reads no longer renew execution
as a side effect; explicit Refresh remains the execution-renewal owner.

Current `proc.read` policy, current registered identity and current task labels are checked
under the existing board/identity/machine/access lock order. AgentHub supplies current
labels for the original task generation even after release/reclaim, without fabricating a
live lease. Broker history requires the exact immutable admission. A paused/ended execution
grant does not itself hide history when current read permission allows it; policy denial,
another principal or changed enrollment refuses. The adapter refreshes bearer, machine and
project scopes both before and after broker waits, before returning any output. Long waits
use bounded observation slices and recheck permission between slices. A run-only key can
retrieve its own run result; separate operation-read tools still require machines:read.

Local validation (required isolated PostgreSQL on port55441):

- First compile found an incorrect test field path (`record.cancellation_requested`);
  corrected to `record.view.cancellation_requested`. No runtime failure was suppressed.
- `cargo test --locked` with all five affected library packages and filter `observation`:
  10 passed, zero failed/ignored (`target/absorption-observation-fixed.log`).
- Same package set, filter `access`, excluding the two previously passed unchanged
  `access_start_selection` cases: AgentHub7, hub114, native2 = 123 passed, zero failed/ignored
  (`target/absorption-observation-access-regression.log`). This includes all three new real
  MCP run tests and the original-task release/reclaim regression.
- Same package set, filter `remote`: AgentHub1, contracts3, hub10, link2, native12 = 28 passed,
  zero failed/ignored (`target/absorption-observation-remote-regression.log`). Filters overlap;
  these totals are not a claim of 161 distinct tests. Native tests launch actual processes,
  prove unchanged execution deadlines and termination at expiry, reject observation effects
  and changed bindings, and read original output after expiry/revocation.
- Generated contract check, formatting, AI contract check and all eight layout tests passed.
  All 100 source commits/907 records still verify against their preserved Git objects.

Only the three process rows were advanced from unverified to **partial**, with concrete
targets. Input/cancel/resize, heartbeat adapter and operator kill behavior, file/batch/image
routes, task/credential-aware grant selection, native confinement and policy-change review
remain open. No complete source closure, push, PR, CI, merge, branch deletion or deployment
is asserted by this local checkpoint.

## Admitted process input, resize and cancellation, 2026-11-21

The same S1 process source paths now map to action-bound `GrantControl`/`ProcessControl`
contracts, `machines/access/continuation.rs::operation_control`, the MCP control adapter,
and the existing native input/resize/cancel owners. Original admission lookup is shared
as `AdmissionTarget`; it is an identity snapshot, not permission. Current original grant,
principal, task generation, machine and policy are checked before `proc.write` or
`proc.kill` is authorized. The proof expires no later than the existing admission and
does not update its clock, heartbeat or use count. Input/resize require the original
interactive capability. The legacy Execute envelope no longer admits cancellation.

Broker effects verify the exact action, stored binding, current revocation floor and live
execution clock. Input requests carry a `ControlPermit` into the existing native queue.
The input worker checks it before each step and bounds pending I/O by its monotonic deadline.
An expired partial write reports uncertain earlier delivery; it never promises rollback or
safe blind retry. Local IPC retains its existing owner authority and input timeout semantics.
Input/resize/cancel implementations moved from the capped broker file into `broker/control.rs`;
they were not duplicated, and no layout allowance increased.

Local evidence uses the same five library packages and required isolated PostgreSQL:

- First control run: all eight hub cases and two of three native cases passed. The foreign
  admission test found the durable retry check preceded the exact operation-binding check
  (`idempotency_conflict` instead of `authority_mismatch`). The broker now checks the exact
  stored operation binding before the durable grant floor; no assertion was weakened.
- Corrected filter `control`: 16 passed, zero failed/ignored
  (`target/absorption-control-fixed.log`). Includes actual native pipe input, terminal
  resize, cancellation, closed control fields, current policy/paused grant refusals and
  real MCP cancel/unknown-operation/revoked-credential cases. The queue test captures a
  real constructed input request and verifies its carried check after expiry/revocation;
  it is not a claim of a full OS backpressure/partial-write probe.
- Filter `access`: AgentHub7 + hub119 + native2 = 128 passed.
- Filter `remote`: AgentHub1 + contracts4 + hub10 + link2 + native16 = 33 passed.
- Filter `interactive`: contracts1 + hub1 + native6 = 8 passed.
- Filter `lifecycle`: AgentHub5 + hub8 + native11 = 24 passed.
  All four regression runs had zero failures/ignored tests; filters overlap. Logs are
  `target/absorption-control-{access,remote,interactive,lifecycle}-regression.log`.
  The two previously passed unchanged selection cases remain explicitly filtered.
- Generated contracts, formatting, normal Git diff checks, AI contracts and all eight
  layout tests passed. Windows native behavior is proved here; Linux native behavior
  remains a required platform check before source publication.

Remaining process/source work includes the human-only cross-principal operator stop,
source heartbeat surface, list/history authorization, Linux blocked-input native probe,
task/credential-aware selection, file/batch/image routes and native confinement. The
three source rows remain partial. No S1 closure, push, PR, CI, main merge, deletion or
deployment is claimed.

The remaining Windows backpressure probe was then implemented and passed
(`target/absorption-control-backpressure-final.log`, one test, zero failed/ignored).
A real native child waits on a file gate before reading stdin. The fixture saturates the
native pipe with a large internal input step (the public 16KiB request cap is unchanged),
then forwards a second real control request carrying an expiring permit into the same
execution worker. Both replies come from that worker. After expiry the gate opens, input
closes and the child exits successfully. Every retained output page is read: partial filler
arrived, but the queued forbidden marker did not. This specifically covers actual blocked
I/O followed by an expired queued request; it does not claim rollback of earlier bytes.
The final test also checks the forwarded request identities and output cursor progress.
All eight layout checks passed again. Runtime code is unchanged from the prior green
regressions; only this additional native probe and its evidence were added.


### Task and credential-aware start selection (local implementation)

`broker/selection/eligibility.rs` resolves the original task board through the existing
registry and calls the shared `current_in` authority checks before and after broker
preparation. Reclaimed/ended task grants and explicit current-policy denials can be
skipped for a later eligible grant. Credential project limits filter unbound and other
project grants before contacting the broker. Original retries stay on their original
grant; an out-of-scope retry refuses rather than selecting another grant. No use or
admission is created by selection. Final admission retains its atomic authority checks.
Database failures, malformed snapshots and foreign task identities remain visible errors.

Validation: `target/selection-eligibility-regression.log`, five affected packages with
`--lib access -- --test-threads=4`, required isolated PostgreSQL enabled. AgentHub7 +
hub128 + native2 = **137 passed, zero failed/ignored**. This includes all nine selection
tests: 101-candidate paging and one preparation per level; original exhausted retry;
reclaimed task; credential project filtering and out-of-scope retry; task release during
Prepare with cached preparation; all candidates outside credential scope; malformed
grant; foreign task identity; database pool failure during eligibility. Actual MCP tests
assert the selected grant, zero rejected-grant uses and absence of extra broker calls.
The earlier seven-test focused selection run also passed. Formatting, generated contracts,
AI repository checks, all eight layout checks and normal Git diff checks passed.

This finishes the task/credential filter sub-unit for operation starts. Other capabilities,
file/batch/image dispatch, list/history, explicit heartbeat, human operator stop, native
confinement and the remaining S1 batches are still open. Source records remain partial.
Implementation and local validation exist; push/PR/main merge/deletion do not.


### Explicit process heartbeat (local implementation)

The source `devapi/mcp_proc.go::proc_heartbeat` and `machineops/proc.go::ProcOp` now map
to `operation_heartbeat(machine, operation_id)` in the existing hub operation router.
The original immutable admission replaces the source's separate process lease/fence.
The tool requires a current account credential, machines:run, project limits, the original
interactive grant and task generation, and exact `proc.heartbeat` policy on current task
labels. It uses the existing transactional continuation and authenticated broker Refresh,
without creating a process, consuming another use, renewing the task lease or reviving
expired authority. A broker wait is followed by fresh credential checks before renewal;
a revoked credential cannot receive the effect receipt. The returned contract contains
only operation ID/deadline, not reusable grant authority. Local configured IPC has no
hosted grant to renew; this is a hosted authority interface, not a new local capability.

Validation (required isolated PostgreSQL, Windows native broker):
- `target/absorption-heartbeat-focused.log`: all nine new MCP/domain tests passed.
- `target/absorption-heartbeat-access-regression.log`: AgentHub7 + hub137 + native2 =
  **146 passed, zero failed/ignored**, including prior selection, continuation, maintenance,
  task, grant, credential and cancellation regressions. Five affected packages, --lib access.
- `target/absorption-heartbeat-native.log`: 17 real native remote tests passed, including
  refresh deadlines, refusal after expiry, interruption persistence, actual pipe/terminal
  control and the blocked-input probe. Linux native execution remains separately required.
- Two actual router parity/catalog tests and one closed heartbeat receipt contract test
  passed. Generated contracts, AI repository checks, formatting, eight layout tests,
  normal Git diff checks and Python syntax compilation of the updated hub probe passed.
  The process-level hub_probe itself is not claimed run by these focused tests.

Actual MCP tests cover unchanged admission/use count, key revocation during capabilities,
revocation before receipt, mismatched broker acknowledgment, unknown operation, expired
authority and an explicit current-policy deny without changing the stored deadline.
Domain tests cover original task expiry/fence, foreign holder, reclaimed lease and
noninteractive admission refusal. Source records stay partial: human-only operator stop,
list/history, file/batch/image enforcement, native confinement and the other S1 batches
remain. No CI, push, PR, main merge, deletion or Docker update occurred in this unit.
