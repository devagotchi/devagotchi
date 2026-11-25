# AgentHub as a software factory

## Active oldest-first absorption, 2026-11-20

Execution now follows [the complete source plan](absorb-preserved-sources.md), its immutable
file/commit inventory and implementation packets. Complete local authoring precedes new CI;
the earlier individual milestone schedules below are historical, not the active work order.

The prior use of **absorbed** below included capabilities merely assigned future milestones.
That is not implementation or feature parity. Those historical headings do not establish
completion. The operator requires actual absorption, oldest source branch first, with each
behavior implemented and verified before its source branch can be closed.

Source order (original work dates, not the dates preservation commits were created):

1. Go AgentHub baseline through `origin/box-snapshot/2026-10-18` (`46419c6`): starts with
   specification `c8e166f`, PostgreSQL backend `a703761`, CLI `4886860`, protocol tests
   `d469998`, and React interface `dcca3d3`; later October 17–18 work follows in ancestry
   order. **Active; not absorbed.**
2. Go `master` through `ca626db` (October 20): audit its remaining delta after source 1.
3. Go `feat/devspace-bridge-contract` through `97172d9` (October 21–22).
4. Go `preserve/packaging-2026-10-25` (`adca8b7`, preserved November 18).
5. Rust `preserve/nov6-uncommitted` (`c936f7f`): verify earlier ports against each behavior,
   not merely their merge commits. Keep original dirty checkouts untouched.

### First source: board creation and task creation

- Branch: `increment-278/absorb-agenthub-board-creation`, baseline `af88363`.
- Source: `dcca3d3`, `web/src/pages/Board.tsx` and `NewTask.tsx`, backed by `a703761`'s
  project/task store. Current Rust already has board binding and ticket creation through
  MCP/CLI, but its browser's empty board list offers no way to start.
- Implement: signed-in browser board creation and task creation using the existing hub
  board resolver and AgentHub transactions; expose them in the board list and board page.
  Preserve existing ticket editing/delegation/history. Product copy belongs to site messages.
- Acceptance: browser creates a board and task, opens that real task and its history;
  MCP observes the same data; another account cannot read or mutate it; a read-only member
  cannot create a task; cross-site and unauthenticated writes fail; database errors remain
  errors. Test with isolated Postgres, then actual Docker/browser acceptance.
- Non-goals of this packet: do not declare the source branch absorbed, replace the current
  database, restore obsolete auth shortcuts, or mark the Agents/Audit/settings interface
  delivered. Those source capabilities remain pending in this same baseline review.
- Database constraint: hub and AgentHub use PostgreSQL. Any remaining SQLite use elsewhere
  requires an explicit justification grounded in established real-world practice; an old
  implementation or instruction is not sufficient justification.
