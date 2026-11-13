# Roadmap: AgentHub, the phone, the browser, and the cloud edition

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-11: plan the software factory (AgentHub), the mobile
  optimizations and the full browser experience, each moving towards starting the official
  cloud commercial version as its own repository with this one as a submodule.
- PR: not_created
- Branch / baseline: `increment-133/roadmap`, from `main` at `ed818cf`
- Objective: one page that says what is built, what comes next on each track, in what order,
  and what has to be true before the cloud repository starts.
- Non-goals: re-deciding anything already ruled (listed in each track's plan).

## The four tracks

| Track | Plan | Where it stands (main, `ed818cf`) | Next |
|---|---|---|---|
| AgentHub, the software factory | [agenthub-factory.md](agenthub-factory.md) | Board, leases, handoffs, dependencies, ready queue, notes that reach agents, approvals and standing approvals, GitHub issues, 17 connectors, API keys; engine in `crates/hub/src/agenthub/`. F1 and F2 partly built; F3 to F8 not started | F1 finished, then F4: completion gated on checks |
| Phone | [mobile-experience.md](mobile-experience.md) | P1a to P1d done; tabs Overview, AgentHub, Approvals, Machines, You; swipe between tabs; Go to sheet | P2a review queue, P2b live ticket activity |
| AI sessions on your machines | [ai-sessions.md](ai-sessions.md) | Not started; machines, operations, push and email exist to build on | S0 name the clients, S1 consent, S2 list sessions |
| Plugins for Claude, Codex and Grok | [plugins.md](plugins.md) | One marketplace in this repository: the `devagotchi` connector and skills, the `devagotchi-agenthub` hooks; validated and installed in throwaway Claude Code and Codex homes | G2 guided install on the hub, G3 views the API opens (MCP Apps), G4 live notices in Claude Code |
| Full browser | [browser-experience.md](browser-experience.md) | The desktop shows the phone's single column with the tab bar at the bottom | B1 wide layout with a side rail, B2 board as columns |
| Cloud edition | [cloud-edition.md](cloud-edition.md) | Boundary written ([editions](../../docs/editions.md)); license BSL 1.1; public image; no tag, no extension points, no private repo | C1 first pre-release tag, C2 extension points in core, C3 private repo with core as a submodule |

## Order

Two lanes run side by side; each item is one PR, merged on full green (standing rule).

1. **Core lane** (this repository): F1 → F4 → P2a with B1 → F2 → B2 → P2b → F3 → the rest.
   P2a and B1 share one review-queue data model, so they ship together.
2. **Cloud lane**: C1 (tag) and C2 (extension points, in this repository) can start now; C3
   (the private repository) right after C2 merges. C4 onwards waits for the gate below.

## Gate before cloud features (C4 onwards)

Kept from the [factory board plan](../../../docs/agenthub-factory-board-plan.md), "After this plan":
- F1 and F4 merged (the boundary and the completion gate); F2 and F3 may follow.
- Hosted-board tests green on real Postgres (CI `hub (postgres)` already does this).
- The compose stack passes `tests/deploy_scripts.sh` with ClickHouse in it (CI `hub (deploy scripts)`).
- A tagged pre-release (C1) the cloud repository pins.

## Decisions that are the operator's

| # | Decision | Recommendation | Blocks |
|---|---|---|---|
| D1 | Which GitHub organisation owns the private cloud repository: `devagotchi`, `shippinAI` or `shippin-tech` | The organisation that will own the hosted business and its billing; "powered by shippin.ai" points to `shippinAI` | C3 |
| D2 | Billing provider | Stripe (subscriptions, customer portal, webhooks) | C4 |
| D3 | When to tag `v0.1.0-alpha.1` | Now: a pin needs a tag, and pre-release tags do not move `:latest` (C1) | C1 |
| D4 | Project marker: keep `.devagotchi/` or move to `.agenthub/` (factory plan decision 1) | Keep `.devagotchi/`, read `.agenthub/` too | F1 |
| D5 | What counts as verified (factory plan decision 3) | Every required check green, the reviewer is not the worker | F4 |

## Delivery

Implementation: plans only. Validation: `devagotchi ai check`. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
