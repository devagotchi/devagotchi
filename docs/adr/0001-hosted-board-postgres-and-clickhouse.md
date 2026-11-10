# ADR 0001: authoritative hosted boards in Postgres, analytics in ClickHouse

Status: accepted. Date: 2026-11-09.

## Authority and scope

The user explicitly selected the existing Postgres storage/access boundary for hosted
boards and required ClickHouse in every hub deployment. The user then authorized writing
the two ADRs directly and updating the conflicting architecture documents because Jesse's
ADR component is unavailable in this environment. This is that specifically authorized
exception, not a change to the recording rule for unrelated decisions.

This supersedes the hosted-storage portions of B01, B02 and packet 15a. It amends D23/B11
without changing the hub's coordination-only responsibility or the broker's ownership of
execution. [ADR 0002](0002-bound-board-offline-cache-and-queued-drafts.md) owns offline behavior.

## Decision

- The existing hub `Store` and its Postgres database own hosted boards alongside accounts,
  tenants and authorization. Board rows use tenant/project-scoped keys and foreign keys,
  transaction-local tenant scope and forced RLS under the runtime role.
- Account/org identity and shared-board grants continue to determine access through one
  server-side resolver. Request payloads and local directory names never confer authority.
  MCP, web, connectors, plugins and the bound CLI must resolve the same board of record.
- Preserve the existing ticket/workflow contract: lease TTL, server time, monotonically
  increasing fences, version checks, readiness, WIP limits, distinct reviewers, notes,
  handoff, append-only events and verifiable hash chains. Shared mutations, their events,
  source/request deduplication and analytics outbox records commit atomically.
- SQLite remains authoritative for explicitly standalone local boards only. A hub-bound
  client's SQLite is a cache/pending-operation store, not an independent shared writer.
  An unavailable Postgres must not silently route hosted work to a local board.
- ClickHouse is a required self-hosted stack service, not an optional cloud dependency.
  It serves statistics/tracking, not authentication, ticket arbitration or relational
  transactions. A durable transactional Postgres outbox feeds it using stable event IDs.
  Delivery is at-least-once; analytics must deduplicate explicitly.
- Expose export failures, pending counts and oldest-event lag. Require analytics
  initialization/readiness checks; runtime outages have a visible degraded state and
  documented bounded backpressure, never discarded events or success-shaped failures.
  Neither browser clients nor agents receive direct database access.
- Do not add Dragonfly without measured need. It replaces neither relational transactions
  nor analytical storage.

## Migration and delivery gates

Existing SQLite boards require explicit, backed-up import into an existing tenant.
Preserve identifiers, fields, active leases/fences, settings/workflow and event history.
Verify all copied values and chain heads; commit rows, immutable receipt and outbox together.
Identical retries have one effect; different snapshots cannot overwrite an imported board.
Retain the source until cutover, backup/restore and recovery are independently verified.

The `0014_boards.sql` migrations and `hub import-board` satisfy the staging foundation.
They do **not** make Postgres the live backend. Cutover requires all board operations,
the shared resolver, account/org/grant checks, MCP/web parity and concurrent-worker tests
on real Postgres. ClickHouse provisioning, export, deduplication, lag/backpressure and
backup/restore need their own receipts. Acceptance of this ADR is not implementation proof.

## Consequences

Standalone CLI use remains independent of Postgres and ClickHouse. Hub operators run both
databases locally or provide equivalent managed endpoints; no commercial account is needed.
Existing local file paths remain supported for standalone work and migration sources.
Hosted deployment must no longer depend on a per-project SQLite file after verified cutover.
No automatic destructive migration or file deletion is authorized.
