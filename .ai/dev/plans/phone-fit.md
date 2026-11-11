# The landing page and sign-in on a phone

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: make sure the alpha is truly ready.
- PR: not_created
- Branch / baseline: `increment-128/phone-fit`, from `main` at `688b35f`
- Objective: a person on a phone reads the landing page without sideways scrolling and types in
  real fields on the sign-in page.
- Non-goals: redesigning either page.
- Canonical architecture / related plan: none

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | `dl.facts` only: the connector page's grid no longer turns the landing page's list into a 962 px grid | none | `tests/app_browser.py` step 0 | done |
| M2 | `.split` columns `minmax(0, 1fr)` so the terminal scrolls inside its box | none | step 0 | done |
| M3 | Email and password fields full width, 44 px, 16 px text; the current tab marked | none | step 1 | done |
| M4 | Merge to main | green CI | merged PR | pending |

## Risks and approved decisions

Found on the live test hub at iPhone 13 size on 2026-11-11: the landing page measured 992 px wide
at 390 px (1703 px at 1440 px), and the sign-in fields 177 x 21 px.

## Verification receipts

`cargo test -p devagotchi-hub --lib -- sign_in landing site web::tests`; the browser checks run in CI (`e2e`).

## Delivery

Implementation: M1 to M3. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
