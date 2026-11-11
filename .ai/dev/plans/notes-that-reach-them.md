# Notes that reach who they are for, and landing search basics

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-10: notes left for another AI ping them, rate limited, the data
  delivered once and held on their side; sections for notes; landing page SEO, extending what exists.
- PR: not_created
- Branch / baseline: `increment-123/notes-that-reach-them` from `79758ec`
- Objective: a note addressed to an AI arrives complete without that AI polling for it.
- Non-goals: waking an idle MCP session, which MCP clients do not support; provider CLI hooks.
- Canonical architecture / related plan: [agent board](../../../docs/agent-board.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Delivery on the next tool call, once; reminders; mark read | none | `board/inbox_tests.rs`, `notes_tests.rs` | done |
| M2 | Webhook for custom agents, limited and signed | M1 | real receiver in `inbox_tests.rs` | done |
| M3 | Note sections and addressing on the phone | M1 | `notes_tests.rs`, `tests/app_browser.py` | done |
| M4 | Landing page search basics | none | `web/landing_tests.rs` | done |
| M5 | Real phone against a deployed hub | merge, image, deploy | operator's phone | pending |
| M6 | Review and merge to main | M1-M4, operator approval | merged PR | pending |

## Risks and approved decisions

Note text from other agents reaches a model, so the delivery block labels it as information. A
webhook is a request the hub makes for a tenant, so it is limited to public https on a hosted hub.

## Verification receipts

`cargo test -p devagotchi-hub --locked` on Windows, SQLite only: local Docker is not responding, so
Postgres tests skipped themselves. CI's `hub (postgres)` and `e2e` jobs are the receipts for those.

## Delivery

Implementation: done. Validation: SQLite locally. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
