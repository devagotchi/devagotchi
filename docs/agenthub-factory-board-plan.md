# Plan: AgentHub as an AI factory board — boundary, hooks, importers, UI

Status: PLAN for Jesse's review (2026-11-09). Built on the research brief
`agenthub-research.md` (not yet in this repo; every claim there has a source) and on `main`
at 660efb6. Each phase gets a code-anchored spec for approval before any agent builds it.
Storage and offline rules come from [ADR 0001](adr/0001-hosted-board-postgres-and-clickhouse.md)
and [ADR 0002](adr/0002-bound-board-offline-cache-and-queued-drafts.md); nothing here changes
them.

Ruled 2026-11-09 (Jesse):
- The **project boundary is set first**: everything inside that folder is that project.
- Enforcement must be **how a careful developer would build it**: fluid, seamless, tunable.
- Build a **library of the defaults AI tools already write** (roadmaps, plans, todos) with
  filters and guards that structure them; import from markdown/TODO files, CSV/Notion,
  Linear/Jira and GitHub Issues/Projects.
- Hierarchy is **UI-configurable**: baseline 1 (Org → Project → Area → Epic → Ticket) is the
  default, baseline 2 (Org → Project → Ticket + labels) selectable, both customisable.

## What the research says works (and what failed)
- Works: **prime, don't interrogate** — inject project, claimed ticket, acceptance criteria and
  next ready ticket at session start **and again after context compaction** (beads; the
  smoothest tool in practice). **Gate writes, never reads**, and put the exact one-command fix
  in the deny text (OpenAI harness, Claude/Gemini/Cursor deny reasons reach the model). **Use
  Stop as the completion gate** against "declared done too early" (Anthropic's most-reported
  failure). **Fail open** when the board is unreachable. **Tickets sized to one session**,
  scheduled from a dependency graph. **Proof of work** (PR + tests + preview/video) as the
  review unit for non-experts (OpenAI Symphony).
- Failed: Gas Town's many-role supervisor stack (stuck agents, ~$100/h, unmergeable output;
  abandoned Aug 2026); markdown plans that pile up and go stale; status that lags reality.
- Human attention caps one person at about 3–5 concurrent agent sessions (Symphony).

## What exists in Devagotchi today (main, 660efb6)
- Project folder marker `.devagotchi/` (`board/mod.rs` `BOARD_DIR`) with `config.json`
  (project, prefix, hub binding) and `board.sqlite3`; found only in the folder given, no
  walk-up, no git-remote fallback.
- `devagotchi board prime` (context for a session), claim/lease/heartbeat/handoff,
  `discovered_from`, dependencies and a ready queue, configurable workflow with lanes
  (`board/workflow.rs`), kinds task/epic/message, parent/child ids, anchors (machine, repo,
  environment, agent, session, fingerprint) and `board_settings`.
- Hook pack (`board/cli.rs` `HOOK_SCRIPTS`): session start → prime; tool use → heartbeat;
  stop while holding → refused once. Claude Code and Codex only; written by hand into
  settings; nothing gates writes; no re-prime after compaction.
- Phone app board/ticket pages, approvals, push.
- The Desktop Commander-shaped local surface (`run`, `read_file`, `write_file`, `edit_file`,
  `list_directory`, `read_multiple_files`, `read_image`, `get_file_info`, `create_directory`,
  `move_file`, the `operation_*` jobs) is smoked by `tests/adapter_probe.py --account`, which
  `tests/e2e.py` runs on Linux and Windows in CI; `tests/mcp_smoke.py` covers the job
  lifecycle (Linux only).

## Phases (each its own spec; one agent; one PR per item; merged on full green)

### F1 — The project boundary (first, per the ruling)
- Find the project by **walking up** from the working directory to the nearest
  `.devagotchi/` (like git); fallback: the git root's remote URL resolves to a project bound on
  the hub, so worktrees and fresh clones land on the same board. `DEVAGOTCHI_PROJECT` overrides.
- **Areas** (sub-categories) are nested markers: `<sub>/.devagotchi/area.json` (no board of
  its own); the nearest wins, so `services/api/` work is tagged area `api` automatically.
- `devagotchi board init` becomes the one boundary step: infers the name from the folder and
  git remote, asks nothing it can infer, writes the marker, binds to the hub when signed in,
  installs hooks (F2), and runs the first scan (F5).

### F2 — Prime everywhere, automatically
- `devagotchi board hooks install` detects the agent CLIs present and writes their hook config:
  Claude Code (SessionStart incl. after compaction, PreToolUse, Stop), Codex, Gemini CLI,
  Cursor. Idempotent; `--remove` undoes it. Prime output stays within ~1–2k tokens.
