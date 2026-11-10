# ADR 0002: bound-board offline cache and explicitly pending operations

Status: accepted. Date: 2026-11-09.

## Authority and scope

The user selected client-initiated offline synchronization and explicitly allowed discovery,
new tickets, notes and drafts to queue, while requiring hub confirmation for shared claims
and completion. The user authorized direct recording of this ADR and its companion
[ADR 0001](0001-hosted-board-postgres-and-clickhouse.md), as a specific exception while
Jesse's ADR component is unavailable.

This supersedes D23/B11's blanket refusal of offline writes and packet 15c's read-only-only
scope. It does not permit offline ownership, execution admission or unconfirmed completion.

## Decision

- A bound project's Postgres hub is the sole authority. Local SQLite stores authorized
  cached snapshots/cursors and a durable pending-operation queue. Standalone local boards
  remain visibly distinct and never become a fallback shared board.
- When disconnected, allow discovery results, new-ticket proposals, notes and drafts to
  persist as **pending**, not as accepted server records. Show cache age/offline state and
  distinguish pending, acknowledged, conflicting and rejected operations in CLI/plugin/UI.
- Claims, claim-next, heartbeat/lease renewal, release/handoff, force, completion/review
  and other shared workflow/ownership mutations require a confirmed hub transaction.
  Cached leases/fences cannot authorize disconnected writes. A draft describing completion
  is not completed work and must not move the shared ticket.
- Reconnect is client initiated: authenticated push, explicit per-operation acknowledgments,
  then authorized cursor pull. No inbound client port, database-file mirroring or direct
  client database access.
- Assign a stable operation ID before persisting a queue entry. Retries reuse it. The hub
  checks the canonical payload hash, current permissions, server-derived tenant/actor and
  expected version. The same ID/content replays the prior outcome; changed content under
  the same ID is refused. Mutation, deduplication receipt and outbox commit atomically.
- Provisional local ticket identifiers remain visibly provisional. Acknowledgments map
  them to canonical records; dependent queued notes must not attach to an unrelated ticket.
  Never replace or delete a queue entry before its durable acknowledgment is recorded.
- Preserve rejected/conflicting user-authored drafts for explicit review/resubmission.
  Refresh permissions on reconnect; revoke access to cached shared data when no longer
  authorized. A stale snapshot cannot grant access or silently overwrite newer data.
- Pull uses an authorized ordered change cursor, not timestamps as a replay guarantee.
  Provide explicit cursor-expiry/resnapshot and permission-revocation behavior, plus
  documented queue/cache bounds and actionable errors.

## Delivery gates

Test restart durability, disconnect during push, lost acknowledgments, duplicate requests,
changed-payload reuse, concurrent edits, provisional-ID dependencies, cursor expiration,
revoked permissions and preserved conflicting drafts through the real bound CLI and hub.
Verify an offline claim or completion never reports success and queued writes never appear
acknowledged prematurely. Readers must label cached data at narrow and wide UI resolutions.

Acceptance changes the contract, not the current runtime: the durable cache/queue and sync
endpoints remain unimplemented until those receipts exist. Existing honest disconnected
refusals remain in place while the queued-operation implementation is built.
