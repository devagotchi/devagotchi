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
| create_task, update_task | AgentHub hosted/tickets.rs | Current policy then receipt; mutation guards only on fresh writes; include board/id/content/version/fence in identity |
| claim, claim_next | hosted/leases.rs | Identity across boards; replay original lease clearly marked as replay, never silently renew it |
| heartbeat, release, complete, handoff, force_release | hosted/leases.rs | Current authority before replay; preserve original outcome and same-transaction events |
| add_note, add_dep | hosted/notes.rs, graph.rs | Current resource labels, exact note/dependency payload and one event |
| put_workflow, put_project, put_setting | hosted/settings.rs, shared_workflows/; identity agents configuration | Versioned policy checks and resource scope; shared definition changes retain all-board checks |
| put_role, put_policy, put_principal, revoke_token | identity/accounts/agents/, api_keys/ | Compose live manager policy, identity references, audit and receipt; no credential issuance replay |
| delete_ + dynamic configuration kind | source admin_misc.go dispatch, owning catalog | Expand every supported kind; exact version/body identity, refuse active references |
| exported Store.Write callers | source store/export.go and domain call sites | Enumerate machine/catalog/grant/guide mutation calls in A04-A08, retain transaction/outbox boundaries |
| create_token, seed, reap | credential issuance and internal lifecycle | Source deliberately supplies no key; do not introduce secret replay or client-selected reaper identity |

MCP `request_id`, REST Idempotency-Key/body precedence, CLI retry options and browser form
request ids still need typed adapters, bounded input and replay indication. Source dynamically
registered tools/routes must be expanded; SSE reconnect/authorization/backpressure remains
in this packet. All original source rows stay open until operation-specific evidence and
main delivery exist.

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

Still open: configuration/identity mutations, REST, CLI and
browser retry adapters, SSE, archive/retention integration, source event request-id parity,
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

Next compatibility case to resolve: externally verified provider/dev subjects with no local
account row currently fail receipt Directory checks. Their ordinary calls still work; retry
support must preserve their transport-verified identity without treating them as a missing
local account. Local acct_ and registered @agent: namespaces must continue to fail closed.
This is not covered by the account-based MCP retry tests above.