- Re-prime after compaction on every CLI that exposes it.

### F3 — The write gate (tunable modes)
- PreToolUse-type hook on edits, writes, patches and mutating shell commands: allowed when
  this project has a ticket claimed by this agent; otherwise denied with the fix, e.g.
  "No ticket claimed in <project>. Run `devagotchi board claim --next` (next: T-42 Add login)".
- Modes per project, switchable in the UI: **observe** (log only — default for a new project),
  **guide** (deny with the fix — default once the project has tickets), **strict** (also require
  the ticket's area to match the files touched). Reads are never gated. Board/network errors
  fail open and are logged.

### F4 — The completion gate
- Stop/TaskCompleted refuses "done" while the claimed ticket's acceptance criteria are not
  checked or its verify command (tests/build) fails; capped at 2 refusals, then it hands off
  with a note instead of looping. Builds on today's "refused once" stop hook.

### F5 — The defaults library and importers
- **Library**: a registry of parsers, each a small data-driven rule file (editable, versioned,
  extendable by anyone): spec-kit `tasks.md`, Kiro specs, Task Master `tasks.json`, Backlog.md,
  beads JSONL, Lovable `.lovable/plan.md`, Claude plan files, Codex ExecPlans, Cursor saved
  plans, Ralph `fix_plan.md`, Harper Reed `todo.md`, Anthropic `feature_list.json`, and generic
  `- [ ]` checklists, TODO.md / ROADMAP.md / PLAN.md / PRD.md, `TODO:`/`FIXME:` comments.
- `devagotchi board scan` → "Found 37 tasks in 4 files" → **dry-run preview** (in the CLI and
  the UI) → import. Every imported ticket carries `external_ref` (file + line or remote URL);
  re-import is idempotent on it; dedupe by fingerprint; stale plans are flagged, not imported
  twice.
- **Remote importers** with the same contract: GitHub Issues/Projects, Linear, Jira (API),
  CSV, Notion (export). One-shot import by default; two-way sync opt-in per source with a
  stated field-ownership policy.

### F6 — Hierarchy, configurable
- Board settings hold the hierarchy: levels, names and which kinds live at each. Baseline 1
  (default): Org → Project → Area → Epic → Ticket (+ checklist items). Baseline 2: Org →
  Project → Ticket + labels. Custom: rename/add/remove levels in the UI. Scheduling always uses
  the dependency graph (ready = unblocked, unclaimed, by priority then age).

### F7 — The UI and AI-assisted workflow
- **New project** (web + phone): name → boundary (shows the one `devagotchi board init` line to
  paste, or the AI runs it) → hierarchy baseline → scan & import preview → done.
- **Plan with AI**: MCP prompts that turn a goal or a pasted plan into epics and
  one-session tickets with acceptance criteria, shown as a preview the person edits and
  approves before anything is created.
- Board views by area/epic, ready queue, who holds what, gate mode switch.

### F8 — Proof of work and parallel safety
- Completion attaches the diff/PR, test results and a preview link or short recording; the
  person reviews outcomes on the phone.
- Advisory file reservations so two agents don't edit the same files; default 1–3 concurrent
  agents with the ceiling shown.

## Decisions to take (one at a time, before their phase)
1. F1: marker name — keep `.devagotchi/` (exists) or `.agenthub/` now that AgentHub is its own
   surface (proposal: keep `.devagotchi/`, read `.agenthub/` too, decide before the public repo).
2. F3: default mode for a brand-new project (proposal: observe for the first session, then
   guide automatically once tickets exist).
3. F4: what counts as verified (proposal: checklist ticked + the project's verify command).
4. F5: which remote importer first (proposal: GitHub Issues, since every project has a repo).

## Order
F1 → F2 → F3 → F4 (enforcement) in sequence; F5 and F6 in parallel after F1; F7 after F5/F6;
F8 last. Prerequisite outside code (Jesse): Google sign-in keys on the hub, the hub switched to
`ghcr.io/devagotchi/devagotchi`, and the new session with `devagotchi/devagotchi` attached.

## After this plan: the hosted (cloud, enterprise) edition
The implementation and release plan rules that self-hosted completion comes before commercial
hosting, and the private commercial repository does not exist yet. The hosted edition reuses
this core unchanged: the same hub binary, Postgres board of record, ClickHouse analytics and
mail sender, with SaaS sign-in, tenant provisioning and billing added in that repository. It
starts only once the F1–F4 enforcement chain and the hosted-board tests are green on a real
Postgres and the compose stack passes `tests/deploy_scripts.sh` with ClickHouse in it.
