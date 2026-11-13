# Devagotchi agent board: tickets any subscription can claim, work, annotate and hand off

Revision: **2026-11-09 UTC**. Status: **hosted CLI/MCP/web use native Postgres; standalone SQLite retained; offline queue pending**.
Owner: `crates/hub/src/agenthub/`. This document owns the board contract; the
[architecture master](product-and-platform-plan.md) owns product decisions and sequencing; the
[task-graph contract](task-graph-and-mesh-contract.md) owns execution graphs, which are a different thing.

Storage/offline amendment accepted 2026-11-09: [ADR 0001](adr/0001-hosted-board-postgres-and-clickhouse.md)
puts hosted authority in scoped Postgres with required ClickHouse analytics;
[ADR 0002](adr/0002-bound-board-offline-cache-and-queued-drafts.md) allows explicitly pending
offline discovery/new tickets/notes/drafts, never offline shared ownership or completion.
The user authorized direct recording of these two ADRs while the component was unavailable.
Live hub boards still use SQLite: migrations/import are implemented, runtime cutover is not.
Earlier packet receipts below describe the legacy backend, not completed amended scope.

## 1. What a ticket is, and is not

A **ticket** is a unit of coordination and memory between people and agents: "port the claim
protocol", "rotate the key on the SMB share", "review Codex's installer PR". It is claimed under a
lease, worked, annotated, handed off, reviewed by someone else, and closed. It travels with the
project. Its history is a hash-chained event log any reader can verify.

A **broker operation** (and, when the task-graph packets land, a task graph) is a unit of
execution: one admitted command with its own identity, output and settlement. A ticket may link to
the operations that worked it; it never admits, schedules or cancels one. The hub never owns jobs
(ruling 2026-10-21). The two meet only through an attempt link: "ticket X was worked by operation Y
on machine Z", recorded after the fact by the agent or the broker.

The Go `agent-hub` (shippinAI/agent-hub, Postgres) proved this shape on one box in October 2026:
leases with fences, a ready queue over dependencies, reviewer ≠ author as data, notes and handoffs,
an append-only chain. The protocol lives in this Rust hub, per project. ADR 0001 supersedes
the original hosted-SQLite placement; standalone local boards retain SQLite. What the
protocol adds comes from what works elsewhere (§7).

## 2. Boundary test (master §4)

| Question | Answer |
|---|---|
| Responsibility it owns | Tickets, dependencies, claims/leases, notes, handoffs, the board's event chain, per project. |
| Existing owner inspected | `crates/hub` (bastion: subjects, audit chain, MCP surface, web); `crates/core/journal` (operations). The Go `agent-hub` store (`server/internal/store`, `docs/spec.md`). |
| Why extension is adequate | The hub already authenticates actors, owns account/tenant storage and RLS, keeps audit history and serves MCP/web. Board transactions extend that Postgres owner; no second task service. ClickHouse is the explicitly selected analytics dependency, never a coordination authority. |
| What it replaces | The Go hub's tasks/notes/deps/leases/events, ported not run beside; the Go hub freezes behind a superseded banner once parity tests pass (increment 11). |
| Authoritative state | Hosted boards: tenant/project-scoped Postgres tables in the existing hub `Store` (ADR 0001). Standalone boards: `<project>/.devagotchi/board.sqlite3`. Bound client SQLite is cache/pending operations only (ADR 0002). `board.jsonl` is an export, never authority. Live hosted routing remains SQLite until verified cutover. |
| Dependencies and failure | Existing SQLx Postgres storage for hosted transactions; rusqlite for standalone/cache. No silent local fallback when the hub/database is down. Durable outbox to required ClickHouse exposes export lag/backpressure. Broken chains are reported, never silently repaired. |
| Acceptance | `crates/hub/src/agenthub/tests.rs`: lanes, fencing after expiry, reaper → attention, readiness through `blocks`, assignee priority, WIP, handoff, force-release, version conflicts, transitions, filters, append-only triggers, chain tampering, workflow replacement, discovered work. |

