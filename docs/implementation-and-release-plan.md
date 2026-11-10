# Devagotchi and WardenMesh: implementation and release plan

Updated: **2026-11-10 UTC**. Status: **alpha execution plan; incomplete work is not a product guarantee**.

Lifecycle authority: [DG-LIFECYCLE-1](../.ai/docs/lifecycle.md), with machine state in
[`.ai/project.json`](../.ai/project.json). Production promotion is explicit, never inferred.
AI development/capture execution is tracked in the
[linked task plan](../.ai/dev/plans/ai-development-capture.md); it does not replace this
product plan or turn local session capture into hosted project transcript synchronization.

Delivery decision: finish a complete self-hosted Devagotchi release first, then create and
build the separate private commercial WardenMesh repository. The commercial repository
does not exist yet. Do not create it or copy code into it as an incidental planning action.

This is the execution checklist for the user's issue-lifecycle, collaboration, connector,
desktop-competitor, benchmark and commercial-hosting request. It replaces the speculative
phase ordering in [the initial lifecycle plan](agenthub-issue-lifecycle-and-wardenmesh-plan.md).
It does not override locked decisions in [the architecture master](product-and-platform-plan.md),
[the board contract](agent-board.md), or [the task-graph contract](task-graph-and-mesh-contract.md).
Changes requiring a ruling remain explicit blockers, not silently implemented alternatives.

### Current implementation receipt

The repository AI-development packet implements
[canonical instructions](../AGENTS.md), the requested `.ai` plans/docs/patterns,
an explicit **alpha** lifecycle, thin Claude/Codex/Copilot discovery/hook adapters,
and native `devagotchi ai` operator policy/private capture/import/export controls.
Managed defaults permit validated branch pushes and require merge approval; advanced
capabilities require explicit operator acknowledgment. Capture defaults off, raw content
requires separate informed consent, and native hook metadata is never called a full chat.
See the [packet receipt](../.ai/dev/plans/ai-development-capture.md) for validation,
actual-client acceptance and delivery state. This is not completion of hosted project
transcript sharing or the product's release gates.

The Postgres migration foundation and native live-board cutover are implemented:

- Paired `0014_boards.sql` migrations add normalized tenant-scoped board storage,
  append-only history/receipts, forced Postgres RLS and a durable analytics outbox.
- `devagotchi hub import-board` previews an existing board read-only by default;
  explicit `--apply` imports into an existing tenant on Postgres. Rows, identifiers,
  leases, fences, workflow/settings and event hashes are preserved. Every copied value
  is verified inside the import transaction, including locale-independent text ordering.
- Same-snapshot retries and concurrent imports produce one receipt/outbox effect.
  Changed snapshots are refused. Failed imports, including deferred foreign-key failure,
  leave no partial project. The source remains intact.
- Local receipts: **437 hub-library tests passed, zero skipped**, with a disposable real
  Postgres 18 instance enabled as well as SQLite storage-contract fixtures; CLI debug build
  passed. The compiled command passed preview/apply/reapply, unknown-tenant refusal,
  changed-source refusal, source retention and one-effect checks against that Postgres.
  A permanent binary integration test covers preview without opening destination/state and
  SQLite apply refusal. All **4 binary integration tests passed** after removing the
  shell's inherited `DEVAGOTCHI_BROKER_CONFIG` override for the test process; with that
  override present, the existing auto-start test incorrectly expected a new default config.
  Formatting and patch-whitespace checks passed.

- Native account/org/shared board routing, mutations, graph/settings, approvals, history
  and outbox now use Postgres. Hosted boards hold no SQLite connection and create no
  board files. The running hub refuses SQLite account/board storage.
- Database-time leases and global fencing survive separate instances. Transactional
  notifications wake peer board waiters and invalidate them after reconnect. The reaper
  discovers persisted leases across tenants without opening runtime boards first.
- Actual HTTP MCP/web tests verify the shared authority, cross-tenant authorization and
  explicit storage errors/503s rather than fallback or successful empty pages. Native tests
  also cover atomic project limits, externally authenticated namespaces, no-op waits,
  outbox-failure rollback (including fence/history) and restart lease recovery.
- Latest cutover receipts: **450 hub-library tests passed, zero skipped**, with real
  Postgres enabled, including **9 native backend/runtime tests**. The unified binary built;
  all **5 binary integration tests passed**, including refusal of default/explicit SQLite
  before creating state. Formatting and patch-whitespace checks passed.
  Postgres-backed HTTP fixtures use the same multi-thread runtime as production.
  Cold native opens do not hold the cache mutex across database work, and concurrent binds
  preserve cached waiters and report one actual creator.

