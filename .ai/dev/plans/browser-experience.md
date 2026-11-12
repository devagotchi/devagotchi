# The full browser experience

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: the full browser experience planned alongside the
  phone.
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`
- Objective: on a laptop or desktop, the same app becomes a workspace: navigation on the side,
  a board you see whole, a ticket beside it, and the keyboard for everything.
- Non-goals: a second front end. The same server-rendered pages, the same routes, one
  stylesheet; the wide layout is CSS and a little script on top.
- Canonical architecture / related plan: [mobile experience](mobile-experience.md) (the same
  pages), [roadmap](roadmap.md). Code: `crates/hub/src/web/`.

## Where it stands

At 1440 px the app shows the phone's single column (`--narrow`) with the tab bar at the
bottom. Older pages outside the app frame (`/account`, `/orgs`, `/operator`, `/admin`,
`/approvals` before sign-in) have their own top-bar layout. The Go to sheet already lists every
place and `/app/focus` already answers what it needs.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| B1 | Wide layout from 1000 px: a left rail with the five destinations and their counts, the page beside it at a readable width, the tab bar hidden; the Go to sheet becomes a centred dialog | mobile N1 | browser test at 1440 px: rail present, no sideways scroll; the phone layout unchanged at 390 px | not started |
| B2 | AgentHub board as columns by lane; a ticket opens in a panel beside the board (list and detail), and its URL still opens the page alone | B1 | browser test: open, close, back button | not started |
| B3 | Command palette (Ctrl+K or Cmd+K): the Go to sheet's places plus actions (new ticket, next approval, a machine) and search | B1, mobile P2d | browser test | not started |
| B4 | Keyboard: `g o`, `g a`, `g p`, `g m`, `g y` to go; `j` and `k` through lists; `a` accept and `r` send back in the review queue; `?` lists them | B2 | browser test | not started |
| B5 | Move a ticket between lanes by drag, with a Move to menu that does the same | B2 | browser test with keyboard only | not started |
| B6 | Review queue as a table with bulk accept, from the same data as the phone's (mobile P2a) | mobile P2a | app tests | not started |
| B7 | One frame for every signed-in page: `/account` machines into `/app/machines`, `/orgs` and `/admin` into the app frame under You | B1 | existing page tests moved; redirects kept | not started |
| B8 | CI: the browser test at 1440 px and at 390 px for every app page | B1 | CI job | not started |

## Risks and approved decisions

Keyboard shortcuts never fire while typing in a field, and every one has a visible control.
Drag always has a non-drag equivalent (WCAG 2.5.7). The rail and the tab bar are the same links,
so a page never depends on which one is showing.

## Verification receipts

Each milestone records its own.

## Delivery

Implementation: not started. Push: not_verified. PR: per milestone.
**DELIVERY INCOMPLETE: NOT STARTED**
