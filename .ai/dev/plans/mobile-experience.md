# The phone: the tailored mobile experience

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: a tailored mobile experience; swipes and a popup
  overlay to travel effortlessly between AgentHub, Overview and the other tabs.
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`
- Objective: everything a person does with their agents fits one thumb: see what needs them,
  decide, steer, and move on.
- Non-goals: a native app (the installed web app is the app); features that only make sense
  with a keyboard (those are the browser track).
- Canonical architecture / related plan: [spec P1](../../../docs/spec-phone-app.md),
  [mobile navigation](mobile-navigation.md), [roadmap](roadmap.md). Code: `crates/hub/src/web/app/`,
  `web/assets/app.js`, `web/hub.css`.

## Rules every milestone keeps

- Every gesture has a tap that does the same (WCAG 2.5.1). Swipes start 24 px in from the
  edges (iOS Back) and never in something that scrolls sideways or is `[data-no-swipe]`.
- Server-rendered pages that work without JavaScript; the script only adds. One CSP, no inline
  script or style. Tap targets at least 44 px; text fields 16 px so the phone does not zoom.
- No sideways scroll at 390 px, checked in CI (`tests/app_browser.py`).

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| P1a–d | App shell, home, machine page, board and ticket pages, push | none | spec P1 | done |
| N1 | Tabs Overview, AgentHub, Approvals, Machines, You; swipe between tabs; Go to sheet | P1 | PR #179, 13-check probe | done |
| P2a | Review queue: finished tickets and waiting approvals as cards with the summary, check results and diff or PR link; Accept, Send back with a note, Later; "Now" and "Later" groups. Buttons, not row swipes (row swipes fight the tab swipe) | factory F4 | app tests; browser test on an emulated iPhone | not started |
| P2b | Live ticket: the agent's activity line updates while the page is open; a reply box sends a note to that agent's inbox | factory F11 | inbox tests; browser test | not started |
| P2c | Notifications open the exact ticket or approval; counts per section on the Go to sheet's chips | P2a | push tests | not started |
| P2d | Search in the Go to sheet: boards, machines, tickets by title or id | N1 | app tests | not started |
| P2e | Offline: decisions and notes made offline are queued and sent on reconnect (ADR 0002) | P2a | service worker test | not started |
| P2f | Budgets in CI: page weight, script size (app.js under 20 KB), an accessibility scan (axe) on every app page | N1 | CI job | not started |
| P2g | Real devices: iPhone Safari installed to the home screen and Android Chrome, by the operator, recorded in the PR | P2a | operator receipt | not started |

## Risks and approved decisions

Research of 2026-11-11: Material allows swiping between tabs only where content has no other
swipeable parts, so review actions are buttons. iOS edge-swipe Back cannot be turned off in a
standalone web app, so the app never uses the edges.

## Verification receipts

Each milestone records its own. N1: probe on an emulated iPhone 13 against `hub serve` on
Postgres 18, 13 checks (PR #179).

## Delivery

Implementation: P1 and N1 done. Push: verified for those (merged). PR: per milestone.
**DELIVERY INCOMPLETE FOR P2: NOT STARTED**