Operator procedure and limitations are in [the hub README](../crates/hub/README.md#hosted-boards).
No production board was moved and no CI run or push was triggered. Imported staging rows
are not automatically owner-bound; use fresh boards for this pre-release. The paired SQLite
migrations are storage-contract fixtures, not the chosen hosted-board architecture.
ADRs 0001/0002 are now accepted and the conflicting master/board decision rows are amended;
the architecture-recording blocker is resolved, not deferred to another approval.
Older auxiliary hub SQLite stores and legacy identity mode remain outside this board
packet. ClickHouse deployment/export, offline synchronization, discovery/cleanup UI and
real-client connector testing are still outstanding. These receipts
do not establish R1 readiness.

### Current issue/connector packet

The separate SQLite issue prototype is removed from runtime. Paired migration 0016 binds
immutable repository/issue keys to canonical board tickets and adds append-only delivery
and context receipts with tenant-scoped foreign keys and forced Postgres RLS.
Signed ingress commits source/ticket/notes/history/outbox together before acknowledgment.
It preserves held workflow/fences across source closure, rejects changed-ID replays,
ignores stale ordering and marks equal-time ambiguity for reconciliation.
Shared summaries/advice are append-only canonical notes with stable request receipts,
version checks, bounded validation and live holder/fence checks.

Authenticated REST and MCP (`board_issue_context`, `board_issue_checkpoint`) read/write
that same authority and reject same-name personal-board impersonation. The existing ticket
page shows the source link/state, revision/epoch and honest pending finalization.
Compose accepts configured shared boards, grants, repository mappings and the webhook
secret. `board bind --shared` attaches the existing shared board without creating another.
See the [hub contract](../crates/hub/README.md#issue-context) and
[Compose walkthrough](../deploy/compose/README.md#connect-a-github-issue-repository-to-the-shared-board).

Focused real-Postgres HTTP tests passed for concurrent delivery/context replay, ordering,
closure/reopen, authenticated authorization, stable receipts, MCP/REST/browser parity,
same-name namespace refusal and outbox-failure rollback. The shared CLI attachment was
also exercised against that actual HTTP/Postgres authority from a checkout path with spaces.

Final local packet receipts: **455 hub-library tests passed, zero failed/ignored**, with
real Postgres enabled; **6 actual-binary integration tests passed** on Windows. The binary
hook test uses stdin JSON and paths with spaces, with no POSIX shell, and verifies that
first/recursive Stop and SessionEnd never complete or release held work. Native tests
verify safe session export, bounded/typed input, full held context and lease-time heartbeat
throttling. A real Postgres exclusive lock stalls cold board resolution: the webhook
returns 503 in **under ten seconds**, then retries/replays produce one canonical effect.
Paged context checks retain **30** advice/evidence records across checkpoints.
Formatting, patch whitespace and Compose config validation passed. The Linux installer
environment-rendering path preserved boards/grants/mappings/secrets across re-runs,
explicit `[]` disabling and mode 0600. No push or CI dispatch was used.

The native `board hook` handler replaces shell parsing/suppressed failures. It returns
start/resume board/held-ticket context, throttles heartbeats by authoritative lease time,
blocks first Stop while holding, and reports recursive-stop/session-end interruption
without completing or releasing another session's leases. Client config is printed for
review, not silently installed or trusted. This is not yet a safe merge/backup/uninstall
connector installer, an offline operation queue, or a pinned real-client receipt.

Still open in P02/P03: durable reconciliation/finalization dispatch and recovery/UI,
historical source import/mapping migration, and discovery preview/import/cleanup.
The current webhook uses a bounded synchronous transaction (eight-second processing
timeout), not the planned asynchronous inbox worker. Its acknowledgment is durable,
but backlog handling/redelivery and operational acceptance remain release gates.

### Online evidence checked for this packet (2026-11-09)

| Primary source | Decision grounded in it |
|---|---|
| [GitHub webhook best practices](https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks) and [signature validation](https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries) | Respond within ten seconds; verify original bytes; dedupe reused delivery IDs; explicitly redeliver failed deliveries rather than assuming automatic retry |
| [Claude Code hooks](https://code.claude.com/docs/en/hooks) | SessionStart begins/resumes; Stop ends a response, not an issue; event-specific context and bounded stop feedback |
| [Current Codex hooks](https://developers.openai.com/codex/hooks/) (redirects to official Learn docs) | Native lifecycle hooks now exist; project/exact-definition trust review is mandatory; Stop exit-0 output must be JSON; SessionEnd is advisory. Do not retain notify-only or universal-enforcement claims |
| [MCP tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) | Tools are model-controlled; availability alone cannot guarantee invocation |
| [Postgres NOTIFY](https://www.postgresql.org/docs/current/sql-notify.html) | Commit-only wakeups, not durable payload authority; keep payload to scoped identifiers |
| [ClickHouse Postgres integration](https://clickhouse.com/docs/integrations/postgresql/inserting-data) | Cloud ClickPipes is not a mandatory self-hosting dependency; app outbox/export still needs real duplicate-aware analytical verification |
| [Current upstream AgentHub](https://github.com/shippinAI/agent-hub) | One Postgres/API/MCP/SSE authority, local claim cache, versioned workflow/settings and immutable events; extend existing owners, not another task service |
| [Current Desktop Commander](https://github.com/wonderwhy-er/DesktopCommanderMCP) | Comparator now includes remote MCP, in-client preview/editor, native document formats, paged process output and audit history. Generic file/process parity does not prove superiority |

The older process-level Python end-to-end harnesses still assume a SQLite-default hub.
They must provision isolated Postgres stores and replace the old local-file/shared-board
parity assertion with checks against the bound authority. Their cross-platform PostgreSQL
provisioning is an open gate; these harnesses were not run or reported as passing here.
No new CI job was dispatched to discover that known incompatibility.

## 1. What "done" means

| Milestone | User-visible outcome | Cannot be claimed before |
|---|---|---|
| R1: self-hosted testing candidate | Fresh install, local password access, connected agents, one issue-backed board, start/resume/handoff/close, visible evidence, recovery and upgrade | P00-P06 and the R1 walkthrough pass on the same candidate |
| R2: complete OSS release and measured desktop comparison | Supported desktop execution and coordination work reliably; reproducible comparison and documented feature gaps; install/update/backup receipts | P07-P09 pass, including supported native-host gates |
| R3: WardenMesh private cloud beta | Managed hosting of the tested core, isolated accounts, quotas/billing, backup/restore, cloud onboarding and operations | P10-P12 pass in the private repository |

R1 is not "better than Desktop Commander," and R2 is not a deployed commercial service.
The whole request remains open until R3. No milestone can hide an unfinished required
capability behind a passing unrelated test.

### Product boundary

- **Devagotchi:** open-source self-hosted image, per-user local broker, MCP front door,
  issue-connected AgentHub board and desktop experience. No mandatory cloud account,
  passkey, Nomen, Google, SMTP or model API subscription.
- **Storage in that image:** the hub, its Postgres and (from P06) its ClickHouse ship and
  run together on the operator's own box. "ClickHouse required" means required in the stack,
  never a cloud dependency. Standalone local work and the CLI/plugin's offline cache stay on
  SQLite (§3). Mail is a sender the operator may turn on, never a prerequisite (§3, "Mail").
- **AgentHub:** the existing hub's shared coordination capability, not another daemon or
  second job/task scheduler. Agents retain their existing subscription/CLI sessions.
- **WardenMesh:** the commercial managed-service identity and private operational overlay.
  Devagotchi remains the OSS identity/alias. Keep existing CLI names and storage paths
  compatible; do not globally rename them or silently change the default hosted URL.
- **Execution:** the broker owns operations; the hub owns coordination. No second operation
  engine. Reasoning/provider orchestration already assigned elsewhere by the master stays
  there; an MCP-compatible subscription client does not require a new model API adapter.
- Existing deferred capabilities, including GUI automation and unimplemented native
  platforms, need their own approved packets. Never label a platform fully supported
  merely because its contracts compile.

## 2. Evidence-based starting point

Inspected in this working copy, not inferred from old planning status:

| Surface | Present | Remaining requirement |
|---|---|---|
| Local authentication | Owner-link/claim password bootstrap; optional passkey and configured providers | Full fresh-install/recovery/upgrade walkthrough; preserve mailbox verification distinction |
| Accounts/storage | SQLite/Postgres `Store`, scoped transactions and Postgres RLS | Apply relevant existing ownership/scoping to lifecycle metadata; test both backends |
| Board | Native Postgres hosted CLI/MCP/web resolver and mutations; scoped history/outbox, peer notifications and restart reaping verified; standalone local SQLite retained | Integrate canonical GitHub identity/state and offline sync; legacy staging rows are not automatically adopted |
| Issue lifecycle | Signed `issues` ingress and separate SQLite issue/shared-state tables | Delivery deduplication, ordering, configured repository scope, board integration, authenticated state access, durable dispatch/recovery |
| Shared state | Summary/handoff fields and a JSON advice array | Current writes replace advice; there is no expected-version conflict check; lifecycle updates leave stored snapshots stale |
| Connector | `connect`/`setup` register Claude Code, Codex and Claude Desktop and probe tools | Integrate project lifecycle setup, capability/version checks and real client execution |
| Existing hook pack | Board start/heartbeat/stop POSIX scripts and printed client snippets | Native Windows path, honest errors, issue context, actual installation, client-specific validation, durable stop/handoff behavior |
| Inspector/hub UI | Native activity/workspace/connection views; hub board/ticket pages | Issue-source links, collaboration/evidence/finalization health, actionable connector errors |
| Deployment | Compose (hub, Postgres, ClickHouse, Caddy), install/update/backup scripts, `install.sh --email-from` for mail, the outbox exporter and `/healthz` `analytics` (increment 115), existing probes | The sync endpoints do not exist yet; lifecycle configuration/state survives restart, backup/restore and upgrade; a mail check and `/healthz` `mail`; no cloud prerequisite |
| Benchmark | Proposed workload/budget documents and correctness probes | Executable harness, raw outcomes, pinned comparison artifact and honest reports |
| Commercial service | Planned only | Private repo, license/reuse clearance, infrastructure, billing, operations and launch gates |

Known local receipts from the preceding auth implementation: CLI debug build passed;
hub library suite **429 passed, 0 failed**; Chromium owner/password/passkey test passed.
These do not establish lifecycle route authorization, Docker readiness, Postgres lifecycle
behavior, desktop parity, live subscription integration or cloud readiness.

Working-copy receipt: Git metadata was restored at archive base
`660efb6a46e6213d2c9b23a2dc2b184feec01e17` on
`increment-N/issue-board-integration` without checking out or overwriting working files.
Existing modifications are preserved. No commit or push has been made.
The public home is `devagotchi/devagotchi`. GitHub's license metadata did not identify an
OSS license during this check; verify the actual repository/license before redistribution
or commercial reuse. Do not choose a license on the user's behalf.

## 3. Contract and integration decisions

### Confirmed storage and offline direction (2026-11-09)

The user superseded the SQLite hub-board direction: hub board tables belong in the existing
Postgres storage/access boundary, with ClickHouse required in every hub deployment for
statistics and tracking. SQLite remains only for standalone local work and the CLI/plugin's
durable offline cache and pending-operation queue; it is not a second authoritative copy
of a hub board.

Use client-initiated synchronization, not database-file replication: push queued operations
with stable request IDs, receive acknowledgments, then pull authorized changes using a
server cursor. No inbound client listener or direct client access to either database.
Authentication, current permissions, expected versions and payload hashes are checked on
the server when an operation arrives. Never trust a queued tenant/actor field. Retries
retain their request IDs; changed content under the same ID is refused.

The user confirmed offline scope: discovery, new tickets, notes and drafts may queue;
shared claims and completion require hub confirmation. Cached leases cannot authorize
offline shared mutations. UI/CLI distinguish pending, acknowledged, conflicting and rejected
operations. Reconnect preserves rejected drafts for review and never silently reports them
as saved. Pull caches only authorized project data and handles revocation explicitly.
Independent standalone boards remain visibly local; connecting one requires deliberate import.

Postgres commits authoritative mutations and analytics outbox rows together. A bounded,
retrying exporter feeds ClickHouse with stable event IDs; account/board correctness must
not depend on a distributed transaction with ClickHouse. Export is at-least-once and
analytics queries must account for duplicate deliveries explicitly. No direct unauthenticated
ClickHouse browser access. Required deployment/readiness checks include ClickHouse, while a
runtime analytics outage is an explicit degraded condition: queue safely, expose lag, and
apply a documented backpressure limit rather than silently losing tracking.

Dragonfly is not selected: its Redis-compatible in-memory model does not replace Postgres
relational transactions/access controls or ClickHouse analytical queries. Add caching only
after measurement identifies a concrete need.

Discovery includes POA&Ms, roadmaps, task lists and agent-identified missing items. Provide
preview/selection, source references and repeat-safe import via MCP, CLI and UI. Agent-inferred
gaps are labeled suggestions, not verified facts. Cleanup preserves history and offers
explicit merge/close/archive review, conflicts and confirmation. Extend connector/plugin
surfaces for supported chat apps with capability limits stated; responsive UI uses fluid
`clamp()` sizing, accessible wrapping and tested narrow/wide resolutions.

**Rulings recorded; delivery still pending.** The user authorized direct recording of
[ADR 0001](adr/0001-hosted-board-postgres-and-clickhouse.md) and
[ADR 0002](adr/0002-bound-board-offline-cache-and-queued-drafts.md) while Jesse's ADR
component was unavailable. The master and board contract now amend B01/B02/B11, D23 and
packets 15a/15c accordingly. This removes the decision-recording blocker, not the requirement
for implementation, migration, authorization and recovery receipts. The runtime resolver
must not switch until board semantics pass on Postgres, and disconnected writes must keep
failing honestly until the explicitly pending cache/queue and sync protocol exist.
Dragonfly stays unselected without a separate ruling.

### One coordinated issue, not parallel task stores

Use the existing board for ticket ownership, workflow, notes, decisions, review and handoff.
Persist ingress/configuration/issue mapping in the existing scoped hub `Store` where
appropriate. A mapping resolves an immutable repository ID + issue number to one tenant,
project/board and ticket; repository names/URLs are display metadata.

Migrate hub board storage to Postgres through the existing scoped `Store`, retaining board
workflow, fencing, optimistic concurrency and audit semantics. Issue ingress, source-event
deduplication, board mutations and outbox insertion share a transaction in that database.
Standalone/offline SQLite behavior must not be confused with confirmed shared ownership.
Do not implement another SQLite hub-board ingestion path while the migration is pending.

The legacy `agenthub-issues.sqlite3` data needs an explicit migration/import receipt:
mapping selected by an authorized operator, counts/hashes verified, repeat import safe,
backup retained, rollback documented. Never discard it or infer a tenant from a repo name.
Compatibility routes can remain authenticated projections of the canonical board while
clients migrate; they must not keep a second writable advice/workflow truth.

### Start/resume and end are two different guarantees

1. GitHub `opened`/`reopened` ensures the mapped ticket/context exists and queues lifecycle
   work. It does not grant arbitrary agents permissions or pretend to start a SaaS chat.
2. Client session start/resume reads current issue, board, lease, prior advice, evidence and
   blockers. Work is claimed through the existing fence protocol.
3. Decisions/evidence/handoffs are durable as work progresses, not generated only at shutdown.
4. Agent completion records summary, validation and review readiness. Protected external
   writes and GitHub comments require the configured policy/user approval.
5. GitHub `closed` records external source state and queues finalization. It does not
   automatically certify successful tests or bypass the board's review/active lease.
   Missing evidence becomes visible `pending`/`needs_attention`, never an invented summary.
6. Finalization gathers existing evidence and preserves a closure snapshot. Reopen creates
   a new lifecycle epoch while retaining earlier decisions/closure history.

GitHub issue state and internal board workflow state are distinct. An externally closed
issue with incomplete internal work is visible as such; it must not force-close a held ticket.
`Stop` is a client turn event, not proof that the GitHub issue ended.

### Shared-state rules

- Identity/tenant/project authorization is derived from the authenticated subject and
  board grants; agent/model labels in payloads are provenance, not authorization.
- Reads return a canonical current projection plus revision, source status, workflow,
  active lease/holder, decisions, evidence, handoff and finalization status.
- Mutable summaries carry `expected_version`; stale writes return a stable conflict and
  current revision, without overwriting a newer update.
- Decisions/evidence are append-only board records with stable IDs/request keys. Retrying
  a key with changed content is a conflict. Reads are paged; retention is explicit and
  never silently drops the last 25 recommendations required by the benchmark.
- Validate confidence ranges, identifiers, payload sizes and evidence shapes. Keep secrets
  out of payloads, logs, config snippets and routine benchmark output.
- Source delivery revision and shared-board revision are separately named. Timestamps alone
  are not a concurrency token. Delayed source events cannot roll the issue back; equal or
  ambiguous source updates require reconciliation rather than guessed ordering.

### Mail: one sender, configured by the operator, never required

Mail is already one owner in the hub and stays one; nothing here adds a crate, a process or a
second sender. The boundaries it sits inside:

- **Only the hub speaks to the network** (CLAUDE.md). The SMTP client is
  `crates/hub/src/mail.rs`; the one sender is `web/email.rs` (`Mailer`). `core`, `mcp`, the
  broker and the link never mail, and `deny.toml` keeps it so.
- **It is optional** (master decision 8; product boundary, §1). A hub starts, is claimed,
  signs in with a local email and password and recovers through a replacement owner link with
  no mail server. R1 proves that first (step 1); mail is turned on afterwards (step 2).
- **Secrets stay out of the URL and the config file** (security baseline §5 and row 23). The
  SMTP URL names user, host and port only and is refused with a password in it; the password
  comes from the environment (`password_env`, set from the secret store) or a 0600 file
  (`password_file`). TLS from the first byte (`smtps://…:465`) or STARTTLS (`smtp://…:587`);
  plain SMTP only to this machine's loopback, for tests. Nothing logs the password or a body.
- **Two account systems, one sender.** `auth.mode hub` sends through `auth.email`.
  `auth.mode devagotchi` has no sender: its magic link is `local.deliver` `off` or `log` (the
  journal and `{state}/login-outbox/`). [auth.md](auth.md) says so; "no SMTP client in the hub
  today" in [plan/hub-auth-and-identity.md](plan/hub-auth-and-identity.md) is that plan's
  history, not current status.

Where an operator configures it, from the outside in. Each is the place its neighbours
(providers, operators, analytics) are already configured; none is new:

| Surface | What is set | State |
|---|---|---|
| `deploy/compose/install.sh --email-from "Name <address>" --env-file FILE` (same flags in `deploy/proxmox/hub-up.sh`) | Writes `DEVAGOTCHI_HUB_EMAIL` into `.env` (0600) and keeps `DEVAGOTCHI_SMTP_URL` / `DEVAGOTCHI_SMTP_PASSWORD` from the env file; a re-run keeps values it is not given | built |
| `.env` (`deploy/compose/.env.example`) | `DEVAGOTCHI_HUB_EMAIL` as `auth.email` JSON, `DEVAGOTCHI_SMTP_URL`, `DEVAGOTCHI_SMTP_PASSWORD` | built |
| `hub.json` `auth.email` (`crates/hub/README.md`) | `smtp_url` or `smtp_url_env`; `password_env` or `password_file`; `from`; `per_address_per_hour` (1 to 100, default 5) | built; parsed at start, and a named variable that is unset stops the hub with its name |
| The site, `content/messages.json` `email` and `access.admitted_email` | Subject and body of the two emails, with `{product}`, `{link}`, `{account}` | built |
| `/account` | A person links a verified address to their account (`POST /auth/email` with `link=1`); which channels notify them, in what order, is a per-user setting (B10) | linking built; channel settings not built |

What goes out today, in `auth.mode hub`: the sign-in link (`POST /auth/email`; five per
address an hour, twenty per client address) and the "you're in" admission email
(`announce_admissions`, in the background, to the tenant's first verified address). Ruled and
not built: approvals and notices by email (master 10c, B10) and the email leg of recovery
(master decision 7). Both reuse the same `Mailer`; a second sender fails the boundary test.

What an operator could not do until increment 113 is learn that their mail works before a
person needs it. The first, second and fourth items below landed there (CHANGELOG, "Mail
check and health"); the third stays with P05/P06:

- **A connection check and a test send** (built). `auth.email` was parsed at start, never
  connected. `devagotchi hub check-mail --config … [--to ADDRESS]` sits beside `check-site`:
  it connects, negotiates TLS and authenticates; with `--to` it sends one email that says the
  hub's mail works, plainly a check and not a sign-in link, and prints the server's reply
  code, never the password. `install.sh` runs it after the stack is up when `--email-from`
  was given, says what failed, and does not stop the hub over mail.
- **Readiness that is reported, not counted** (built). A `mail` entry in `/healthz`:
  `configured`, `tls`, the per-address limit and the last send's outcome and time, with a
  failure reduced to the reply code or the step so no address reaches a public page. Like the
  issuer, a mail outage never marks the hub down; password, passkey and providers keep working.
- **Failures the operator sees.** A sign-in or admission email that does not go out is one
  line on stderr and the health entry. It becomes a row on `/operator`, beside the sign-ins,
  with the SMTP reply code and the time, never the body or the address list (P05 for the
  page, P06 for the row).
- **The walkthrough** (built). `deploy/compose/README.md`, "Mail: sign-in links and notices":
  install without mail, claim, sign in; then the env file, `--email-from`, the installer's
  check, `check-mail --to` your own address, read the mail; what `smtps://` and `smtp://`
  mean, the two password forms, and the Google Workspace, Fastmail and Postfix URLs. P06's
  fresh-stack acceptance stays "without SMTP"; mail is the second walkthrough on the same
  candidate.
- **Hosted (R3).** WardenMesh sets the same `auth.email` from its own secret store (P11).
  Nothing provider-specific enters the OSS hub.

### Client support is a measured matrix

| Client class | Delivery path | Required receipt |
|---|---|---|
| Claude Code | Existing MCP connector + verified native lifecycle hooks | Start/resume context, heartbeat, failure, stop/handoff tested on a pinned client |
| Codex CLI | Existing MCP connector + verified current hook schemas/features | Same cases using Codex's actual accepted schema, not assumed Claude compatibility |
| Claude Desktop | Existing stdio MCP connector | Real discovery and board calls; explicit workflow wrapper if no reliable lifecycle interception |
| Other MCP clients, including hosted subscriptions | Authorized HTTP MCP or documented compatible bridge | Real supported client/plan receipt, capability limitations and manual/wrapper workflow labeled |
| REST/CLI automation | Existing authenticated board protocol | Same fences/revisions/idempotency/errors; restart/reconnect tested |
| Local-model host | Its MCP/CLI integration where supported | One real host receipt; no fabricated API adapter or model quality result |

A subscription UI without hooks cannot be forced by MCP to make a particular tool call.
For managed execution, enforce start/close acknowledgments at the workflow boundary.
For external clients, report `hook-enforced`, `workflow-enforced`, or `manual/unverified`.
Never advertise "all AIs automatically comply." Plan restrictions and approvals remain
visible; use the supported clients' normal authorization, not subscription credential scraping.

## 4. Ordered implementation packets

Each packet ends with a reviewable implementation, targeted receipt and updated status.
Dependencies are real prerequisites, not simply row order. Packet acceptance checks listed
here are tests to implement/run, not existing passing evidence.

| ID | Work and primary owner | Dependencies | Acceptance / smallest useful validation |
|---|---|---|---|
| P00 | Establish approved Git checkout; preserve archive changes; inventory current diff; confirm release/license and CI required-check baseline | None | Existing work preserved; remote/branch/base recorded; no accidental push or source loss |
| P01 | Freeze issue-to-board mapping, projection, conflict/idempotency/error contracts; config types and paired Store migrations; migration plan | P00 | Schema/contract tests; unauthorized/malformed mapping rejected; migration fixtures cover existing data |
| P02 | Harden ingress in `issue_lifecycle.rs`: configured repo mappings/secrets; verify raw-body HMAC using library verification; mandatory delivery ID/type/action checks; bounded durable inbox; correct 4xx/5xx; stale-event policy | P01 | Valid open/close/reopen, wrong signature, missing headers, unsupported action, disallowed repo, duplicate/redelivery and failed commit tests |
| P03 | Apply inbox to canonical board; board-side unique source keys; resume/closure epochs; append-only advice/evidence; version conflicts; authorized compatibility routes/MCP projection; import legacy data | P02 | One ticket per source issue; crash/retry produces one effect; concurrent writer conflict; 25+ decisions retained; current close state; cross-account/project denial and migration receipts |
| P04 | Extend `connect`/`setup` and board hook command path; native Rust hook handler/thin OS launchers; version/capability detection; safe merge/backup/uninstall; one start/beat/handoff contract | P03 | Client config fixtures with unrelated settings; repeated install no duplication; paths with spaces; Windows without POSIX shell; real Claude/Codex start/stop; unavailable hub reported, not ignored |
| P05 | Extend existing hub board/ticket/connect/operator UI and inspector links; show issue source, active agents, advice/evidence, conflicts, pending finalization and recovery | P03; P04 for connector acceptance | Browser walkthrough with keyboard/mobile access; escaped issue content; permission-denied/conflict/offline states actionable; second agent sees first agent's evidence |
| P06 | Complete self-hosted onboarding/deployment: local credentials, optional provider and mail setup (`check-mail`, `/healthz` `mail`, the mail walkthrough; §3 "Mail"), GitHub webhook guide, mapping UI/CLI, restart/upgrade/backup/restore | P03, P04, P05 | Fresh Compose walkthrough without SMTP/OIDC; then mail on with `--email-from`: `check-mail --to` delivers, an unset `DEVAGOTCHI_SMTP_URL` stops the start naming the variable and no log line carries the password; persisted issue/advice after restart and restore; replacement-owner recovery; existing install/update/rollback checks preserved |
| P07 | Extend existing probes with lifecycle/continuity benchmark and JSON report; separate correctness suite, performance profile and comparator adapter | P03; P04 for real-client cases; P06 for Docker cohort | Rerunnable deterministic fixtures; denominator includes failures; thresholds measured directly; comparator absent means `not_run`, never a Devagotchi win |
| P08 | Close desktop/workstation gaps under existing G0-G5 packets: tool inventory, durable operation reconnect, terminal/file/search parity, native diagnostics, reviewable safe workspace, update authenticity and inspector behavior | P00; P04/P05 for integrated UX | W01-W15 receipts for applicable capabilities; real native-host tests; scope-escape/cancel/reconnect cases; every parity tool explicitly measured/pending/not supported |
| P09 | OSS release candidate: documentation/client matrix, license/distribution clearance, compatibility/migration checks, R1/R2 acceptance report and one complete final validation | P06, P07, P08 | Same tested source/build and image digests; supported-platform matrix, full required CI once ready, unresolved blockers prevent release |
| P10 | Create approved private commercial repository from accepted OSS baseline; preserve history/notices; thin branding/config overlay; upstream-sync and shared contract compatibility | P09; operator confirms private repo owner/name and license | Private visibility verified; no credentials/hosted deployment code added to OSS; both builds pass the same core contract fixtures |
| P11 | Private SaaS: hosted auth with configured providers/email delivery, tenant authorization/quotas, billing webhooks, metering, managed connector setup, backups, migrations, monitoring and rate limits | P10 | Tenant escape tests, duplicate payment handling, quota boundaries, revocation, billing state reconciliation, restore and degraded-provider receipts |
| P12 | Private production beta: domain/TLS/DNS, release/update signing, secret management, staged deployment/rollback, operational runbooks, retention/export/deletion, incident alerts and real onboarding | P11 | New paying/free account walkthrough, restore drill, failed deploy rollback, load limits, consent and operator acceptance; user explicitly approves launch |

Storage/sync amendment to the packets above:

- P01 first migrates hub board tables and board resolver to scoped Postgres, with explicit
  backup/import receipts for existing SQLite boards and preserved lease/event semantics.
- P02-P03 implement transactional Postgres issue ingestion and canonical shared state,
  plus the durable analytics outbox and authenticated cursor synchronization endpoints.
- P04 adds client-initiated SQLite cache/outbox sync to CLI/plugin/connector surfaces.
  Test dropped acknowledgments, restart, token expiry/revocation, stale versions and
  offline refusals for claims/completion; no success-shaped fallback to a local board.
- P05 adds discovery preview/import and cleanup UI, including pending sync/conflict
  presentation and fluid clamped layouts across mobile, tablet and wide desktop sizes.
- P06 provisions required ClickHouse with persistent volumes, restricted credentials,
  exporter/migrations, health/lag diagnostics and backup/restore coverage for both databases.
  It also owns the mail check, `/healthz` `mail` and the mail walkthrough (§3, "Mail").
- P07 includes duplicate-safe analytical counts, backlog recovery and offline/reconnect
  correctness, not merely online latency.

AgentHub product sequence (agreed with the user 2026-11-10; each ends on a CI gate and
the next does not start on a red one):

| Increment | Work | Gate |
|---|---|---|
| 114 | Organisations, members, teams, roles (owner, admin, member, agent, viewer) and project access in Postgres; one server-side resolver for MCP, web, issue routes and the API; invites | Concurrent owner changes and invite takes on Postgres; `tests/orgs_probe.py` in e2e: a demoted member's AI is refused on its next call on the same session |
| 115 | ClickHouse in the compose stack; outbox exporter with stable event ids and replacing-merge deduplication; `/healthz` analytics with pending count and oldest lag; bounded backpressure; backup; first per-project queries | R1 walkthrough step 9 (stop ClickHouse, hub serves and reports degraded, restart drains, each event counted once) and the compose job |
| 116 | The board on the web as the primary surface: projects, board, ticket with evidence and history, workflow/settings, members and organisation pages, server-rendered on the same resolver | A browser probe per page in the Chrome job |
| 117 | One versioned signal contract (board events, audit, operations, approvals, auth, mail), exportable from ClickHouse and over a push endpoint; external consumers such as rrflow attach to it as sinks, never as a dependency | Contract tests; a sink that is down never blocks the hub and its lag is reported. Shipped as `devagotchi.signal/v1` (docs/signals.md) with three sources: board events (approvals included), tenant events, and the audit chain of tool calls. Not yet sources: broker operations beyond the tool call that started them, and mail sends |

P08 is not permission to implement every deferred feature speculatively. Begin with an
inventory of the declared comparison surface, reuse existing mechanisms, and route missing
locked/decision-dependent capabilities through the master register. Existing task-graph,
sandbox, terminal and signed-update packets cannot be bypassed by shell-command shortcuts.

### P02-P04 implementation details that prevent rework

- Acknowledge webhook success only after durable receipt. Process slower board/application
  work in the hub's existing process. Bound queue/retry growth and expose terminal failures.
- Keep ingress GitHub authentication separate from account authorization. A webhook cannot
  assign itself to another tenant by choosing payload fields.
- Never hold a synchronous SQLite mutex across async work or run unbounded writes on the
  HTTP executor. Follow existing board/store blocking and transaction patterns.
- Replay handling preserves delivery identity and payload hash. A duplicate is a no-op;
  the same ID with different payload is rejected. No `INSERT OR REPLACE` replay shortcut.
- Authenticate read/write projections with the same grants as `board_*`; cookie mutations
  require the existing same-origin protections. No unauthenticated issue/state API.
- Hook failures remain explicit. Do not retain current `|| true`/stderr suppression as
  "successful bootstrap." Distinguish no board configured from configured-but-unreachable.
- A recursive stop hook must not loop indefinitely. Commit a valid handoff/release or show
  an actionable unresolved failure; if the process dies, lease expiry records interrupted
  work honestly. Do not mark an issue finished because a chat ended.
- Generated client config changes are limited to owned keys; preserve unrelated servers,
  back up atomically, never place tokens in argv/config text, and test removal/rollback.

## 5. R1 walkthrough: what the user will confirm

One disposable repository/issue, a fresh self-hosted install and two independently
authenticated agent seats:

1. Start the documented image/stack. Create local owner credentials without passkeys,
   Nomen, Google or SMTP. Log out/in; recover through a replacement owner link. `/healthz`
   reports Postgres and ClickHouse ready and `mail` not configured.
2. Turn mail on: `install.sh --email-from` with the env file, then `hub check-mail --to`
   your own address; the sign-in email arrives and `/healthz` `mail` shows the send. Unset
   the SMTP URL: the start stops naming the variable, with no password in any log. Put it
   back. A candidate run without a mail server skips this step and its receipt says so.
3. Configure one authorized repository mapping and GitHub secret without exposing it.
   A signed open event produces exactly one visible ticket and initial context.
4. Run project connector setup. Verify the actual configured client discovers and calls
   the hub tools, not only that a config file exists.
5. Agent A starts/resumes, receives current context, claims the ticket, executes a disposable
   broker operation, and records advice plus evidence.
6. End A's session with a durable handoff. Agent B reads the same evidence, claims with a
   new fence, resumes and records an independently identified review.
7. Disconnect/restart the client and hub. State remains readable; no duplicate execution
   or duplicate issue ticket; stale A writes are refused.
8. Cut A's client off from the hub. `board prime`, `ls` and `show` answer from the cached
   copy with its banner; a claim or a completion is refused with why. Whether a note or a
   draft queues and is acknowledged on reconnect, or is refused, follows the offline-writes
   ruling of §3; the receipt names which rule the candidate ran under.
9. Stop ClickHouse. The hub keeps serving and `/healthz` reports analytics degraded with the
   outbox lag; start it again and the backlog drains, each event ID counted once.
10. Close the source issue. Source state changes immediately after durable processing;
    closure evidence/finalization status is truthful. Reopen retains earlier history.
11. Redeliver events and simulate one dropped response. Exactly one effect is retained.
12. An unrelated account/project cannot read/write the ticket or claim another tenant's
    repository. UI and MCP return clear denials.
13. Restore a backup of both databases into a fresh instance, verify record counts, the
    chain, the mapping and the analytics totals, then test the documented upgrade/rollback path.

Receipt: candidate source identity, binary/image digest, configuration without secrets,
OS/client versions, steps/results and remaining limitations. Failure at any required step
means R1 is not ready. Live GitHub/client approvals require the operator; fixtures prove
protocol correctness but do not substitute for those live receipts.

## 6. Validation without wasting CI

Keep the full pre-release gate; stop running it repeatedly during unfinished packets.
There have been no CI dispatches or pushes from this source archive in this planning pass.

### Local cadence

- **Documentation-only:** review links/status/contracts. No Cargo rebuild or full suite.
- **Contract/migration packet:** focused schema, migration and owner tests, both store
  backends (Postgres and SQLite) when storage semantics change. Regenerate contract artifacts only when contracts change.
- **Lifecycle/board packet:** one combined targeted runner selection covering the changed
  owner, then the lifecycle probe. Expand only for affected integration boundaries/failures.
- **Connector/hook packet:** config/unit fixtures first; build CLI once after coherent
  Rust changes; run the relevant real-client/OS probe against that build.
- **UI/auth packet:** relevant Rust routes plus one applicable browser suite. Reuse the
  built candidate unless compiled code/assets changed.
- **Deploy packet:** targeted script/static checks and disposable Compose integration with
  Postgres and ClickHouse, the mail check against the stub SMTP server; don't rebuild an
  identical image for every doc edit.
- **Milestone freeze:** build default-member binaries, full required tests/probes and
  supported OS/backend/deployment gates on one recorded revision. A later source change
  invalidates affected receipts; don't reuse them as if the new build were tested.

Commands already owned by the repository remain the baseline: `cargo fmt --all -- --check`,
`cargo build --locked --bins`, `cargo nextest run --locked`, existing e2e/MCP/boundary probes,
contract generation checks and the appropriate browser/deploy harnesses. Check availability
before selecting nextest; missing tooling is a setup issue, not a skipped test.

### CI work to implement in the proper checkout

Current CI rebuilds binaries in core, e2e and installer jobs across several native hosts;
browser coverage already runs on one Linux host. Concurrency already cancels superseded
runs. Do not remove these correctness gates just to make the dashboard green.

1. Preserve required-check names and branch-protection semantics. Record them before editing.
2. Add pinned, trust-scoped Rust dependency/build caches keyed by OS, architecture, toolchain,
   lockfile and profile. Never share untrusted executable artifacts into a trusted release.
3. Reuse identical same-revision native binaries where jobs genuinely share toolchain/profile;
   retain separate ci-a/ci-b update builds and native compilation where semantics differ.
4. Separate docs-only checks from compiled/runtime work using explicit change classification
   and an always-reporting aggregate gate. Test renamed/deleted files, forks, merge commits
   and unknown changes; unknown changes select full coverage.
5. Keep full native/storage/deployment/release coverage at the milestone and release gates.
   Manual live-provider/comparison/soak work stays opt-in, not every edit.
6. Push coherent locally validated packets, not every micro-edit. Preserve work through local
   commits/checkpoints in the proper checkout without dispatching experimental workflow runs.
7. Measure before/after runner-minutes and wall time on equivalent change classes. No claimed
   CI saving percentage until measured. Do not rerun a green unchanged revision without a
   documented flaky-test/environment reason.

CI workflow changes are a distinct validated packet after P00, not an unreviewed deletion
of the current platform matrix. Full gates are preserved even when their cadence improves.

## 7. Benchmark and honest competitor comparison

The executable specification is [the benchmark plan](agenthub-benchmark-plan.md), using
the existing [W01-W15/B01-B14 rules](performance-and-experience-spec.md).

- Correctness fixtures: open/reopen/close, duplicate/out-of-order delivery, multi-agent
  handoff, concurrency, 25+ advice entries, reconnect, interruption, isolation and restore.
- Performance fixtures: cold/warm startup, durable source-to-readable-state latency,
  repeated authenticated reads/writes, handoff time, mixed issues/output/control load.
- Desktop parity: compare overlapping command/file/search/terminal/cancel/recovery/inspector
  semantics. Missing issue orchestration in a comparator is a feature difference, not an
  artificial infinite-latency baseline.
- AI-assisted cohort: real client/model/plan and prompts recorded; measure recovered facts,
  evidence correctness, tokens/cost where available and successful handoff outcomes. Two
  scripted subjects prove protocol continuity, not model intelligence or subscription support.
- Compare on matched hosts with pinned builds. Current DesktopCommanderMCP `main` reference
  checked here: `d05721a09d86d35bb86a2b1c07c87139b5d9b6a6`, package `0.3.2-alpha.1`;
  that is an alpha source reference, not a claimed stable installed baseline. Pin a stable
  released artifact separately for public claims.
- Use release builds for performance, retain all attempts/failures, label conditional-success
  percentiles, preserve raw observations. At least 1000 successful observations for p99;
  small startup cohorts do not support a reliable startup p99.

## 8. Research and limits

Checked 2026-11-09 UTC:

- [GitHub webhook best practices](https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks):
  validate secrets/event/action, respond within 10 seconds, retain delivery IDs; redelivery
  reuses the ID. Availability/reconciliation is required; do not assume every event arrives.
- [Claude Code hooks reference](https://code.claude.com/docs/en/hooks):
  start/resume, per-turn Stop, tool lifecycle and session end are different events.
- [Official Codex hook schemas](https://github.com/openai/codex/tree/main/codex-rs/hooks/schema/generated):
  current source has dedicated event input/output schemas. Pin the installed release and
  prove feature/config compatibility before generating production hooks.
- [MCP tools contract](https://modelcontextprotocol.io/specification/2025-06-18/server/tools):
  tools are model-controlled; MCP connectivity alone is not a guaranteed start/stop callback.
  This is the protocol revision exercised by the current board probe, not a claim of newest revision.
- [AgentHub reference](https://github.com/shippinAI/agent-hub/tree/ca626db10456b8597704032e80944b359b892312):
  its existing connect/protocol design uses agent identity, leases/fences, request keys,
  notes/handoffs and a shared board. Repo default branch is `master`; this is the frozen
  reference, not a newly evolving separate service to deploy alongside Devagotchi.
- [DesktopCommanderMCP source reference](https://github.com/wonderwhy-er/DesktopCommanderMCP/tree/d05721a09d86d35bb86a2b1c07c87139b5d9b6a6)
  and [existing primary-user evidence](desktop-commander-user-evidence.md):
  historical reports are scenario inputs, not proof of current unresolved failures.

Operator-dependent blockers: proper Git checkout/source identity, release/license decision,
real GitHub webhook and subscription-client approvals, supported-host test access, and later
private repo/domain/cloud/payment credentials. Never ask for credentials in chat or fabricate
their availability.

## 9. Next action and progress rule

Start P00, then P01-P03 as one coherent integration sequence. Do not start another round of
auth redesign, UI repainting, generic provider abstraction or cloud billing while canonical
issue ownership/access/continuity is incomplete.

After each packet, update its receipt/status here or in the existing board and report only:
what changed, what exact acceptance passed, what remains blocked, and the next ready packet.
Do not create competing planning documents or call the entire product implemented because
one subsystem passes. The user confirms R1 by the walkthrough, R2 by release/comparison
receipts, and R3 by the private cloud launch gate.
