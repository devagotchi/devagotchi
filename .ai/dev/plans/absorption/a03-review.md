# A03 mutation receipts and surface comparison

Source S1 `46419c697964016eb547c60416a8f2f5e854a95f`; work remains open. No CI,
push, PR or source deletion. This document separates the shared engine from its callers.

## Source semantics and corrections

Read `server/internal/store/store.go` directly. Source keys are principal + request id,
shared across operations. The operation and JSON body determine a hash; a repeated matching
key returns the saved response, a mismatched one refuses, and a mutation and its event share
the receipt transaction. Source admin token issuance explicitly supplies an empty key.

The new PostgreSQL store owns `store_request_receipts`, with `_store_migrations` applied
AFTER caller migrations (the hub creates its restricted role first). Request identities
bind tenant + canonical authenticated principal + key; operation and canonical JSON body
are hashed, including the resource identifiers the adapter must supply. JSON object order
is normalized, array order preserved; keys, principal/operation, body nesting and response
size are bounded. Request contents are not stored, only their hash and the nonsecret result.

Reservation, domain mutation, audit/outbox and completion must share one serializable
transaction. A deferred database constraint refuses a forgotten completion and rolls back
its domain mutation. A concurrent identical reservation waits/retries and replays one result.
Completed identities/results cannot be rewritten or deleted; retention removes response
bodies in bounded pages while leaving permanent retired keys. No automatic retention runs;
A08 must archive where required before explicitly retiring responses.

Unlike the source's replay-before-operation-policy order, current account/profile and base
resource authority are checked before receipt access. Where an operation's authorization
includes its original state (for example ready -> blocked), its receipt retains those exact
action names and rechecks them against current policy and current labels before returning
any result. The changed state cannot reconstruct the original action. No receipt is exposed
before all these checks. The engine does not manufacture authorization: adapters compose it.
Secret issuance remains excluded. External effects require a transactional outbox, not an
external call inside the retry loop. No generic receipt API can prove a caller followed
those obligations; the surface integration tests remain required.

## Authored and checked locally

- `crates/store/src/receipts/{mod,request,tests}.rs`, additive migration, build tracking.
- Store connection bootstrap moved by concern to `opening.rs`, bringing the former oversized
  `lib.rs` under the cap. The size baseline only removes its old entry.
- Hub table-prefix and migration-history expectations include the store owner; standalone
  AgentHub expectations distinguish its board tables from the store receipt table.
- Five focused PostgreSQL checks passed in `target/absorption-receipt-engine-rls.log`:
  concurrent duplicate single mutation; reopen/replay; cross-operation/body conflict;
  authorization-before-replay fixture; rollback before commit; deferred incomplete receipt
  rolling back its mutation; oversized response rollback; separate principal/environment
  keys; permanent retirement; database protection against tombstone deletion/reuse; actual
  restricted-role RLS read/write isolation; canonical identity bounds; embedded SQL bytes.
- Hub (10) and standalone AgentHub (5) migration integration checks passed in
  `target/absorption-receipts-{hub,agenthub}-migrations.log`. Domain/surface wiring is not yet
  complete; the engine test's authority fixture alone is not end-to-end authorization proof.

## Operation wiring queue

Read source `s.write` calls in ancestry-pinned `store/`. Port these into the owning domain
transaction and expose the optional request id through all supported adapters:

| Source operation | Owning Rust implementation | Work still required |
|---|---|---|
| create_task, update_task | hosted/tickets.rs, updates.rs; MCP tools; browser creation/actions | REST adapters and complete source surface mapping; CLI retry continuation below |
| claim, claim_next | hosted/claims.rs; MCP lease tools | REST retry contracts and remaining source surface mapping; CLI retry continuation below |
| heartbeat, release, complete, handoff, force_release | hosted/leases.rs; MCP lease tools | REST retry contracts and remaining source surface mapping; CLI retry continuation below |
| add_note, add_dep | hosted/notes.rs, graph.rs; MCP tools; browser comments | REST and browser dependency surface mapping; CLI retry continuation below |
| put_workflow, put_project, put_setting | hosted/settings.rs, shared_workflows/; MCP project settings; browser configuration | Arbitrary source setting/catalog equivalence, CLI/REST/admin MCP adapters |
| put_role, put_policy, put_principal, revoke_token | identity/accounts/agents/, api_keys/revocation.rs; browser agents/keys | Remaining principal semantics and REST/admin MCP mappings; no credential issuance replay |
| delete_ + dynamic configuration kind | role/rule and shared workflow deletes composed with identity/board receipts | Project deletion and other dynamic source kinds remain open; expand every branch |
| exported Store.Write callers | source store/export.go and domain call sites | Enumerate machine/catalog/grant/guide mutation calls in A04-A08, retain transaction/outbox boundaries |
| create_token, seed, reap | credential issuance and internal lifecycle | Source deliberately supplies no key; keep secret issuance and internal reaping outside client receipt APIs |