Rejected: a board per machine (tickets outlive machines), Dolt or another versioned database.
The original rejection of centralized hosted board storage is superseded by ADR 0001;
authorized client-side offline access is covered separately by ADR 0002.

## 3. Records

```text
Ticket {
  id: "<prefix>-<hash>" | "<parent-id>.<n>",   -- hash ids never collide across machines; children are hierarchical
  kind: "task" | "epic" | "message",
  title, description, state, priority (lower = sooner), labels[],
  parent: Id | null, assignee: Principal | null,
  holder: Principal | null, lease_id, lease_ttl_ms, lease_expires_at, heartbeat_at,
  fence: integer (global, strictly increasing, never reused),
  version: integer (optimistic concurrency), expiry_count, last_worker,
  progress: JSON | null, created_by, created_at, updated_at, closed_at,
  blocked_by[]: computed (open `blocks` dependencies), lease_expired: computed
}
Dep   { ticket, depends_on, kind: blocks | parent | related | discovered-from | duplicates | supersedes }
Note  { seq, ticket, author, kind, body, to_principal, fence, created_at }          -- append-only
Event { seq, at, actor, action, ticket, data, fence, prev_hash, hash }              -- append-only, hash-chained
Workflow { states[{key,label,kind}], lanes[{queue,active,complete_to,release_to,distinct_from_previous}], transitions[], initial_state, attention_state }
Settings { lease_ttl_ms, lease_min_ttl_ms, lease_max_ttl_ms, max_lease_expiries, wip_limit_default, default_priority }
```

Only `blocks` affects readiness. `parent` gives epics their children and hierarchical ids
(`deva-a3f8`, `deva-a3f8.1`, `deva-a3f8.1.1`). `discovered-from` records work an agent found while
doing something else, so nothing is lost and nothing is invented. Workflow and settings are data
rows in the board, validated before they land; state *kinds* (`queue`, `active`, `review`,
`blocked`, `done`, `attention`) are what the code reasons about, names are the operator's.

## 4. Claim and lease protocol (ported; the Go hub's `docs/spec.md` §3 holds)

1. `claim(id, ttl?)` / `claim_next(filter)`: one transaction. A candidate is a lane queue state
   (or a lapsed lease), has no open `blocks`, is unassigned or assigned to the caller. Checks the
   lane's `distinct_from_previous` (reviewer ≠ author), then WIP (`actor.wip_limit ??
   settings.wip_limit_default`; 0 = unlimited). Sets holder, a fresh lease id, `lease_expires_at =
   now + ttl` (clamped to the settings' bounds), `fence = next`, `state = lane.active`, `version + 1`.
   Re-claiming what you hold returns the current lease, nothing changes.
2. `heartbeat(id, fence, progress?)`: holder and fence must match, lease must be live; extends by
   the lease's ttl. Recommended interval: `heartbeat_every_ms` = ttl/3.
3. Every mutation of a held ticket carries the current fence. Wrong fence → `stale_fence`; not the
   holder → `not_holder`; lapsed → `lease_expired`.
4. Expiry (the hub's reaper, which sleeps until the next lease lapses or a board changes, then
   expires what lapsed; or lazily inside a claim): back to the lane's `release_to`, `expiry_count + 1`,
   event `lease.expired`; at `max_lease_expiries` the ticket goes to the workflow's attention
   state. The fence is burned, so a worker that wakes up late is rejected, not merged.
5. `release(id, fence, reason)` → `release_to`. `complete(id, fence, summary)` → `complete_to`,
   `last_worker = me`, summary as a note. `handoff(id, fence, note, to?)` = handoff note +
   release + assignee, one transaction.
6. `force_release(id, reason)`: a human principal (code rule) with a reason ≥ 3 characters; the
   event keeps the previous holder and fence.
7. `update(id, version, patch)`: optimistic concurrency; a state change must be a declared
   transition, never into an active state, never on a held ticket (`use_lease_ops`).
8. Every mutation appends its event in the same transaction; the chain is
   `sha256(prev ‖ canonical{seq, at, actor, action, ticket, data, fence})`.

Error codes are stable across CLI, MCP and REST: `not_found`, `invalid`, `held_by_other`,
`not_claimable`, `blocked`, `distinct_required`, `wip_limit`, `stale_fence`, `not_holder`,
`lease_expired`, `version_conflict`, `transition_not_allowed`, `use_lease_ops`, `humans_only`,
`dependency_cycle`, `parent_closed`, `board_mismatch`, `board_version`.

## 5. Where the board lives and how an agent starts it (increment 8)

```text
<project>/
  .devagotchi/
    board.sqlite3        the truth (git-ignored; one writer at a time: the CLI as the OS user, or the hub)
    board.jsonl          `board export`: one JSON line per ticket with its deps and notes (commit it if you want diffs)
    config.json          project name and id prefix; later the hub endpoint and which seats may claim here
    claims/<actor>.json  the CLI's claim cache ({id → fence, version, lease expiry}) so agents never copy fences
    .gitignore           ignores the database, its WAL and the claim caches
  AGENTS.md              gains one fenced block (markers `devagotchi-board:start/end`) pointing at `board prime`
