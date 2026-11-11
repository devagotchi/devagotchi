# License and a public image

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: BSL, then AGPL; Licensor AngryVibes LLC; production
  self-hosting free, no hosted service for others; pulling the image like any user would.
- PR: not_created
- Branch / baseline: `increment-127/license`, from `main` at `688b35f`
- Objective: anyone can lawfully self-host the alpha, and pull it with no login.
- Non-goals: the private cloud repository (its home is the operator's choice).
- Canonical architecture / related plan: [editions](../../docs/editions.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | `LICENSE`: BSL 1.1 text from SPDX, parameters from the operator | none | file | done |
| M2 | Crates `BUSL-1.1`; README, landing FAQ, llms.txt, editions say it | M1 | `cargo deny` | done |
| M3 | Package public (operator, in GitHub) | none | anonymous token 200 | done |
| M4 | Each publish pulls the image with no login and runs it | M3 | Release run | pending |
| M5 | Merge to main | green CI | merged PR | pending |

## Risks and approved decisions

BSL 1.1 caps the change date at four years; the operator chose four over a custom six-year text.
Change License AGPL-3.0. Not legal advice: a lawyer should read the Additional Use Grant before a
paid offering depends on it.

## Verification receipts

Anonymous GHCR token 200 and `:main` manifest 200 on 2026-11-11; the test hub took the newer
image on its own timer (`/api/v1/connectors` answers).

## Delivery

Implementation: M1, M2, M4. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