Ticket/lease MCP and the authored browser mutation forms accept request identities; REST
Idempotency-Key/body precedence, CLI retry options, dynamic administrative registrations,
SSE reconnect/authorization/backpressure and the remaining domains are still open. All
source rows stay open until operation-specific evidence and main delivery exist.

## Domain and MCP wiring in progress

Create, update, add-note, add/remove-dependency and all seven lease mutations reserve/complete inside
existing board transactions. The request-local board copy shares the normal notification
subscription; no cached registry board acquires a retry key. Identity includes environment,
canonical authenticated principal, operation, internal board id, acting seat/human status
and the complete typed adapter input. The adapter captures input before filling session,
routing and remembered-fence defaults, so reconnects cannot change retry identity.

The host's identity directory checks the account/profile on the same transaction before
reading a receipt, and refuses a stale kind/role snapshot. AgentHub contains no auth SQL.
Create replay additionally checks policy against the created ticket's current labels.
Update replay checks base edit authority plus the original transition/edit-held actions;
only fresh writes run version/fence mutation guards. Notes and graph changes check current
resource policy before replay. Events and outbox rows remain in the mutation transaction.

MCP board_create, board_update, board_note, board_dep and all lease tools accept request_id and
return request_id/replayed beside the normal result fields. A note replay does not issue a
second recipient wake. Explicit null progress and omitted progress have distinct identities.
The original source HTTP precedence is a nonempty Idempotency-Key header before body
request_id, with Idempotent-Replayed: true on replay; REST is still pending.

Local evidence so far:
- Store scope regression: 5 passed, target/absorption-receipt-scope.log. This includes a
  restricted writer switching RLS scope before commit: the privileged, fixed-search-path
  deferred trigger still rejects its unfinished receipt and rolls back.
- Board domain: 4 passed, target/absorption-board-receipts-update.log. Concurrent duplicate
  create has one event/outbox entry, replay returns the original ticket after later edits,
  note/dependency replay does not write again, failed mutation frees its reservation, keys
  bind principal/body/operation, and revoked original transition policy refuses replay.
- Existing MCP ticket/schema regressions: 2 passed, target/absorption-mcp-receipt-adapters.log.
- All six board receipt cases passed in target/absorption-lease-receipts.log, including
  original expiry/fence replay after a later claim, one handoff/summary/force-release note,
  and claim-next replay of a selected task or empty result without taking another task.
- All 14 MCP cases passed in target/absorption-mcp-lease-receipts-fixed.log, including
  reconnect, null/omission, two credentials for a registered agent, live revocation,
  schemas and original receipt flags without overwriting/forgetting newer fence memory.
  An earlier contract check found a missing receipt error next-step; it was added. A tool
  extraction initially duplicated operation text in macro attributes; compilation caught
  it and the attributes were corrected before this passing run.

Still open: remaining configuration/identity source semantics, REST, CLI and
unmapped browser mutations, SSE, archive/retention integration, source event request-id parity,
all remaining source accounting and final main delivery. This is not A03 or S1 completion.

Lease adapters capture typed input before resolving implicit fences. After a completed
release/completion forgets the cached fence, the same request can still reach its receipt;
a fresh request without a usable fence fails the ordinary guard with an impossible sentinel.
A replay never writes or clears fence memory. Claim-next rechecks the saved task's original
claim action against current labels before returning it; an empty result is replayed empty.
Heartbeat/release keep the source's live-principal/held-fence authority contract rather than
inventing a new policy action. Fresh claims now check current policy even when already held.
The source-dependent claim and completion action names are saved beside their responses.

