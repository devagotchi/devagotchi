# Project lifecycle source comparison

S1 `46419c697964016eb547c60416a8f2f5e854a95f`, read from the pinned Git objects:
`server/internal/store/admin_config.go::PutProject`,
`server/internal/store/admin_misc.go::DeleteConfig`, and
`server/internal/httpapi/admin.go::adminRoutes/configRoutes`.

The source creates/updates name and workflow with optimistic versioning and request
receipts. Removal refuses any referenced task, including completed tasks. The current
Rust project row anchors append-only events, task data, workflow publication history
and receipts. Physical cascading deletion would lose that evidence; migration 0007
retains removed generations and makes owner/address uniqueness apply only to active
projects. This realizes the lifecycle decisions already recorded in implementation.md.

## Authored owners and adapters

- `agenthub/src/hosted/projects/`: one serializable transaction creates metadata,
  defaults, optional workflow binding, event and receipt. Edits use metadata versions;
  workflow changes invalidate those versions. Empty project removal preserves immutable
  history and refuses all tasks and catalog definitions anchored there. Every mutation
  checks current person, owner and policy before receipt replay.
- `hosted/projects/resolve.rs` resolves active owner/address to its physical identity;
  a recreated address gets a new identity. `hosted/mod.rs::scope` rejects retired handles.
  Hub `agenthub/{registry.rs,mcp/boards.rs}` resolves and evicts cached retired handles.
  Onboarding's existing welcome path remains separate from explicit empty creation.
- `identity/accounts/agents/directory.rs` owns the active project-key guard in the same
  database transaction. A live narrowly scoped key must be revoked before removal;
  account-wide keys keep their documented coverage of future projects. No board-table
  query was added to the identity owner.
- `agenthub/mcp/projects.rs` exposes internal Hub methods for both adapters. This file
  does not register new MCP administration tools: pinned source registrations did not
  contain them. HTTP `agenthub/http/projects.rs` and browser
  `web/app/agenthub/projects/` call these owners, preserving retry identity and authority.
- Browser forms expose active/removed lists, empty creation, display name and workflow
  editing, guarded removal and retained history; site copy is localized. Submitted
  drafts/keys remain on version errors. Exact-origin checks protect browser mutations.
- Retired history is owner-only, by physical generation, bounded to 100 events per page.
  Active/retired project lists use stable owner-scoped address/identity cursors.

## Validation status

Authored real HTTP/PostgreSQL scenarios in `http/tests/projects.rs`: concurrent same-key
create, rename/version conflict, removal replay, name reuse with new identity, cached
retired handles, preserved history, old-key conflict, atomic failed workflow selection,
shared-definition edits invalidating metadata versions, audit-anchor and completed-task
refusal, project-key guard, browser/API replay, escaped forms, draft preservation,
origin refusal, separate users with the same address, foreign archive and seat refusal.
Migration tests cover adoption, active uniqueness, multiple generations and rerun.

First compile found the event-vocabulary array length still declared 28 after adding
two project actions; corrected to 30. The subsequent focused command passed:

`cargo test --locked -p devagotchi-hub -p devagotchi-agenthub --lib -- agenthub::http::tests::projects migrations::tests tests::shared_workflows tests::configuration --test-threads=2`

Nineteen AgentHub and eleven hub tests passed, zero failed/ignored, with required
PostgreSQL enabled. Build: 1m55s; execution: 6.37s + 13.81s. Log:
`target/absorption-project-lifecycle.log`. All eight layout checks passed; inventory
still verifies 100 commits/907 records against Git objects, all open. Aggregate config,
remaining source operation review, full integration/visual acceptance and main delivery
remain outstanding. This local result does not close a source or authorize its deletion.
