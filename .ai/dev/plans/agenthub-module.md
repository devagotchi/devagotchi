# AgentHub in one place

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: "agenthub needs to be its own clear submodule and nested
  folder and code".
- PR: not_created
- Branch / baseline: `increment-131/agenthub-module`, from `main` at `875b970`
- Objective: everything AgentHub is found under one name: the engine in `crates/hub/src/agenthub/`,
  its phone screens in `crates/hub/src/web/app/agenthub/`.
- Non-goals: a separate crate. The board uses the hub's store, tenancy, orgs, preferences,
  signals and warehouse throughout (64 references the other way too); a crate comes after those
  are their own crate.
- Followed by: [AgentHub as its own crate](agenthub-crate.md) (increment 137): the engine is now
  `crates/agenthub` on `crates/store`; this folder keeps the hub's side of it.
- Canonical architecture / related plan: [agent-board](../../../docs/agent-board.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | `board/` and `issue_lifecycle` become `agenthub/`; `crate::board` is `crate::agenthub` | none | 523 hub tests | done |
| M2 | Board, ticket and ticket-action screens in `web/app/agenthub/` | M1 | same | done |
| M3 | Living docs and comments name the new paths | M1, M2 | grep | done |
| M4 | Merge to main | green CI | merged PR | pending |

## Risks and approved decisions

Layering: the engine never depends on the web. `web/approvals.rs` stays shared, since a dozen
pages use its same-origin check and approvals also cover machine operations. Tool names
(`board_*`), the `board` command line and stored data are unchanged.

## Verification receipts

`cargo test -p devagotchi-hub -- --test-threads=8` on Postgres 18 and SQLite: 523 passed. Warnings
unchanged from main (23).

## Delivery

Implementation: M1 to M3. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