Checkpoint validation additions: CLI output tests had six passes and one missing
not_granted recovery message; after adding the message, that specific contract test passed
(target/absorption-receipt-cli-refusals-fixed.log). All eight layout checks passed
(target/absorption-receipt-layout.log); the old oversized store/lib.rs entry was removed,
never increased. The real bound CLI heartbeat test passed
(target/absorption-cli-heartbeat-progress.log): omitted progress survives, explicit null
clears it. Remote heartbeat now omits an absent progress field. Formatting, diff whitespace
and the pinned source inventory pass; the inventory still has all 100 commits/907 rows open.

Provider/dev subjects verified on the current transport request now retain retry support
without a local auth_accounts row. The fallback excludes local acct_ and registered @agent:
namespaces and profile snapshots. The real HTTP/MCP provider case passes (1 test,
target/absorption-provider-receipts.log): reconnect replays create and project settings,
keeps later settings intact, and refuses a request key on a settings read.

Configuration domain receipts cover settings, private workflows, project metadata, shared
workflow publish/bind/edit/detach/delete. Shared edits reauthorize the union of original and
current affected boards before replay, including a board detached since the original edit.
Two PostgreSQL cases pass (target/absorption-configuration-receipts.log). MCP board_settings
exposes project metadata retry; the browser adapters are still being authored. Account
profile, OAuth binding, role, policy and principal settings receipt adapters are authored
locally and their focused checks are in progress. None of these changes closes S1/A03.


## Configuration, account and browser continuation

Locally authored account APIs compose live manager checks, policy, mutation, tenant audit
and receipt in the existing serializable transaction. Roles/rules support versioned put
and delete; settings, agent profiles and stable OAuth bindings return their original
nonsecret metadata. Replay skips fresh version/reference checks, but repeats current live
account and policy checks. Ordinary users still may only rename their own assigned profile;
a replay of that rename after a later authority edit returns its old result without
restoring the old kind/roles/enabled state. Revocation receipts contain JSON null, never an
issued/rotated key secret. Disabled/expired accounts cannot replay revocation.

Browser forms carry one random request id per form, preserve it through validation and
workflow draft controls, and issue a new id after a successful redirect/reload. This covers
agent profiles, OAuth bindings, principal configuration, API-key revocation, task creation,
comments/assign/delegate/close/undo/hand, board settings/private workflows, and shared
workflow publish/bind/edit/detach/delete. The ordinary authorization/origin checks still
run. Shared edit replay can reach its saved result after deletion without recreating the
definition. Close forms retain the original state for the undo redirect; a replay cannot
replace it with the ticket's later state. Comment/delegation replay does not wake twice.
Board binding and other not-yet-mapped domain mutations remain open.

Delegation and undo now use same-transaction receipts. Undo retains the original transition
action and rechecks it with current policy/labels before replay; later closes are not
reopened by an old retry. All five mutation receipt cases passed in
target/absorption-delegation-undo-receipts.log.

Validation notes (not final acceptance): the initial identity run had two passes and a test
fixture collision with the seeded reviewer role; the fixture now uses a unique role name.
The next agent run had 13 passes and a form selector incorrectly including sign-out; its
selector now targets configuration forms. A broader browser run had 22 passes, including
both new board/workflow retry cases, and six setup failures from PostgreSQL's connection
limit. The affected checks are being rerun with two test threads. Compilation also caught
a test treating an OAuth Grant as a Subject and task creation trying to serialize the
non-wire NewTicket type; it now uses the actual Grant.account and typed browser form.
Logs distinguish these failures from passing evidence. No CI or deployment was run.


The bounded rerun passed: 8 account/browser receipt tests
(target/absorption-configuration-receipts-browser-fixed.log), both key tests
(target/absorption-key-revocation-receipts.log), 11 board/shared workflow configuration
regressions (target/absorption-browser-configuration-regressions.log), and 6 registered
agent browser regressions (target/absorption-browser-agent-regressions.log). The eight
layout checks passed (target/absorption-configuration-receipt-layout.log). The source
inventory still verifies all pinned objects and keeps all 100 commits/907 rows open.

