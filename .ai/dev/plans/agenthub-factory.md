# AgentHub as a software factory

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-09 (factory board plan, rulings), 2026-11-11 ("fold in
  the latest software factory trends that are showing massive success via research") and
  2026-11-14 ("all of my software factory absorption, the ability to assign agents … research
  and extend what we have … capabilities that are modern and showing strong trending").
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

## Evidence, second pass (research of 2026-11-14, primary sources, Aug to Nov 2026)

- Agents are assignees with an identity: Jira brings an agent in from the picker, an
  @mention, a transition or a column, and only the person who invoked it talks to it
  (support.atlassian.com); Devin starts on a Jira assign or mention (Sep 30, GA) and has
  service users with their own identity (Nov 6); GitHub's usage API keys every metric by a
  stable `agent_id` per third-party agent (Aug 7, GA).
- Isolation beats coordination: Claude Code runs each agent-view session and each `/batch`
  subagent in its own worktree; CooperBench (arXiv 2601.13295) finds two agents on one task
  ~30% worse, and messages between them cut conflicts but not failures.
- A review agent before the person: Copilot code review covers bot-authored PRs including its
  own agent's (Aug 27, GA); Devin Review fixes its own findings (Sep to Nov).
- Event-started work, read-only by default with checked outputs: GitHub Agentic Workflows (Oct
  28, GA), Copilot Automations, Linear Loops.
- Routing by cost: Copilot Fusion cascade (cheap model first, escalate on a failed check; Oct 28,
  GA), Linear adaptive routing (Sep 24).
- What people measure: merge rate (Octoverse: 90% of agent PRs merged within two weeks), cost per
  PR, sandbox minutes.
- Views: a desktop "which agents need you" screen (Claude Code agent view, Codex command center,
  Copilot Projects, mostly preview); on the phone, review and merge (Linear Mobile Diffs, Nov 5,
  GA).
- MCP 2026-07-28 is final: stateless requests, `server/discover`, tasks as an extension, Client
  ID Metadata Documents in place of dynamic client registration (now deprecated), issuer
  validation. The hub's OAuth registers clients dynamically today.

## What the operator's own factory brings (`D:\localdev\agent-hub`, Go, surveyed 2026-11-14)

The Go hub is this board's predecessor; Devagotchi has its claim, lease, fence, handoff, review
lane and hash-chained history. Not yet absorbed: a roster of principals (jesse, claude-code,
codex, cursor, clyffy) with a role (admin, reviewer, worker, observer), a WIP limit and
`last_seen` presence (`server/internal/presets/default.json`, `docs/spec.md`); assignment by
name from that roster; an external policy decision service that fails closed (`HUB_POLICY_URL`,
`POST /decide`); an Agents page and an Audit page with chain verification. Its secret box is
write-only AES-256-GCM, as the hub's `sealed.rs` is. Its connectors are LLM provider adapters
only; outside services are the [connections gateway](connections-gateway.md).

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
| F5 | Defaults library and importers (markdown plans and TODOs first, then GitHub Issues) | F1 | parser fixtures | partial: `board scan` (increment 189), the library as rule files, preview, import once by fingerprint, local and hub boards; GitHub Issues and the device proof remain |
| F6 | Hierarchy, configurable (baseline 1 default, baseline 2 selectable) | F1 | settings tests; UI | not started |
| F8 | Proof of work (diff or PR, test results, preview) attached at completion; advisory file reservations; fan-out of one parent ticket to child tickets per repository or machine, rolling up to one review | F4 | board tests; review queue shows them | not started |
| F9 | Outcomes per agent and client: accept rate, send-back rate, claim-to-done time, expired leases, cost per ticket when the client reports it (ClickHouse) | F4, F10 | warehouse tests; Overview card | not started |
| F12 | Agents as assignees: a roster per board (the account's connected AI clients, agents with a webhook, agents on the board's tickets, any name typed), **Assign to an agent** on the ticket page (desktop and phone), the assignment as a note to that agent so it reaches its inbox and webhook at once; `board_update` already takes an assignee | none | `ticket_tests::a_person_assigns_a_ticket_to_an_agent_and_the_agent_is_told` | done (increment 139) |
| F13 | The roster as data, from agent-hub: each agent's role (worker, reviewer, observer), WIP limit, last seen, and a stable id every metric and audit entry keys on; an Agents page | F12 | board tests; page | not started |
| F14 | A review agent before the person: a ticket an agent completes goes to a reviewer agent (not the worker) first, then to the phone's review queue with its findings | F4, F13 | board tests; review queue | not started |
| F15 | One worktree per claimed ticket on the machine that works it, and advisory file ownership between tickets | F8 | broker and board tests | not started |
| F16 | Tickets that start from events (an issue labelled, a check failing, a schedule), read-only by default, outputs checked before they apply | F4 | board tests | not started |
| F17 | A policy decision service, from agent-hub: an optional URL the hub asks before claim, complete and approval, failing closed | F4 | board tests with a stand-in | not started |

## Risks and approved decisions

Ruled 2026-11-09: the boundary is set first; enforcement fluid and tunable; a library of the
defaults AI tools write; hierarchy configurable. Kept here. The research ranks the completion
gate highest, so it follows F1 directly. Status lagging reality (a known failure of agent task
lists) is why every gate reads the board of record, never a cache. Notes and issue text stay
untrusted data (`agenthub/issue_lifecycle/mod.rs` protocol).

## Verification receipts

Each milestone records its own in its PR. Plans only here.

## Delivery

Implementation: F1 and F2 partial; F12 done. Validation: per milestone. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
