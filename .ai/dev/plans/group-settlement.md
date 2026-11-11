# A member that leaves the group does not hold settlement

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: CI on PR #175 (2026-11-11): `a_descendant_that_leaves_the_group_does_not_hold_settlement` took 3.09 s on ubuntu-24.04-arm.
- PR: not_created
- Branch / baseline: `increment-129/group-settlement`, from `main` at `688b35f`
- Objective: an operation settles when its group is empty, even if a member left it without exiting.
- Non-goals: tracking processes outside the group (SCOPE excludes them).
- Canonical architecture / related plan: none

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Test: a member that leaves after it is watched | none | fails without the fix (3.30 s) | done |
| M2 | Re-read the group at least every 100 ms while waiting on pidfds | M1 | 7 Linux tests pass, 3 runs | done |
| M3 | Merge to main | green CI | merged PR | pending |

## Risks and approved decisions

A pidfd is readable only when its process exits. A member that called `setsid` after being
watched held settlement until it exited: a daemon could hold an operation open indefinitely. The
fix costs one `/proc` read per 100 ms, only while members remain.

## Verification receipts

`rust:1.99.1-bookworm` container: the new test fails without the fix (3.30 s) and passes with it;
`cargo test -p devagotchi-core --lib execution::platform::linux` 7 passed, three runs.

## Delivery

Implementation: M1, M2. Validation: local container. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