Browser redirects now expose Idempotent-Replayed: true only on replay, including account
configuration, key revocation, task creation/actions and workflow/settings forms. Their
header regressions and the non-test hub build are being checked. Non-keyed convenience
methods that are now used only by fixtures compile only under cfg(test); the production
entry points accept an optional request key. No warning is suppressed to hide unused code.


Header checks passed: 8 receipt cases (target/absorption-browser-replay-headers.log) and
2 key cases (target/absorption-key-replay-headers.log). After removing the unused import,
cargo check -p devagotchi-hub completed without compiler warnings
(target/absorption-configuration-receipt-check-clean.log). The existing PostgreSQL key
lifecycle/isolation regression passed using the just-built test executable
(target/absorption-key-revocation-regression.log); only that import changed since its build.
The just-built layout executable also passed all eight checks against the current files
(target/absorption-configuration-receipt-layout-current.log). Formatting and diff whitespace
checks passed. These are local checks, not CI or source absorption into main.


## CLI retry continuation

Source comparison and exact contract: [CLI retry handoff](a03-cli-retries.md), covering
S1 server/cmd/hub/{main,client,run,state}.go request-id and cached-fence behavior. The source
files contain other CLI behavior too; these mappings are partial, not source closure.

Single task/project-settings writes generate a request key or accept --request-id. Before
sending, cli/requests persists and locks bounded original input plus resolved session/fence
under the ignored project requests directory. Storage/account secrets are excluded. Its
namespace includes authenticated hub principal (whoami), board and acting seat, or the
standalone PostgreSQL board/actor. Corrupt, mismatched and unwritable preparation refuses.
Matching retries recover original defaults after process restart, claim-cache removal or a
later claim. Absence of a cached fence is preserved too; command adapters cannot substitute
a newer one. PostgreSQL still owns the result and same-transaction mutation receipt.

Board and Remote expose request-local replay status. The CLI does not rewrite the claim
cache on replay. Human output identifies the original result; JSON retains object fields
and adds request_id/replayed, wrapping array results under result. Bound writes require the
hub's whoami capability, send the key only to the prepared mutation, and validate the
response's echoed key/replay boolean. An unexpected mutation is refused before transport.
Reads and batches reject an explicit key; this does not claim batch/lifecycle replay.
README documents the retry protocol and sensitive local record retention.

Evidence so far: 18 existing CLI tests passed before added history cases
(target/absorption-cli-retry-regressions.log). Three PostgreSQL history cases passed
(target/absorption-cli-retry-history.log). The complete focused CLI domain then passed all
30 checks, including simultaneous invocations, torn preparation, unsupported peers and
malformed acknowledgements (target/absorption-cli-complete-regressions.log). The real HTTP
hub case passed after correcting its assertion to account for the board-binding event
(target/absorption-cli-bound-retries-fixed.log); it reconnects for every invocation, keeps a
newer lease/cache, distinguishes omitted/null progress and refuses invalid credentials.
A separate live API-key revocation/replacement check is being verified. Its initial compile
caught the fixture trying to clone Accounts rather than its Arc; the fixture now shares
Arc<Accounts>, as the application does. No CI, push, source closure or deployment.


The live-credential check passed alongside the reconnect regression (2 tests,
target/absorption-cli-bound-revocation-fixed.log). The hub uses its actual local account
and API-key verifier. Revoking a key prevents replay; a different valid key for the same
account returns the original task without another event. Both key secrets are absent from
local request records. The two output-schema tests passed
(target/absorption-cli-capability-schema.log), all eight layout checks passed
(target/absorption-cli-retry-layout.log), and cargo check -p devagotchi-hub passed without
compiler warnings (target/absorption-cli-retry-check.log). The schema/layout checks used
the current built executables, with the layout executable reading current source files.
Formatting, both repositories' diff-whitespace checks and pinned-object inventory passed.
The 100 commits/907 source rows remain open; four CLI rows now carry partial source mappings.