```

`devagotchi board init [--project NAME] [--prefix P] [--agents-md]` (increment 8a, built; `devagotchi board` since 13b) creates
the directory and, with `--agents-md`, appends the block once. `board prime` is what an agent
runs, or has run for it, at session start: under fifty lines — the tickets it holds (fence, lease
left), what is ready for it, tickets that need a person, notes addressed to it, and the one-line
protocol. `board onboard` prints the block for pasting elsewhere; `board agent` prints the whole
workflow. Identity on this surface is the OS user (the board file is theirs, as the broker's trust
scope is): `--as` or `DEVAGOTCHI_ACTOR` names an agent, which may not force-release; the bare OS
user is the person. The same directory, named in the hub's `boards` config, is what the hub's
`board_*` tools open (§6). Still to come in 8b: registering the project with the hub from here,
the MCP snippets for each CLI, and `--hooks` for the hook pack (§7).

## 6. Surfaces (increment 8)

Two faces, one store. The command line (§5) is for agents on the host that holds the project;
the hub's MCP tools are for agents anywhere. Both open `<project>/.devagotchi/board.sqlite3`
(WAL, five-second busy timeout), so an agent on the hub's host and one across the world see one
board, one chain.

**MCP tools on the hub's `/mcp` (8b, built).** The hub config names the boards it serves and who
may work on them, the way it names brokers:

```json
"boards": [ { "project": "devagotchi", "dir": "/srv/projects/devagotchi" } ],
"grants": [
  { "subject": "300000000000000001", "boards": ["devagotchi"], "human": true },
  { "subject": "claude-on-gus",      "boards": ["devagotchi"] }
]
```

`project` must be the name the board was initialised with (ids embed its prefix; a mismatch is
refused at first use), `dir` is absolute, and an uninitialised directory is `no_board`, never
silently created. A hub may serve boards and no broker, or nothing yet (whoami and `/healthz`
still answer, so an installer can bring it up before its first board or machine). The actor is the authenticated subject;
it is a person only when its grant says `human` or it is the owner (`whoami` reports `human` and
`boards`). Agents never are, whatever they claim. No grant, no board: `not_granted`; a project the
hub does not serve: `unknown_board`.

Nineteen tools (with `board_inbox` and `board_wake`), each taking `project`: `board_prime` (start here), `board_ready`, `board_list`,
`board_get`, `board_create`, `board_claim` (by id, or the best ready ticket when `id` is omitted;
`claimed: false` when nothing is ready), `board_heartbeat`, `board_update`, `board_note`,
`board_handoff`, `board_release`, `board_complete`, `board_dep`, `board_force_release` (people
only), `board_events`, `board_wait` (held until the board commits an event, at most 25 s), `board_verify`,
and `board_settings {project, repo?, environments?}`: the project's repo (an https URL; new tickets
carry it) and its environments (1 to 20 lowercase names), set by the owning account, its AI, or a
person granted on a shared board, each change on the chain as `board.settings_changed`; with
neither field it returns the settings. `devagotchi board settings [--repo URL] [--env a,b,c]` is the
same on the command line, locally or on the hub for a bound project.
`board_wait` and `approval_wait` sleep on the board's commit signal: every write that records an
event wakes them, nothing re-reads the store on a timer. A write by another process (the board
command line on a project not bound to this hub) is seen at the next change or when the wait ends.
Outputs are the store's records with JSON Schemas; a refusal is `{code, message, ticket?, next}`
with the store's stable code (§4) and what to call next, as the CLI's `next` does. The fence may
be omitted on a held ticket: the hub keeps the fence it last handed that subject for that ticket
(claim or heartbeat), as the CLI's claim cache does, and forgets it on complete, handoff or
release; the store still checks whatever is passed, so a lost lease is refused either way. Every
call lands on the hub's audit chain (`tool`, the project under `broker`, the ticket under
`operation_id`, the outcome), and a lapsed lease is reaped every minute as well as lazily on claim.
`tests/board_probe.py` works a board as three subscriptions (a person, two agents) and then reads
it back with the command line; `tests/e2e.py` runs it.

**The native hook handler.** `devagotchi board hooks [--for claude-code|codex|all]`
prints configuration calling `devagotchi board hook PHASE --as AGENT` directly on
Windows and Unix. Existing `.devagotchi/hooks/*.sh` paths become thin compatibility
launchers; they no longer parse JSON with shell tools or suppress errors.
The handler bounds stdin JSON to 256 KiB, validates event/phase/session identity,
uses the same local/remote board face as the CLI, and never opens a fallback local board
for a configured hub.

- `SessionStart` (`session-start`) returns event-specific context JSON: prime, current
  held-ticket records/notes, and instructions to page the selected issue's
  `board_issue_context`. A safe session ID is appended to `CLAUDE_ENV_FILE` when supplied.
- `PostToolUse` (`beat`) extends held leases once their remaining time reaches two
  heartbeat intervals. This uses authoritative lease time, not lossy marker files.
- First `Stop` (`stop`) exits 2 with actionable stderr when holding work. The agent must
  checkpoint evidence and complete or hand off explicitly.
- Recursive Stop (`stop_hook_active=true`) and `SessionEnd` (`session-end`) report
  interrupted held work with a nonblocking error (exit 5), rather than loop, silently
  succeed, or complete/release work belonging to another session sharing the same seat.
  Explicit handoff/release or database-time expiry remains authoritative.

Claude Code (`.claude/settings.json`) and Codex (`.codex/hooks.json`) get printed
snippets. The `devagotchi-agenthub` plugin (`plugins/`, [plan](../.ai/dev/plans/plugins.md))
ships the same four hooks as `board hook PHASE --as auto --if-board`: the agent is the client
whose plugin runs it, and a project without a board answers `{}`. Keep unrelated entries when merging. Current Codex documentation requires
project trust and review of exact hook definitions using `/hooks`; changed definitions
need review again. Never bypass that trust. The binary must be on the client's PATH.
Subscription UIs without interception remain manual/workflow-enforced; MCP cannot force
model invocation. Real pinned-client install/start/resume/stop testing is still open.
`board bind --shared --hub URL --project NAME` attaches an authorized configured shared
board, using its actual prefix without creating a personal namespace.
`board held` and `board beat` with no id remain available. The skill that teaches all
of this to a seat lives in agent-skills (`hubs/intelligence/devagotchi-board/`), with the MCP
connection snippet per CLI.

Not built yet from the 8b list: `request_id` idempotent retries on writes (the store's version
check and `already_held` cover the common retries) and project registration from the CLI
(`board init --hub`). They stay in packet 8b.

**Account boards and seats (15a; D23, ADR 0001).** The hosted resolver now uses native
Postgres transactions for account/org and shared boards. Legacy directories remain import
sources, not a second authority. Two kinds of board answer to a project name.
A *configured* board is shared by grant. An *account*
board is the hosted tier's: with

```json
"account_boards": { "free_limit": 1, "unlimited": ["300000000000000003"] }
```

in the hub config (`free_limit` defaults to one, D21's free
account; the owner is always unlimited), `board_bind {project}` makes the calling account's own
board in the tenant-scoped Postgres store — idempotent, `created` says which — and from then
on `project` means that board in every `board_*` tool, from any of the account's machines.
`whoami.boards` lists the account's own boards first, then the shared ones. Refusals:
`not_offered` on a hub without `account_boards`, `plan_limit` at the free limit, `name_taken`
when the name is a shared board the account already reaches (one name never means two boards),
`invalid` for a name that is not `[a-z0-9][a-z0-9-]{0,62}`. An unknown project's message now
names `board_bind` as the way in. Nothing unbinds yet; do not delete board rows manually
because history is append-only. A managed archive/cleanup surface remains pending.

Committed events notify other hub instances transactionally. Board waiters refresh after
listener reconnect, and persisted leases are reaped after restart without opening each
board first. No hosted board creates or falls back to a local SQLite connection.

A caller may name its **seat** with the header `X-Devagotchi-Seat: claude` — the account's own
agent on that machine (the CLI face sets it from `DEVAGOTCHI_ACTOR`; `devagotchi mcp --hub` will
too). On the account's own board the seat is the actor outright (`holder: "claude"`), so one
account's Claude and Codex coordinate as two seats; on a shared board it is `<account>/<seat>`,
so no account can pose as another's seat. A seat is never a person, and the fence memory is kept
per seat. The account is the person on its own board (it may force a seat's lease free); on a
shared board its grant says. `tests/board_probe.py` binds a project for the ungranted subject,
works it from two seats (a claim on one is held against the other), hits the free limit and the
taken name, and shows the unlimited list binding more. `board_list` takes `@me` as a holder or
assignee: the caller as the board names it, since a seat's spelling is the hub's.

**The command line bound to the hub (15b, built; B11).** `devagotchi board bind [--hub URL]
[--project NAME]` asks the hub for the account's board (`board_bind`, idempotent) and writes
`hub` into `.devagotchi/config.json`; from then on every `board` command and every hook script
here runs on the hub as the account on file (`devagotchi login`), through one MCP session
(`account::client`). The agent's name (`--as`, `DEVAGOTCHI_ACTOR`) travels as its seat; a bare OS
user is the account itself. The command line renders the hub's records exactly as the local
store's — one `Face` trait (`agenthub/face.rs`), two implementations (`Board`, `remote::Remote`) —
and the hub's refusals keep their codes and the CLI's `next` hints; `not_signed_in`,
`signed_out` and `hub_unreachable` say what to run. No local store is written for a bound
project; `board unbind` makes the local store the board again (the hub keeps its board). A
project is found by `.devagotchi/config.json`, bound or local. `devagotchi hub board …` is gone:
`devagotchi board` is the one command line. Proof: `board::tests::bound` runs the real hub in
process — two machines of one account, a claim on one held on the other at once, the stale fence
refused by the hub, force by the account, `held`/`beat` resolving `@me`, unbind, and the words
when signed out; `tests/nomen_probe.py` does the same through the binary after a real
`devagotchi login`. Not yet: ADR 0002's authorized offline cache and explicitly pending
operation queue with push/ack/cursor pull — amended packet 15c. The current disconnected
refusals remain honest until that protocol is implemented.

## 7. What the CLI agents can do now, and what the board does for them

Re-probed 2026-10-24 against vendor documentation (chatmaton `docs/51` of 2026-08-15 is stale on two
rows):

| Seat | Multi-turn without a new process | Resume across processes | Hooks | ACP registry |
|---|---|---|---|---|
| Claude Code | yes (stream-json) | `--resume` | 30+ events incl. `SessionStart`, `PreToolUse`, `PostToolUse`, `Stop`, `TeammateIdle`, `TaskCreated/Completed` | Claude Agent (wrapper) |
| Codex | yes (`codex app-server`) | `codex resume` | `SessionStart`, `PreToolUse`, `PostToolUse`, `Stop` (`decision: block` forces continuation) behind `codex_hooks = true` | `codex-acp` |
| Grok | yes (`grok agent stdio`, ACP v1) | `session/load` | — | Grok Build |
| Gemini / Antigravity `agy` | **yes**: `--input-format stream-json` (requires `--output-format stream-json`) | `--continue`, `--conversation <id>` | `.agents/hooks.json`, `transcript.jsonl` | Google Antigravity; Gemini CLI |
| Cursor `agent` | **yes**: hidden `agent acp`; otherwise `create-chat` + `--resume <id>` | `--resume`, `--continue` | `sessionStart`, `stop`, `postToolUse`, `afterFileEdit`, `afterShellExecution` (two response hooks do not fire headless, acknowledged March 2026) | Cursor |

Consequences the board builds on:

- **One seat driver.** All five seats, and 37 more, are in the ACP registry. ACP gives an
  orchestrator `session/new` (with the board's MCP endpoint handed over in the handshake),
  `session/prompt`, `session/update`, `session/request_permission`, `session/cancel`. chatmaton
  already speaks it for grok; the devagotchi broker runs the ACP process as a job. "One-shot
  seat" stops being a category; what remains per vendor is a config row and the policy question
  below.
- **Liveness is the broker's job, not the agent's.** When a seat works a ticket as a broker
  operation, the hub heartbeats the lease while the operation is alive and releases or hands off
  when it settles. An agent that cannot remember to heartbeat still cannot lose a lease by
  forgetting. Symphony does the same with its workspace reconciliation; this is the inverted form.
- **Hook pack, same four hooks on every CLI that has them:** `SessionStart` → `board prime` as
  injected context; `PostToolUse` → `board beat` (debounced to a third of the ttl); `Stop` → the
  GUPP guard: a held ticket without a handoff or completion blocks the stop with the reason
  "hand off or complete <id>"; `TeammateIdle` (Claude teams) → `board claim_next`. Gas Town's
  "if there is work on your hook, run it" as four shell lines.
- **One skill, spec-pure.** `.agents/skills/board/SKILL.md` under the Agent Skills standard
  (Claude Code, Codex, Gemini CLI, Cursor, Kiro, 32+ tools as of May 2026), validated by the
  `agent-skills` repo's CI; per-surface extras live beside it, never in the frontmatter.
- **Agent-friendly CLI contract.** JSON output whenever stdout is not a terminal; `--quiet`;
  distinct exit codes per failure class (0 ok, 2 conflict, 3 refused, 4 not found, 5 unavailable);
  every error carries `next` (what to run); creates are idempotent by `--request-id`; `board
  agent` prints the workflow offline with progressive disclosure. What Speakeasy and Nx measured
  on their own CLIs in 2026: fewer turns, and skills that fire without helping cost more than no
  skill.
- **Vendor sessions are foreign keys.** A ticket records the vendor conversation that worked it
  (`agy --conversation`, `agent --resume`, `codex resume`, `claude --resume`), so the next turn on
  that ticket resumes the same conversation; the board holds the id, never the token
  (chatmaton CM-0.4, CM-0.5).

Policy, stated plainly: Antigravity's own forum carries five unanswered threads (September 7 to
October 19, 2026) asking whether a local orchestrator may launch `agy` under a consumer account; the
one reply that says "fully supported, same entitlements" is not identified as Google staff. Cursor,
Codex and Claude document their headless modes. Treat `agy` orchestration as technically proven and
policy-unconfirmed until Google answers; the seat's config row carries that flag.

## 8. Decision register

A RULED row normally becomes canon through Jesse's ADR component. ADRs 0001/0002 are the
user-authorized direct-recording exception, limited to hosted storage and offline behavior.

| ID | Status | Decision |
|---|---|---|
| B01 | LOCKED; storage amended by ADR 0001 | The Rust hub owns per-project coordination, never jobs. Hosted boards use the existing scoped Postgres boundary; ClickHouse analytics is required. Standalone boards retain SQLite. |
| B02 | ACCEPTED, ADR 0001 | Hosted storage is tenant/project-scoped Postgres. Existing project/account SQLite files are explicit migration sources; standalone local files stay supported. `board.jsonl` remains an export. Never two authoritative copies of a bound board. |
| B03 | PROPOSED | Ids: prefix + hash for roots, hierarchical for children. Alternative: counters (the Go hub), rejected for multi-machine creation. |
| B04 | PROPOSED | Dependency kinds `blocks | parent | related | discovered-from | duplicates | supersedes`; only `blocks` gates readiness. |
| B05 | PROPOSED | Liveness by broker: an operation working a ticket keeps its lease alive; settlement releases or hands off. |
| B06 | RULED 2026-10-24 (chat; ADR pending through the component) | All five seats are driven over ACP, one driver with a config row per vendor; `agy` runs only behind its policy-unconfirmed flag until Google answers. |
| B07 | PROPOSED | Hook pack (prime / beat / GUPP stop guard / idle → claim_next) installed by `board init --hooks`. |
| B08 | DEFERRED | Per-state WIP caps, per-principal budgets, message tickets with threads, path reservations with a pre-commit guard (increment 9). |
| B09 | DEFERRED | Board pages in the hub's web and the one-tap Approve from the agent-board plan (increment 10). |
| B11 | ACCEPTED, ADRs 0001/0002 | The hub arbitrates a bound project's claims, leases, fences, completion and shared workflow. Local SQLite caches authorized reads and queues discovery/new tickets/notes/drafts as pending, never confirmed shared mutations. Client push/ack/cursor pull rechecks permissions, versions and stable request IDs. Account/org board names, seats and plan limits remain unchanged. |
| B10 | RULED 2026-10-24 (chat; ADR pending through the component) | Approvals reach the user by phone push and email first, on both iPhone and Android; Slack and Discord follow as one-click connects. Which channels, in what order and for which kinds of request are per-user settings whose defaults need no setup. Every approval step (requested, seen, approved or denied, running, verified, done or rolled back) is an event the requesting agent continues from; nothing polls. |
| B12 | RULED 2026-10-24 (chat, 17:13 EDT; ADR pending through the component) | Connecting never lands on an empty board: a board the hub opens for the first time with no tickets gets a Getting started ticket in the attention state (the steps: machines, phone notifications with a one-tap test approval, one small job, close it). Account boards (B11) get it too, when `board_bind` first makes them. |
| B13 | RULED 2026-10-24 (chat; ADR pending through the component) | Multi-tenant by default: a project's board belongs to the org (tenant) the identity provider proves the account belongs to, and the org manages it. Every member and the members' agents work it; another org never sees it. Tokens are least-privilege: a sign-in asks only for who the person is, a refresh token, the org and the hub's audience; roles and per-tool scopes follow (21e). The Shippin identity works with PAM on the machines (21f). |
| B14 | RULED 2026-10-24 (chat; ADR pending through the component) | A project starts when an AI or a person names it, and it must be linked to a real repository (GitHub, GitLab, or wherever the person's code lives). All notes and all chats of the work are saved to that project, so every session has its context (21c, 21d). |

## 9. Packets

| Increment | Scope | Exit |
|---|---|---|
| 7 (this one) | `crates/hub/src/agenthub/`: schema, ids, workflow, store, leases, reaper, chain; this document | `cargo nextest run -p devagotchi-hub` green; §2 acceptance list |
| 8a (built) | `devagotchi board` CLI (`devagotchi-hub board` until 13b): `init` (+ AGENTS.md block), `prime`, `onboard`, `agent`, `ready`, `ls`, `show`, `new`, `claim`, `beat`, `note`, `done`, `handoff`, `release`, `force`, `set`, `dep`, `reap`, `verify`, `export`; JSON off a pipe, `--quiet`, exit codes by class, `next` on every error, claim cache | five CLI tests: init idempotence, the loop from a subdirectory with a cached fence, exit codes and hints, `prime` ≤ 50 lines, export + verify |
| 8b (in) | MCP tools on `/mcp` (one board per `project`; **built**, §6: `boards` config, grants with `boards`/`human`, 17 tools, fence memory, audit, reaper, `tests/board_probe.py`); the hook pack (`board hooks`, **built**); the skill with the MCP snippet per CLI (**built**, agent-skills); still open: project registration with the hub, `request_id` | `tests/e2e.py` works a board over MCP as a person and two agents (**green**); the hook scripts exercised against the CLI (stop refused once while holding, beat throttled) in the CLI tests and by hand; on Claude Code and Codex themselves: open |
| 9 | Broker liveness (B05), attempts link, message tickets, path reservations, per-state WIP, budgets | A lease survives an agent that never heartbeats while its operation runs; two agents cannot edit one reserved path |
| 10 | Web: `/board`, `/board/<id>`, Approve; approval events; phone push (installable web app, iPhone and Android) and email (B10). **10a built:** approvals as board tickets (`agenthub/approval.rs`), `approval_request` / `approval_wait` on `/mcp`, `/approvals` with one-tap Approve / Deny. **10b built:** Web Push (RFC 8291 / 8292 on aws-lc-rs), "Notify this phone" on `/approvals`, service worker and installable manifest. **10d built:** a phone stays reachable: every visit to `/approvals` re-saves (renews) its subscription, the service worker re-subscribes on `pushsubscriptionchange`, a busy or unreachable push service is tried again (2 s, 10 s), each push outcome is logged by host, and `approval_request` returns `phones` so an agent that reached nobody hands the user the link. Next: 10c passkey and email | Phone-width pages served by the existing axum app; a request an agent raises reaches a phone and an inbox, one tap approves, and the agent continues on the event |
| 15a/15b (native Postgres runtime built) | Account/org/shared boards, seats and atomic project limits; bound CLI uses the same authority | Real-Postgres HTTP MCP/web isolation, lease/fence, rollback, peer notifications and restart reaping verified; legacy staging imports are not automatically owner-bound |
| 15c (amended; pending) | ADR 0002: authorized cached reads, durable pending discovery/new tickets/notes/drafts and client push/ack/cursor pull; no offline claim/completion | Disconnect/restart/lost-ack, request dedupe, conflicts, revocation and cursor-resnapshot tests; explicit offline/pending UI |
| 21a (built) | Org boards (B13): the identity carries the org (`urn:nomen:iam:user:resourceowner:id`; web and CLI sign-in and the RFC 9728 metadata ask for it); `board_bind` makes the org's board, which every member finds; seats are `<account>/<seat>` on it; the free limit counts the org's projects; `whoami.org` | `board::mcp::tests::org_boards_belong_to_the_org_and_every_member_works_them`; `tests/board_probe.py` org section (two members share, another org never sees it) |
| 21b | Approvals and push on org and account boards: `/approvals` lists their pending approvals, a push reaches the org's members who turned on notifications | an agent's approval on an org board buzzes another member's phone |
| 21c | A project is a repo (B14): `board_bind` takes the repository, the hub checks it exists and the person can reach it, `whoami` and the projects view show it | a bind with no reachable repo is refused with how to link one |
| 21d | Notes and chats saved to the project (B14): a session's transcript and notes land on the project's board, findable by the next session. [DG-CAPTURE-1](../.ai/docs/capture.md) now implements consented **private local** session capture; it is not automatic hosted transcript publication, complete provider coverage or B14 completion. | a new chat on the project primes with the last session's notes; sharing obeys explicit content/recipient authorization |
| 21e | Roles and least-privilege scopes from Nomen: org admins make and archive projects; agents' tokens carry only the tools they use | a member without the admin role cannot archive; a read-only token cannot claim |
| 21f | PAM: the Shippin identity is the login on the machines; jobs run as the person's mapped account | a job started by a member runs as that member's OS account |
| 21g | Projects view: every project of the org with what is held, ready and waiting, to cycle between them and see parallel work at a glance | the org's three projects show their agents working in parallel |
| 11 | Freeze the Go `agent-hub` | Parity tests pass here; banner there |
