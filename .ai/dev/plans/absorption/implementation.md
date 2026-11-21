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
