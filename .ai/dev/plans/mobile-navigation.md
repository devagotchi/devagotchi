# The phone app's navigation: swipe between tabs, a Go to sheet

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: a tailored mobile experience with swipes and a popup
  overlay that swipes, to travel effortlessly to AgentHub, Overview and the other tabs.
- PR: not_created
- Branch / baseline: `increment-132/mobile-navigation`, on `increment-131/agenthub-module`
- Objective: move anywhere in the app with one gesture or one tap.
- Non-goals: a native app; offline beyond what the service worker keeps.
- Canonical architecture / related plan: [spec-phone-app](../../../docs/spec-phone-app.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Tabs Overview, AgentHub, Approvals, Machines, You (AgentHub second) | none | `web/app/tests.rs` | done |
| M2 | Swipe left or right on a tab's page to the next or previous tab, sliding that way | M1 | probe, `tests/app_browser.py` | done |
| M3 | Go to sheet: button and swipe up on the tab bar; sections that swipe; drag down, Escape, close | M1 | probe, `tests/app_browser.py` | done |
| M4 | `/app/focus`: the person's boards, machines, owner flag; nobody else's | M3 | `web/app/tests.rs` | done |
| M5 | Merge to main | green CI | merged PR | pending |

## Risks and approved decisions

From research (2026-11-11): WCAG 2.5.1 requires a tap for every gesture, so the tabs, the Go to
button and the section chips stay; iOS takes swipes from the screen's edge for Back in a
standalone app, so swipes start at least 24 px in; swipes never start in something that
scrolls sideways (tables, code, `[data-no-swipe]`). Listeners are on the document so a second
swipe during the slide-in still counts. `reveal.js` runs in the head so a page slides from the
side it was reached from.

## Verification receipts

Probe on an emulated iPhone 13 (Chrome) against `hub serve` on Postgres 18, 13 checks: tab order,
no sideways scroll, swipe left twice and right twice, edge swipe ignored, short drag settles,
sheet opens on the current section, sections swipe, Escape closes, swipe up opens, drag down
closes, a place opens. `cargo test -p devagotchi-hub`: 523 passed.

## Delivery

Implementation: M1 to M4. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