- Implementation: browser forms use the shared audited binding owner and existing board
  creation transaction. Task detail lists dependency links and whether each still blocks
  the task, including changes when a dependency finishes. MCP and standalone/bound CLI
  creation accept dependency sets.
  AgentHub [PR #9](https://github.com/devagotchi/agenthub/pull/9) merged as `e299d18`.
- Validation: 90 AgentHub tests; 492 hub tests passed on the initial full run, and the one
  static audit-owner scan failure was fixed and passed its focused rerun. The bound CLI
  dependency test and final browser HTTP tests passed. The other default workspace crates
  passed 291 tests (cargo test; nextest unavailable). Windows E2E, stdio protocol, layout,
  dependency audit, generated contracts and AI contract checks passed. Edge acceptance
  exercised owner/ordinary-user board and dependent task creation at desktop/phone widths
  against a single-environment native hub and isolated Docker PostgreSQL.
- Delivery: backend merged; parent [PR #272](https://github.com/devagotchi/devagotchi/pull/272)
  merged to main as `53c5c93`, with all 18 checks passing, including Linux/macOS/ARM,
  real ClickHouse export and Docker Compose. Official-image Docker acceptance passed for both existing accounts on `2ce4e91`.

### First source: task search

The next concrete gap is exact task-ID, note-body and PostgreSQL full-text search from
`a703761`. See [the scoped plan](october-task-search.md) and backend
[PR #10](https://github.com/devagotchi/agenthub/pull/10). Bound-CLI acceptance exposed a
parser conflict that discarded the query; [PR #11](https://github.com/devagotchi/agenthub/pull/11)
fixes it. Both backend changes merged; parent pins `6912c3e`. All 94 backend and 495 hub
tests passed against required PostgreSQL, including browser/MCP/bound-CLI search and
account isolation. Parent [PR #273](https://github.com/devagotchi/devagotchi/pull/273) merged
as `2ce4e91`, all 18 checks passed. Deployed-image creation/dependency/search acceptance passed for both existing accounts on `2ce4e91`.

### First source: audit history

[Scoped plan](october-audit-history.md): restore actor/action filters, event details,
newest-first paging, chain verification and live history in the browser. Backend
[PR #12](https://github.com/devagotchi/agenthub/pull/12) merged as `fce03e2`, with 96 local
PostgreSQL tests and its GitHub PostgreSQL check passed. Parent [PR274](https://github.com/devagotchi/devagotchi/pull/274)
merged as `bf74369` after all 18 pre-merge checks; its feature branches were deleted.
The owner-visible hub activity view continues in [its scoped plan](october-hub-audit.md).

The baseline also still requires a source-to-implementation audit of principal roles,
presence, WIP overrides, policy editing/enforcement, workflows/settings, request idempotency,
audit UI, and all later baseline commits. No later source branch is being declared complete.

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
  [roadmap](roadmap.md). Code: `engine/hub/src/agenthub/`. The 2026-11-06 project-agent
  delivery plan (M0 to M9, never on `main`) is folded in: M1 is F5a, M2 F1b, M3 and M4
  F5b and F5c, M5 F19, M6 F20, M7 F8b, M8 F21, M9 F22 and the
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

### The Go hub's last branches (absorbed 2026-11-18)

Two local refs in `D:\localdev\agent-hub` were never pushed: `feat/devspace-bridge-contract`
(`3ca781f`, `34b15b9`, `016883d`, `085b588`, `97172d9`; 2026-10-21 to 22, on top of `ca626db`)
and `preserve/packaging-2026-10-25` (`adca8b7`, committed 2026-11-18, holding the uncommitted
packaging work of 2026-10-25). Each capability is either already in Rust or a milestone here:

| Go capability (commit, path) | Here |
|---|---|
| Hub URL rule: https origin, plain http only on loopback, no userinfo, query or fragment; no redirects for enrollment, polling or WebSocket (`3ca781f`, `bridge/hubclient/transport.go`) | in Rust: `runtime/link/src/state.rs:56-82`, tests `:279-304`; the link's WebSocket follows no redirect (`runtime/link/src/session/mod.rs`). The updater's redirects: [free edition](free-edition.md) D2b |
| One versioned `enrollment.json`, written atomically, owner-only, no fallback to older state (`3ca781f`, `bridge/config/enrollment.go`, `state_unix.go`) | the device key and the state directory in Rust on both OSes (`state.rs:85-138`, `:210-259`); the key signs a message naming the hub (`platform/contracts/src/lib.rs:482-484`), so it is not a bearer token. `link.json`'s atomic write: free edition D2b |
| Windows hub enrollment failing closed until owner-only ACLs exist (G-31) and the server key's Windows gap (`state_windows.go`, `docs/windows-private-storage-next-packet.md`) | the link has Windows DACLs (`state.rs:106-138`); the hub's keys do not: free edition D6 |
| Grant cache that denies after a failed refresh, exact grant version (`3ca781f`, `bridge/runner/grant.go`) | free edition D7, built on AgentHub's standing approvals (`agenthub/src/approval.rs:41-75`) |
| Hub-wide machine lists (G-30) | closed for a multi-tenant hub: `engine/hub/src/tenancy/machines.rs:1-7` |
| The agent guide as `connection_contract` tool, `devagotchi://agent-guide` resource and `devagotchi_connectivity` prompt (`3ca781f`, `bridge/agentguide`) | [plugins](plugins.md) G10 |
| Hosted resource attachment: a person's own endpoint bound to a workspace, a hostname is not a grant, no silent paid fallback (`3ca781f`, `docs/devagotchi/26-connectivity.md`) | [connections](connections-gateway.md) X10 and its risks |
| The devspace as a client of the person's hub; provider sign-in by device code in the person's own browser and key; no credential volume shared (`34b15b9`, `016883d`) | connections X9; [cloud edition](cloud-edition.md) C10 |
| Hosted single-owner OIDC: PKCE, one-use state and nonce, endpoints pinned to the issuer origin with no redirects, RS256 against JWKS, `azp` and `iat` checks (`085b588`, `server/internal/browserauth/`) | PKCE, state, nonce, `iss`, `aud`, `exp` in Rust (`engine/hub/src/identity/web/providers.rs:1-18`, `:422-435`, `:573-620`); the token exchange and discovery follow no redirects (`:116`, `:161`). Missing: `azp`, `iat`, endpoint pinning, and the bearer verifier's redirects and JWKS origin: free edition D1b |
| Hashed server-side sessions in `__Host-` cookies (`085b588`) | in Rust: `engine/hub/src/identity/accounts/sessions/mod.rs:10-12` |
| Edge-to-hub TLS 1.3 with a dedicated client CA (`085b588`, `server/internal/transport/tls.go`) | cloud edition C9 |
| `/readyz` with a 1 s database ping, and a 5 s HTTP drain before dependencies close (`adca8b7`, `server/internal/health/ready.go`, `server/internal/lifecycle/http.go`) | free edition H1 |
| A digest-pinned nonroot image, built twice with identical hashes for amd64 and arm64 (`adca8b7`, `deploy/package/`) | the image is in Rust's deploy (`deploy/compose/Dockerfile:7-8`, `:41`); reproducible builds and the released arm64 artifact run: [self-hosted](self-hosted-and-provenance.md) S3 |
| Browser smoke test in a fresh profile without sandbox-bypass flags (`adca8b7`, `tests/ui/packaging-smoke.mjs`) | its rules in self-hosted S3; the browser tests are `tests/*_browser.py` |
| Notification outbox and signed, expiring decision links (`adca8b7`, `docs/agenthub-packaging-slice.md`) | F18 below |
| `.gitattributes` LF for deploy and static files (`97172d9`) | main's `.gitattributes` keeps `*.sql` and `*.sh` LF; nothing to carry |

**Not carried over, and why:**

- The legacy `bridge.toml` and token fallback (`loadLegacyConfig`, `readLegacyToken`,
  `loadLegacyConnection`): it only read state from before `enrollment.json`. Pre-release: no
  legacy paths.
- `HUB_HUMAN_AUTH` modes `none`, `tailscale-whois`, `proxy-header` and `dev`, and the
  proxy-header secret: the Rust hub's identity (accounts, OpenID Connect providers, Nomen)
  replaces them.
- The single owner pinned by environment (`HUB_OIDC_OWNER_SUB`, `HUB_OIDC_OWNER_PRINCIPAL`):
  the Rust hub has its owner claim and linked sign-ins.
- 5-minute idle and 15-minute absolute browser sessions: the Rust hub's 30 days and 90 days
  (`sessions/mod.rs:10-12`) are a product choice. Revoking a live stream on logout becomes D7's
  acceptance on long waits.
- `server/internal/db/migrations/011_browser_identity.sql`: a Go schema. The hub's identity owns
  its `auth_` tables; there is nothing to migrate.
- The hardcoded central origin in the agent guide's contract, its README and `39c`: no hostname
  is a default in code (cloud edition C11).
- `deploy/gold/*` (a systemd unit, an env file with the real owner, tenant and workspace IDs, a
  private address and a DNS zone ID, a Nomen app JSON, a Python edge-route renderer): real
  deployment data never goes in this repository (editions rule 3), and its policy and ticket
  ceremony is a release gate alpha does not have. The unit's hardening list goes to C9.
- The Go `hub-server.Dockerfile`, its verify stage and its Go and Node pins: the Rust image
  replaces it.
- The Go web client's OIDC-only mode and logout button (`web/src/App.tsx`, `lib/api.ts`,
  `pages/Login.tsx`, `pages/Shell.tsx`), its rebuilt static bundle, `server/internal/auth/`,
  `server/internal/config/config.go`, `server/internal/httpapi/server.go` and the Go modules for
  OIDC (`go.mod`, `go.sum`): the Rust web, identity and session pages replace them.
- The Windows CRLF fix to the Go web enums test, and the `.gitignore` `__pycache__/` line: Go
  tree housekeeping.
- Review transcripts, SHA-256 manifests, the Claude review bundle, packaging logs and
  screenshots under `test-evidence/`: receipts for Go code. `inventory-followup-daybreak.md`
  reviews an inventory-name check whose code is in another repository, so this absorption does
  not cover it.
- The learning inventory and learning-acceptance proposal in `docs/agenthub-packaging-slice.md`:
  they belong to the RRFlow repository, not this one.

## Project-bound delivery (the 2026-11-06 plan, absorbed 2026-11-18)

`docs/project-agent-delivery-plan.md` was written on 2026-11-06 and never committed (it is on
branch `preserve/nov6-uncommitted` of `D:\localdev\devagotchi-source-20261106`). Its outcome
stands: a person picks a repository and a host, reviews the plan work found in it, and gets one
project board; an AI provider they chose works its tickets through the broker on that host;
every ticket keeps its origin, claims, handoffs, attempt evidence, review and outcome; a
cross-project overview reads the boards a person may see and is never a second ticket store.
It was written against `engine/hub/src/board/` on per-project SQLite and `crates/core`. Here it
is rewritten for today: AgentHub is `agenthub` (the `devagotchi/agenthub` submodule) on
Postgres, `board_` tables, its own migrations; the machine side is `engine/machine-engine`
behind `runtime/link`; the hub's machines are `engine/hub/src/machines/`. Its milestones:

| 2026-11-06 | What it asked | Here |
|---|---|---|
| M0 | Native execution baseline: a process outlives its observer, a retry never runs twice, cancel contains children, a broker restart reports ambiguity, contention keeps unrelated files | in Rust: the idempotency key and its retirement (`engine/machine-engine/src/broker/tests/lifecycle.rs:115`), the owned process tree to cancel and deadline (`:198`, `:237`; `execution/platform/linux/tests.rs:92`), a restart marking unsettled work `unknown` and never replaying it (`broker/recover.rs`, `lifecycle.rs:353`), the journal's idempotency evidence (`journal/tests.rs:5`). The rest of the 2026-11-06 machine work is absorbed in its own increments (201 to 204) |
| M1 | Immutable source provenance on imported tickets | F5a, built (increment 205) |
| M2 | Verified repository binding and project identity | F1b |
| M3 | A bounded, read-only scanner with evidence | F5b |
| M4 | A reviewable import and project settings UI | F5c |
| M5 | The provider choice on a designated host, and its health | F19 |
| M6 | An audited SSH bootstrap of a host | F20 |
| M7 | Automatic attempt, session and evidence provenance | F8b |
| M8 | An authorized overview of active projects | F21 |
| M9 | Release and recovery | F22, rewritten for alpha: the hostile and concurrent cases stay; rollback, backup and restore drills arrive with promotion ([lifecycle](../../docs/lifecycle.md)) |

Its rules for scanning and importing (F5b, F5c), kept:

1. Check the project's binding and read permission before discovery; keep the repository's
   identity apart from a machine-local path. Scanning sends no source to another service.
2. Discover inside a configured allowlist, bounded in depth, file count and bytes. Skip VCS
   objects, generated and vendored folders and secrets by default; follow no symlink out of the
   root; record what was excluded, unreadable or over budget.
3. Parse known forms deterministically first. An AI suggestion is a candidate with evidence and
   a confidence, never an instruction. The same revision scanned twice gives the same
   candidates. Repeated headings or items need the person's mapping, not a guessed merge.
4. Preview a manifest (revision, parser version, item evidence, proposed mapping); check the
   revision and digests again before commit and refuse a stale preview. A batch commits whole,
   or keeps a receipt with each item's outcome; never half a board. The project's own settings
   are kept.
5. A re-scan maps unchanged items to their tickets; an edited source item is a conflict for the
   person; done work is never reopened, a claimed ticket never changed, a note never erased; a
   deleted source item is flagged, its ticket kept. F5a (B15) counts the revision in the
   creation digest, so F5b must settle how an unchanged item at a later commit maps to its
   ticket before `board scan` passes origins.

Its rules for hosts (F19, F20, F8b), kept: the way in is the authenticated hub and the
machine's outbound link, never a new public port. SSH is a narrow bootstrap and diagnostic
path, not another owner of jobs: a known host key, a target the person chose, refused on a
changed key; nothing widens a firewall, makes a lasting credential or accepts an installer's
warning on its own. A session is bound to the account and seat, project, host and workspace,
with secrets redacted. A reconnect keeps the operation's identity and reports an uncertain
effect before any replay. A provider is found by what the host can actually run, never by a
product name or an API key being present.

The outcome these rows add up to: a person picks a repository and a host, reviews the work
found in its plans, and gets one board; a provider they chose works its tickets through the
broker on that host; every ticket keeps its origin, claims, handoffs, attempt evidence, review
and outcome. F5a alone, or a scanner tested against a stand-in, is not that feature.

## Milestones and tasks

State from `main` at `ed818cf`. F-numbers are the factory board plan's; new items continue it.

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| F1 | Project boundary: walk up to `.devagotchi/` (CLI does this today), git-remote fallback, areas (`area.json`), one-step `devagotchi board init` | D4 | CLI tests; a worktree and a fresh clone land on the same board | partial |
| F1b | **A project is a verified repository** (2026-11-06 M2; [agent board](../../../docs/agent-board.md) 21c, B14): `board_bind` and `board bind` (`agenthub/src/cli/bind.rs`, today by project name only) take the repository: the provider's repository id and the normalized remote, kept apart from the local checkout root and the host's workspace path (`.devagotchi/config.json`, `agenthub/src/project.rs`). The hub checks the repository exists and the person can reach it before it binds; the same repository or workspace never silently makes a second project; rename, transfer and rebind are explicit and on the board's chain; a board made before its binding is bound by an explicit step, never guessed. Pre-release: existing boards are not migrated; they bind again | F1, F5a | AgentHub tests on Postgres: a repository the person cannot reach is refused with how to link one; a second clone and a worktree of one repository land on one project; a rebind is on the chain; another account's repository is refused | not started |
| F4 | Completion gated on checks: a ticket carries checks (acceptance criteria and an optional verify command); `board_check` records each result with evidence; `board_complete` is refused while a required check is not green, with the exact fix in the refusal; review stays with someone other than the worker; the Stop hook refuses twice, then hands off with a note | F1, D5 | MCP and hosted tests on Postgres and SQLite: refused with a failing check, accepted when green, worker cannot review own ticket | not started |
| F4b | Plan before code: an optional plan step on claim; the plan goes to the phone as an approval; standing approvals cover low-risk lanes | F4 | approval tests; phone page | not started |
| F2 | Prime everywhere: re-prime after compaction (`SessionStart` with source `compact`), hooks for Gemini CLI and Cursor, installed from the connector catalog (`engine/connectors`) | F1 | hook tests per client; `tests/clients_oauth.py` for the clients CI runs | partial (Claude Code and Codex: session start, tool use, stop) |
| F10 | Owner and delegate: each ticket has the accountable person and the agent holding it; filter by either | F4 | board tests; ticket page | not started |
| F11 | Live activity line: heartbeats carry what the agent is doing now; the ticket page shows it; a person's reply lands in that agent's inbox | F10 | inbox tests; phone P2b | not started |
| F3 | Write gate, tunable modes (observe, guide, enforce) | F2, D-decision 2 of the factory plan | hook tests | not started |
| F5 | Defaults library and importers (markdown plans and TODOs first, then GitHub Issues) | F1 | parser fixtures | partial: `board scan` (increment 189), the library as rule files, preview, import once by fingerprint, local and hub boards; GitHub Issues and the device proof remain; the scanner's remainder is F5b and F5c |
| F5a | Import once, by origin (M1 of the 2026-11-06 project-agent delivery plan): `board_create.origin` / `board new --origin` / `NewTicket.source` names `{path, item, revision?, content_sha256?}`; `(path, item)` unique per board (AgentHub migration 0002 `board_sources`, append-only, RLS); principal, time and a version-1 normalized creation digest recorded once with the ticket, deps and event in one transaction; an exact repeat returns the ticket as it is (no event, no notification, lease and edits kept); changed inputs, a new revision included, are `source_conflict` with nothing written; `board_get`, `board show`, export carry `provenance` ([board contract §10](../../../docs/agent-board.md), B15). `board scan --import` still dedupes by fingerprint: wiring it to origins (with the line number out of its description) is F5b | F5 | `agenthub/src/tests/sources.rs` (repeat after reopen, edit and claim; 8 changed inputs conflict; no notification; other board isolated; 6 concurrent tokio importers make 1 ticket and 1 event; invalid origins and a bad discovered-from write nothing; raw UPDATE, DELETE, ON CONFLICT DO UPDATE refused; normalized repeat after a changed default or other anchors; the digest's fixed byte form); `cli/tests/sources.rs` (local lifecycle, exit 2); `engine/hub/src/agenthub/tests/sourced.rs` (two accounts over HTTP-MCP, a repeat from another session, a forged `recorded_by` refused, not found across accounts by id, the bound CLI lifecycle and export); `runtime/devagotchi/tests/board.rs` (six concurrent processes, then the lifecycle with a process per step). Run locally on the test Postgres, 2026-11-18; AgentHub's CI green on devagotchi/agenthub#7 | built (increment 205) |
| F5b | **The scan with evidence** (2026-11-06 M3, under the scanning rules above): `board scan` (`agenthub/src/scan/`) gives each candidate its path, the source revision and digest, an explicit item key, the parser version and the evidence ranges, and `board scan --import` passes them as the ticket's origin (F5a, the line number out of the generated description); it runs within the allowlist and budgets, follows no symlink out of the root, never runs anything a document says, and flags unsupported and ambiguous items. On a bound host it reads through the machine engine's file tools, read-only, under that machine's grants. An MCP `board_scan` tool gives the same result. Open decision (B15): today a new revision alone is `source_conflict`, so an unchanged item rescanned at a later commit conflicts, where scanning constraint 5 says it maps to its ticket; this row decides how a moved revision is reconciled | F5, F5a, F1b | scan fixtures: traversal, a symlink out of the root, ignore rules and the size budget each tested; an ambiguous item flagged; the same revision scanned twice gives the same candidates; two concurrent `scan --import` runs make one ticket per finding; a rescan at a later commit with unchanged items writes nothing and conflicts nothing; a document that says to run something runs nothing | not started |
| F5c | **A reviewable import and project settings** (2026-11-06 M4): the board's pages (desktop and phone) preview what a scan would create, link or conflict, let the person pick items and commit a frozen manifest (repository revision, parser version, item evidence, proposed mapping) whose revisions and digests are checked again at commit, so a stale scan is refused; the batch is atomic or keeps a durable, resumable receipt with each item's outcome; a double click or a retry makes one import; a cancel writes nothing; per-project workflow, labels, priorities and mappings persist. A ticked checkbox may propose a completion; it never completes a ticket silently | F5b | page and AgentHub tests on Postgres for each of those cases; a browser test of an interrupted import; the operator imports a real repository's plan on the phone | not started |
| F6 | Hierarchy, configurable (baseline 1 default, baseline 2 selectable) | F1 | settings tests; UI | not started |
| F8 | Proof of work (diff or PR, test results, preview) attached at completion; advisory file reservations; fan-out of one parent ticket to child tickets per repository or machine, rolling up to one review | F4 | board tests; review queue shows them | not started |
| F8b | **Attempt provenance, recorded by the system** (2026-11-06 M7): ticket and fence, then the run's intent, host and workspace, then the machine engine's operation (its id and idempotency key, `engine/machine-engine/src/journal`), then the output, test and change references, each with the hub's timestamps and identity, on the board's chain. The [task-graph contract](../../../docs/task-graph-and-mesh-contract.md) owns those identities: a ticket, a source item, an agent session, a broker operation and a process are different things. Evidence under a stale fence or from another project is refused; the reviewer (not the worker) sees the real artifacts; what an agent says about itself is labelled as such and never proves completion alone (F4's checks) | F4, F5a, F19 | AgentHub and hub tests on Postgres: an operation's evidence lands on its ticket; a stale fence's evidence and another project's are refused; a completion backed only by the agent's word stays refused | not started |
| F9 | Outcomes per agent and client: accept rate, send-back rate, claim-to-done time, expired leases, cost per ticket when the client reports it (ClickHouse) | F4, F10 | warehouse tests; Overview card | not started |
| F12 | Agents as assignees: a roster per board (the account's connected AI clients, agents with a webhook, agents on the board's tickets, any name typed), **Assign to an agent** on the ticket page (desktop and phone), the assignment as a note to that agent so it reaches its inbox and webhook at once; `board_update` already takes an assignee | none | `ticket_tests::a_person_assigns_a_ticket_to_an_agent_and_the_agent_is_told` | done (increment 139) |
| F13 | The roster as data, from agent-hub: each agent's role (worker, reviewer, observer), WIP limit, last seen, and a stable id every metric and audit entry keys on; an Agents page | F12 | board tests; page | not started |
| F14 | A review agent before the person: a ticket an agent completes goes to a reviewer agent (not the worker) first, then to the phone's review queue with its findings | F4, F13 | board tests; review queue | not started |
| F15 | One worktree per claimed ticket on the machine that works it, and advisory file ownership between tickets | F8 | broker and board tests | not started |
| F16 | Tickets that start from events (an issue labelled, a check failing, a schedule), read-only by default, outputs checked before they apply | F4 | board tests | not started |
| F17 | A policy decision service, from agent-hub: an optional URL the hub asks before claim, complete and approval, failing closed | F4 | board tests with a stand-in | not started |
| F18 | **A notification outbox and links that expire** (agent-hub `adca8b7`, `docs/agenthub-packaging-slice.md` "Proposed missing contracts"; absorbed 2026-11-18). Already in main, not rebuilt: a decision bound to the version the person saw, and replays refused (`agenthub/src/hosted/approvals.rs:160-164`, `already_decided` and the version check); decide is POST only and same-origin (`engine/hub/src/web/routes.rs:76`, `web/approvals.rs:411`) and checks the board is the person's (`web/approvals.rs:430-437`); push already tells about approvals. Added: every push or mail about an approval or ticket goes through a durable outbox whose delivery outcome lands on the chain, idempotently, and carries an authenticated HTTPS deep link with minimal metadata that expires; no approval by email reply. `web/approvals.rs` is at its ratchet (713), so the outbox lands in `agenthub` first (its own migrations) | F4b | tests: an outbox row per notification with its outcome on the chain; an expired link opens a refusal page and decides nothing; a resend is idempotent | not started |
| F19 | **The AI provider chosen on a designated host** (2026-11-06 M5): a project names its host (one of the person's machines) and the provider that works its tickets there. The choices are what the host can actually run, found on the host (which AI clients are installed and signed in, their versions), through adapters pinned by version (the catalog in `engine/connectors`, as [AI sessions](ai-sessions.md) S2 reads sessions); neither a product name nor an API key proves a client can run there. It builds on `machine_inspect` (increment 204), which reports what a machine's broker answers today but not which AI clients it can run. Model, settings and allowed workspace are explicit; a choice that is unsupported or not signed in shows as unavailable, never hidden; a launch goes through the broker's admission and audit like any operation; no credential ever reaches a ticket or a log | F1b; M0 (met, above) | tests against recorded host fixtures: an installed and signed-in client offered, a missing or signed-out one shown unavailable; a launch audited; a ticket and the logs searched for a planted credential find none | not started |
| F20 | **An audited SSH bootstrap** (2026-11-06 M6, under the host rules above) for a host the person can reach but not type on: they pick the host, its key is pinned, the account and least-privilege scope are explicit; a plan goes to their phone as an approval before any install or service change; the bootstrap can run again without harm; it verifies the installed binary against the release's sums (as `machines/join/join.sh` does), the host's binding, its grants and its outbound link; an interrupted run, an unreachable host or a changed key never reads as success. SSH bootstraps and diagnoses only; it never owns a job. The SSH session runs as an operation on a machine the person already has, through its broker and grants, not from the hub process. The one-line join ([free edition](free-edition.md) D2) stays the first way in; [cloud edition](cloud-edition.md) C9's custom host reuses this | F19, F4b, free-edition D2 | tests against a throwaway SSH server: a changed host key refused; no install before the approval; a second run changes nothing; an interrupted run reports what is known | not started |
| F21 | **An overview of the person's active projects** (2026-11-06 M8; [agent board](../../../docs/agent-board.md) 21g): a read model over the boards the person may see, never a second ticket store, building on `/app/boards` (`engine/hub/src/web/app/mod.rs`): bounded pages and a stated freshness; counts equal each board's own; no ticket shown as owned twice; revoked access disappears; an offline or partial board is said to be so; a drill-down keeps the project's scope. On the phone's Overview (`engine/hub/src/web/app/home.rs`) and the desktop | F1b, F5c, F8b | hub tests on Postgres: counts match each board; a revoked board vanishes from the next page; a board whose store does not answer shows as partial | not started |
| F22 | **The project path, proven end to end** (2026-11-06 M9, for alpha): tenant isolation on a multi-tenant hub, hostile documents, stale keys, lost connections, simultaneous imports and a host restart, each tested on Linux and Windows; the real pages' interrupted flows; the release signed and pinned ([self-hosted](self-hosted-and-provenance.md) S3). Rollback, backup and restore drills are not alpha work; they arrive with promotion | F1b to F21 | the suite green in CI on both OSes; the operator's run on Texas and the phone | not started |
| F23 | **"Make this a Devagotchi project?"** (operator, 2026-11-18): when the person starts a new project (a new repository or folder seen by the plugin's local signals, [plugins](plugins.md) G13, or by `devagotchi` itself), Devagotchi asks once whether it should be a Devagotchi project, with AgentHub. Yes makes and binds its board (`board init`, F1b); no is remembered for that project. Either answer can be changed later from the project's settings: a project turned off stops its hooks and leaves its board's history as it was, and deleting the board is a separate, explicit step | F1b, plugins G13 | tests: a new repository asks once; no is remembered; turning it off in the project's settings silences its hooks and keeps the board's chain | not started |

## Risks and approved decisions

Ruled 2026-11-09: the boundary is set first; enforcement fluid and tunable; a library of the
defaults AI tools write; hierarchy configurable. Kept here. The research ranks the completion
gate highest, so it follows F1 directly. Status lagging reality (a known failure of agent task
lists) is why every gate reads the board of record, never a cache. Notes and issue text stay
untrusted data (`agenthub/issue_lifecycle/mod.rs` protocol).

## Verification receipts

Each milestone records its own in its PR. Plans only here.

2026-11-18, increment 206 (plans only): the Go hub's refs read in `D:\localdev\agent-hub`
(`git log ca626db..preserve/packaging-2026-10-25`: `3ca781f`, `34b15b9`, `016883d`, `085b588`,
`97172d9`, `adca8b7`; `git diff --stat ca626db 97172d9 -- docs/`: 08, 26, 27, 39c changed);
the 2026-11-06 plan read from `preserve/nov6-uncommitted`; every `file:line` cited in this plan,
free edition D1b to H1, self-hosted S3, connections X10, cloud C9 to C11, plugins G10 and
editions checked against `main` at `219e008` by reading those lines. No code changed.

## Delivery

Implementation: F1 and F2 partial; F5a built (increment 205); F12 done.
Validation: per milestone. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
