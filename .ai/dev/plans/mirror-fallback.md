# Caddy's install when a package mirror fails

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: CI on PR #174 (2026-11-11): `host-install (ubuntu-24.04)` failed twice, 15
  minutes apart, at `apt-get install caddy`: `Temporary failure resolving 'azure.archive.ubuntu.com'`.
- PR: not_created
- Branch / baseline: `increment-130/mirror-fallback`, from `main` at `688b35f`
- Objective: CI does not fail on the runner's mirror; a person whose mirror fails is told what to do.
- Non-goals: vendoring Caddy.
- Canonical architecture / related plan: none

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | CI: Ubuntu's own archive when the Azure mirror does not resolve | none | `host-install (ubuntu-24.04)` | done |
| M2 | Installer: apt retries, its error visible, a sentence on what to do | none | shellcheck | done |
| M3 | Merge to main | green CI | merged PR | pending |

## Risks and approved decisions

The CI step changes only the runner's mirror list, and only when the Azure name does not resolve;
the installer's own apt path still runs, so its coverage is unchanged.

## Verification receipts

shellcheck (koalaman/shellcheck:stable) on `deploy/linux/install-devagotchi.sh`.

## Delivery

Implementation: M1, M2. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
