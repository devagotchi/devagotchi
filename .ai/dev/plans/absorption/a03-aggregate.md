# Aggregate configuration source mapping

Pinned S1 `server/internal/httpapi/admin.go::adminRoutes` registers GET `/api/v1/config`.
It combines workflows/projects/roles/principals/settings/policies/state kinds, but drops
several read errors. Current `agenthub/http/configuration/aggregate.rs` restores those
sections through their existing owners and fails the request if any section fails.

The prerelease API requires a selected account project and environment-owner human
authority. The whole environment roster/policy catalog is not exposed merely because
someone owns one board. Shared/org grants do not provide a complete owner namespace.
Board and principal settings remain explicitly separate typed/versioned records.
Potentially large project and principal sections have independent bounded pages/cursors;
the result does not claim a single transaction across independent owners. See the
public HTTP document for exact field names and query parameters.

`http/tests/aggregate.rs` checks correspondence to individual routes, malformed/missing
scope, invalid cursors, absence of partial success, agent/seat refusal, and ordinary
user refusal even when they own a same-named board. `http/tests/project_races.rs` adds
the planned task-create/removal and project-create/removal races: committed work cannot
be orphaned, old/new identities stay distinct, and an old retirement receipt cannot act
on the replacement generation.

Focused command:
`cargo test --locked -p devagotchi-hub --lib -- agenthub::http::tests::aggregate agenthub::http::tests::project_races --test-threads=2`

All four tests passed with required PostgreSQL, zero failed/ignored. Build 1m25s;
execution 11.71s; log `target/absorption-configuration-aggregate.log`. These results
supplement the preceding 30 lifecycle/configuration checks. Source batch and main
delivery remain open; no CI, push, PR, merge or source deletion occurred.
