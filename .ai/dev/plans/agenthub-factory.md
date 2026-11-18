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
  [roadmap](roadmap.md). Code: `crates/hub/src/agenthub/`. The 2026-11-06 project-agent
  delivery plan (M0 to M9, never on `main`) is folded in: M1 is F5a, M2 F1b, M3 and M4
  F5b and F5c, M5 F18, M6 F19, M7 F8b, M8 F20, M9 the
  [release readiness](v0.1-release-readiness.md) plan's M5; M0 is the broker's own work.

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
| F1b | Verified repository binding (M2 of the 2026-11-06 project-agent delivery plan): a project's identity is the provider's repository id plus its normalized remote, kept apart from a machine's checkout root and the host workspace mapped to it; binding checks the repository exists and the person can read it (agent-board 21c, B14); the same repository or workspace never silently makes a second project; rename, transfer and rebind are explicit and audited; a board made before its binding is bound by an explicit step, never guessed | F1, F5a | board and hub tests: an inaccessible or wrong repository refused; a second bind of the same repository finds the first project; a rebind leaves an audit event | not started |
| F4 | Completion gated on checks: a ticket carries checks (acceptance criteria and an optional verify command); `board_check` records each result with evidence; `board_complete` is refused while a required check is not green, with the exact fix in the refusal; review stays with someone other than the worker; the Stop hook refuses twice, then hands off with a note | F1, D5 | MCP and hosted tests on Postgres and SQLite: refused with a failing check, accepted when green, worker cannot review own ticket | not started |
| F4b | Plan before code: an optional plan step on claim; the plan goes to the phone as an approval; standing approvals cover low-risk lanes | F4 | approval tests; phone page | not started |
| F2 | Prime everywhere: re-prime after compaction (`SessionStart` with source `compact`), hooks for Gemini CLI and Cursor, installed from the connector catalog (`crates/connectors`) | F1 | hook tests per client; `tests/clients_oauth.py` for the clients CI runs | partial (Claude Code and Codex: session start, tool use, stop) |
| F10 | Owner and delegate: each ticket has the accountable person and the agent holding it; filter by either | F4 | board tests; ticket page | not started |
| F11 | Live activity line: heartbeats carry what the agent is doing now; the ticket page shows it; a person's reply lands in that agent's inbox | F10 | inbox tests; phone P2b | not started |
| F3 | Write gate, tunable modes (observe, guide, enforce) | F2, D-decision 2 of the factory plan | hook tests | not started |
| F5 | Defaults library and importers (markdown plans and TODOs first, then GitHub Issues) | F1 | parser fixtures | partial: `board scan` (increment 189), the library as rule files, preview, import once by fingerprint, local and hub boards; GitHub Issues and the device proof remain; the scanner's remainder is F5b and F5c |
| F5a | Import once, by origin (M1 of the 2026-11-06 project-agent delivery plan): `board_create.origin` / `board new --origin` / `NewTicket.source` names `{path, item, revision?, content_sha256?}`; `(path, item)` unique per board (AgentHub migration 0002 `board_sources`, append-only, RLS); principal, time and a version-1 normalized creation digest recorded once with the ticket, deps and event in one transaction; an exact repeat returns the ticket as it is (no event, no notification, lease and edits kept); changed inputs, a new revision included, are `source_conflict` with nothing written; `board_get`, `board show`, export carry `provenance` ([board contract §10](../../../docs/agent-board.md), B15). `board scan --import` still dedupes by fingerprint: wiring it to origins (with the line number out of its description) is the next step | F5 | `crates/agenthub/src/tests/sources.rs` (repeat after reopen, edit and claim; 8 changed inputs conflict; no notification; other board isolated; 6 concurrent tokio importers make 1 ticket and 1 event; invalid origins and a bad discovered-from write nothing; raw UPDATE, DELETE, ON CONFLICT DO UPDATE refused; normalized repeat after a changed default or other anchors; the digest's fixed byte form); `cli/tests/sources.rs` (local lifecycle, exit 2); `crates/hub/src/agenthub/tests/sourced.rs` (two accounts over HTTP-MCP, a repeat from another session, a forged `recorded_by` refused, not found across accounts by id, the bound CLI lifecycle and export); `crates/devagotchi/tests/board.rs` (six concurrent processes, then the lifecycle with a process per step). Run locally on the test Postgres, 2026-11-18; not yet in CI or on Texas | built (increment 205), unmerged |
| F5b | Scanner evidence (M3 of the 2026-11-06 project-agent delivery plan): `board scan --import` passes an origin per finding (path, a stable item id, the revision read, the item's content digest; the line number out of the generated description), with the parser version and evidence ranges; an MCP `board_scan`; bounded read-only discovery that never executes what a document says (constraints 1 to 3 below); ambiguous or unsupported items flagged. Decides how a moved revision is reconciled (B15): today a new revision alone is `source_conflict`, so an unchanged item rescanned at a later commit conflicts, where constraint 5 says it maps to its ticket | F5a, F1b | parser fixtures; traversal, symlink, ignore and size-budget tests; two concurrent `scan --import` runs make one ticket per finding; the same revision scanned twice yields the same candidates; a rescan at a later commit with unchanged items writes nothing and conflicts nothing | not started |
| F5c | Reviewable import (M4 of the 2026-11-06 project-agent delivery plan): preview the proposed creates, links and conflicts in the CLI and on the web, select items, commit a frozen manifest (repository revision, parser version, item evidence, proposed mapping) whose revisions and digests are checked again at commit, so a stale scan is rejected; the batch is atomic or keeps a durable resumable receipt with per-item outcomes; a double click or a retry imports once and a cancel writes nothing; per-project workflow, labels, priorities and mappings persist; a ticked checkbox never completes a ticket silently | F5b | hub web and board tests; a browser test of an interrupted import | not started |
| F6 | Hierarchy, configurable (baseline 1 default, baseline 2 selectable) | F1 | settings tests; UI | not started |
| F8 | Proof of work (diff or PR, test results, preview) attached at completion; advisory file reservations; fan-out of one parent ticket to child tickets per repository or machine, rolling up to one review | F4 | board tests; review queue shows them | not started |
| F8b | Attempt evidence recorded by the system (M7 of the 2026-11-06 project-agent delivery plan): ticket and fence → run intent → host and workspace → broker operation and attempt → output, test and change references, with authoritative times and identities (the [task-graph contract](../../../docs/task-graph-and-mesh-contract.md) owns those identities: a ticket, a source item, an agent session, a broker operation and a process are different things); a stale fence or another project's evidence is refused; the reviewer sees the real artifacts; an agent's own report is labelled as such and never alone proves completion | F8, F5a, F18 | board and broker-journal tests: a stale fence and a wrong project refused; a completion carrying only self-report shown as such | not started |
| F9 | Outcomes per agent and client: accept rate, send-back rate, claim-to-done time, expired leases, cost per ticket when the client reports it (ClickHouse) | F4, F10 | warehouse tests; Overview card | not started |
| F12 | Agents as assignees: a roster per board (the account's connected AI clients, agents with a webhook, agents on the board's tickets, any name typed), **Assign to an agent** on the ticket page (desktop and phone), the assignment as a note to that agent so it reaches its inbox and webhook at once; `board_update` already takes an assignee | none | `ticket_tests::a_person_assigns_a_ticket_to_an_agent_and_the_agent_is_told` | done (increment 139) |
| F13 | The roster as data, from agent-hub: each agent's role (worker, reviewer, observer), WIP limit, last seen, and a stable id every metric and audit entry keys on; an Agents page | F12 | board tests; page | not started |
| F14 | A review agent before the person: a ticket an agent completes goes to a reviewer agent (not the worker) first, then to the phone's review queue with its findings | F4, F13 | board tests; review queue | not started |
| F15 | One worktree per claimed ticket on the machine that works it, and advisory file ownership between tickets | F8 | broker and board tests | not started |
| F16 | Tickets that start from events (an issue labelled, a check failing, a schedule), read-only by default, outputs checked before they apply | F4 | board tests | not started |
| F17 | A policy decision service, from agent-hub: an optional URL the hub asks before claim, complete and approval, failing closed | F4 | board tests with a stand-in | not started |
| F18 | Provider choice on a designated host (M5 of the 2026-11-06 project-agent delivery plan): the choices a host offers come from what it can actually run (neither a product name nor an API key proves a client can run there); pinned adapter versions; an explicit model, settings and allowed workspace; an unsupported or signed-out choice is shown unavailable; a launch goes through the broker's admission and audit; no credential in a ticket or a log. Main has no host capability report yet; the session adapters are [ai-sessions](ai-sessions.md) S2 | F1b, ai-sessions S2 | device and broker tests per adapter; an unavailable choice shown with why | not started |
| F19 | Audited SSH bootstrap (M6 of the 2026-11-06 project-agent delivery plan): a host the person picks, its SSH host key pinned (a changed key fails closed), an explicit account with the least privilege; a plan and a phone approval before any install or service change; idempotent; it verifies the installed binary, the host binding, the grants and the outbound link; an interrupted, unreachable or changed-key run never reads as success. SSH bootstraps and diagnoses only, it never owns a job ([cloud edition](cloud-edition.md) C9 is SSH to instances, a different thing) | F18 | bootstrap tests against a throwaway host: a changed key refused, a rerun changes nothing, an interrupted run reported as such | not started |
| F20 | Active-project overview (M8 of the 2026-11-06 project-agent delivery plan): a read model over the boards the person is granted, never a second ticket store, building on `/app/boards` (`crates/hub/src/web/app/mod.rs`) and agent-board 21g; bounded pages with their freshness; counts match each board; revoked access disappears; offline or partial results say so; drilling down keeps the project's scope | F1b, F5c, F8b | hub web tests over two orgs and a revoked grant; a browser test | not started |

## Risks and approved decisions

Ruled 2026-11-09: the boundary is set first; enforcement fluid and tunable; a library of the
defaults AI tools write; hierarchy configurable. Kept here. The research ranks the completion
gate highest, so it follows F1 directly. Status lagging reality (a known failure of agent task
lists) is why every gate reads the board of record, never a cache. Notes and issue text stay
untrusted data (`agenthub/issue_lifecycle/mod.rs` protocol).

**Importing from documents** (from the 2026-11-06 project-agent delivery plan; F1b, F5b, F5c):

1. Check the project's binding and read permission before discovery; keep the repository's
   identity apart from a machine-local path; never send source contents to another service
   as a side effect of scanning.
2. Discover within a configured allowlist and bounded depth, file count and bytes; ignore VCS
   objects, generated and vendor directories and secrets by default; follow no symlink out of
   the root; report excluded and unreadable files and an exhausted budget.
3. Parse supported forms deterministically first. AI suggestions are candidate readings with
   evidence and confidence, never instructions or authority. The same revision scanned twice
   yields the same candidates. Ambiguous repeated headings or items need the person's mapping,
   not a guessed merge.
4. Preview a manifest with the repository revision, parser version, item evidence and proposed
   mapping; check revisions and digests again at commit and reject a stale preview; keep the
   project's own settings; commit the batch atomically or with a durable resumable receipt of
   per-item outcomes, never half a board silently.
5. A rescan maps unchanged items to their existing tickets; an edited source item is an
   explicit conflict for review. Never reopen done work, alter a claimed ticket or erase a
   person's notes to make the document match. A deleted source item is flagged; its ticket is
   not deleted. F5a (B15) counts the revision in the creation digest, so F5b must settle how an
   unchanged item at a later commit maps to its ticket before `board scan` passes origins.

**Hosts and management** (from the same plan; F18, F19, F8b): the management path is the hub
with the machine's outbound link, not a new public unauthenticated port. SSH is a narrow
bootstrap and diagnostic transport, never another job owner: known-host verification, a host
the person picks, fail closed on a changed key. Never broaden firewall or network policy,
create persistent credentials or accept installation warnings on the person's behalf. Bind
sessions to the authenticated account and seat, project, host and workspace grants; redact
secrets. A reconnect keeps the original operation's identity and reports uncertain effects
before any replay. Provider discovery is by capability: a product name or an available API key
is no proof a client can run on that host.

The outcome these rows add up to: a person picks a repository and a host, reviews the work
found in its plans, and gets one board; a provider they chose works its tickets through the
broker on that host; every ticket keeps its origin, claims, handoffs, attempt evidence, review
and outcome. F5a alone, or a scanner tested against a stand-in, is not that feature.

## Verification receipts

Each milestone records its own in its PR. Plans only here.

## Delivery

Implementation: F1 and F2 partial; F5a built (increment 205, unmerged); F12 done.
Validation: per milestone. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
