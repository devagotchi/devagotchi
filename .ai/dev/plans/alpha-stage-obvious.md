# Make alpha = pre-release obvious, and undo the tagged-release default

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-10: the alpha = pre-release toggle must be obvious in `.ai`.
- PR: not_created
- Branch / baseline: `increment-122/alpha-obvious` from `79758ec`
- Objective: an agent opening `.ai` or `AGENTS.md` sees alpha = pre-release and how to work under it.
- Non-goals: changing the stage; changing what `update.sh` does when a deploy fails.
- Canonical architecture / related plan: [DG-LIFECYCLE-1](../../docs/lifecycle.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Banner at the top of `.ai/README.md` and `AGENTS.md`, rule in the lifecycle | none | `devagotchi ai check` | done |
| M2 | Installs follow `:main` again | none | shell syntax check, deploy test in CI | done |
| M3 | Review and merge to main | M1, M2, operator approval | merged PR | pending |

## Risks and approved decisions

Increment 121 made `:latest` the install default, which is a production release lane and has
nothing to pull while no release is tagged. This restores `:main`. Increments 120 and 121
shipped without a plan here; this file records that gap.

## Verification receipts

`devagotchi ai check` and `bash -n` on the changed scripts, run locally before pushing.

## Delivery

Implementation: done. Validation: local checks only. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
