# AgentHub as a software factory

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-09 (factory board plan, rulings) and 2026-11-11 ("fold in
  the latest software factory trends that are showing massive success via research").
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`
- Objective: agents take tickets, prove the work, and a person reviews outcomes, not process.
- Non-goals: a role hierarchy of supervising agents (Gas Town: abandoned Aug 2026, per the
  factory board plan); a task store separate from the board.
- Canonical architecture / related plan: [factory board plan](../../../docs/agenthub-factory-board-plan.md)
  (phases F1 to F8, rulings of 2026-11-09), [agent board contract](../../../docs/agent-board.md),
  [roadmap](roadmap.md). Code: `crates/hub/src/agenthub/`.

## Evidence (research of 2026-11-11, primary sources)

- Agents opened over 40% of PRs on GitHub in September 2026 (17% in March); 90% of them merged
  within two weeks (GitHub Octoverse 2026). DORA 2025: review becomes the bottleneck.
- What raises merge rates: a clear scope and an outcome a test or build can check (Devin 34% →
  67% in a year, its own figures); the agent checks itself before a human sees it (Copilot
  coding agent runs CodeQL, secret scanning and a self-review first).
- What prevents "declared done too early": a durable list of checks the agent can only mark
  passed by evidence (Anthropic, effective harnesses for long-running agents); a hook that
  refuses completion (Claude Code `TaskCompleted`).
- Plans gated on approval before code (Spec Kit, Kiro, Factory Spec Mode, a plan critic in
  Jules). Human as owner, agent as delegate (Linear agents). Review on the phone with accept,
  edit, respond, ignore (LangChain Agent Inbox; Linear mobile sessions).

## Milestones and tasks

State from `main` at `ed818cf`. F-numbers are the factory board plan's; new items continue it.

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| F1 | Project boundary: walk up to `.devagotchi/` (CLI does this today), git-remote fallback, areas (`area.json`), one-step `devagotchi board init` | D4 | CLI tests; a worktree and a fresh clone land on the same board | partial |
| F4 | Completion gated on checks: a ticket carries checks (acceptance criteria and an optional verify command); `board_check` records each result with evidence; `board_complete` is refused while a required check is not green, with the exact fix in the refusal; review stays with someone other than the worker; the Stop hook refuses twice, then hands off with a note | F1, D5 | MCP and hosted tests on Postgres and SQLite: refused with a failing check, accepted when green, worker cannot review own ticket | not started |
| F4b | Plan before code: an optional plan step on claim; the plan goes to the phone as an approval; standing approvals cover low-risk lanes | F4 | approval tests; phone page | not started |
| F2 | Prime everywhere: re-prime after compaction (`SessionStart` with source `compact`), hooks for Gemini CLI and Cursor, installed from the connector catalog (`crates/connectors`) | F1 | hook tests per client; `tests/clients_oauth.py` for the clients CI runs | partial (Claude Code and Codex: session start, tool use, stop) |
| F10 | Owner and delegate: each ticket has the accountable person and the agent holding it; filter by either | F4 | board tests; ticket page | not started |
| F11 | Live activity line: heartbeats carry what the agent is doing now; the ticket page shows it; a person's reply lands in that agent's inbox | F10 | inbox tests; phone P2b | not started |
| F3 | Write gate, tunable modes (observe, guide, enforce) | F2, D-decision 2 of the factory plan | hook tests | not started |
| F5 | Defaults library and importers (markdown plans and TODOs first, then GitHub Issues) | F1 | parser fixtures | not started |
| F6 | Hierarchy, configurable (baseline 1 default, baseline 2 selectable) | F1 | settings tests; UI | not started |
| F8 | Proof of work (diff or PR, test results, preview) attached at completion; advisory file reservations; fan-out of one parent ticket to child tickets per repository or machine, rolling up to one review | F4 | board tests; review queue shows them | not started |
| F9 | Outcomes per agent and client: accept rate, send-back rate, claim-to-done time, expired leases (ClickHouse) | F4, F10 | warehouse tests; Overview card | not started |

## Risks and approved decisions

Ruled 2026-11-09: the boundary is set first; enforcement fluid and tunable; a library of the
defaults AI tools write; hierarchy configurable. Kept here. The research ranks the completion
gate highest, so it follows F1 directly. Status lagging reality (a known failure of agent task
lists) is why every gate reads the board of record, never a cache. Notes and issue text stay
untrusted data (`agenthub/issue_lifecycle.rs` protocol).

## Verification receipts

Each milestone records its own in its PR. Plans only here.

## Delivery

Implementation: F1 and F2 partial. Validation: per milestone. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
