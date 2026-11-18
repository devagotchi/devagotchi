# The path: every feature the operator asked for, in order, each proven on a real device

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-17: "where is my consolidated clear path of features",
  "every chat request from the past week", "you have not proven" anything; the cloud waits
  until the application is built and confirmed.
- PR: not_created
- Branch / baseline: `increment-187/path`, from `main` at `ea50352`
- Objective: one list of every application feature asked for in chat from 2026-11-09 to
  2026-11-17 (104 messages, read in full), what the code on `main` does today, and the one
  step on Texas and the operator's phone that proves it. Work follows this order.
- Non-goals: the cloud edition. No C-milestone, no work in `shippinAI/devagotchi-cloud`
  until every phase below is proven (operator, 2026-11-17).
- Canonical architecture / related plans: every plan in this folder keeps its detail; this
  page decides the order and what "done" means. Where a row names a plan milestone, that
  milestone is the spec.

## What "done" means here

A feature is **done** only when it works on **Texas** (installed from the public release or
image as a new user would) and on the **operator's phone**, and the PR records what was done
and what was seen. Code plus a green CI run is **built**, not done. Status below was read from
the code on `main` at `ea50352` on 2026-11-17 (file and test references in the PR), and updated the same day as every open branch was merged (0.0, 0.1, 0.3, 1.6, 4.1, 5.1, 8.1, 8.3, Phase 9); none of
it has been proven on a device yet.

Status: **built** (code and a test), **partial** (gap named), **absent**.

## What ships first, and what comes after (operator, 2026-11-17)

The operator's words: "the stand alone offering of devagotchi needs to cover just desktop
commander, and the agenthub is limited, the application to be pushed the plugins that all
needs to be truly built and setup in this project so we can finally focus on the
admin/shippin module that would allow us to either deploy a managed instance, or offer a
tailored commercial signup offering free tier and more".

1. **The standalone edition**, free, self-hosted from GitHub. Done when these rows are done:
   - first, the dev build and the conformance run that prove every tool call works, the same
     way every time, and covers everything Desktop Commander offers and more: rows 5.8, 5.9
     (operator, 2026-11-17: "a true dev build that allows me to track our toolcalling ... not
     only does every toolcall work, but it is consistent and reliable and covers everything
     desktop commander offers and more");
   - Desktop Commander, all of it: row 5.1, on this computer with no hub (row 5.4) and through
     a hub to every machine (Phase 2);
   - AgentHub, limited (decision 7 below proposes the line);
   - the plugins for Claude, Codex and Grok, built and set up in this repository: rows 5.2 to 5.5;
   - installed from the release as a stranger would: rows 0.0 to 0.6, 8.3.
2. **Then the admin/Shippin module**, in its own repository: deploy a managed instance for a
   customer, or a tailored commercial sign-up with a free tier and paid tiers
   ([cloud-edition](cloud-edition.md); the tenancy crate, free-edition R4). It starts only
   when step 1 is proven on Texas and the phone.

Rows not named in step 1 (Phases 3, 6, 7, the rest of 4 and 9) are kept and ordered, but they
are not part of the standalone edition unless the operator says so.

## Phase 0: prove what is already built, on Texas and the phone

Nothing new is built until this phase has run once; what it finds is fixed first.

| # | Feature (operator's words, short) | In the code today | Proof |
|---|---|---|---|
| 0.0 | built-in accounts for development: an admin and a plain user, to test, confirm and grow with (operator, 2026-11-17) | built: `devagotchi hub test-accounts` (increment 193): the owner as admin and an ordinary user, each with a password login at `devagotchi.test`, new passwords each run; proven with the binary on a fresh Postgres (admin reaches `/admin`, user gets 404) | `devagotchi hub test-accounts` on Texas prints an admin and a user login; both sign in on the phone, the admin sees `/admin`, the user does not |
| 0.1 | "pull this like any user would": turnkey server, update lane | built: `deploy/compose/install.sh` (Docker, Postgres, Caddy TLS, digest pin, update timer with rollback, nightly backup); image public on GHCR; a re-run keeps tenancy, operators, sign-ins, mail and analytics it is not given (#238); pre-release installs pass `--channel main` until a stable tag ([texas-baseline](texas-baseline.md) M2) | Texas: `install.sh` from the README, nothing else; the hub answers on its domain |
| 0.2 | log in; "a clear UI install on phone" | built: claim link, sign-in, installable phone app with swipe tabs and Go to sheet (Playwright only) | the operator claims the hub and installs the app on the phone |
| 0.3 | connect a machine in one line | partial: `join.sh` and `join.ps1`; Linux needs root; `hub enroll` refuses a machine its config does not name before it prints a code (#226) | Texas joins as a machine; Claude runs a command on it through the hub |
| 0.4 | "I get proper push notifications" | partial: Web Push with VAPID, tested only against a fake push service | an AI asks for an approval; the operator's phone buzzes |
| 0.5 | approve on the phone; standing approvals | built | approve on the phone; the waiting AI continues |
| 0.6 | hosted test hub matches `main` | app.devagotchi.com serves fewer tools than `main` (no `search_files`, `list_processes`, `get_config`, `read_url`, `operation_input`) | update it to `main`; `tools/list` matches |

## Phase 1: sign-in and accounts (free-edition D1, E1; dormancy plan)

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 1.1 | sign in with Google or a passkey after username, display name, email, and an Org from the name when none is given | partial: Google and passkey exist; sign-up asks only email and password; no "<name> Org" | a new person on the phone ends up with name, display name and "<name> Org" |
| 1.2 | Desktop Commander's smooth sign-in; **no passkey on a new account, only after the machine is connected** | partial: "Secure it" with a passkey is shown on every page from the start | new account → machine online → only then a passkey offer |
| 1.3 | password rules configurable in the UI, 8–12 to start, 15–18 shown as recommended, nothing hardcoded | built: `/admin` → Sign-in → Passwords, 8 to 128, 15–18 recommended, in force at once (increment 198) | the owner changes it on a page; sign-up follows it |
| 1.4 | change or reset the password from the panel | built (change does not ask for the current one; reset needs SMTP from config) | change and reset from the phone |
| 1.5 | Google sign-in configured in the admin panel, saved to Postgres | built | the owner pastes a client on Texas's `/admin`; Google sign-in works |
| 1.6 | Shippin.ai sign-in available from deployment, can be toggled off; Google always available; Nomen and other providers in admin | partial: a Shippin card on `/admin` → Sign-in from the first start, its switch on by default, off hides the button and keeps the issuer, the notice beside the button, `install.sh --nomen` (#238); no issuer is built in, so a fresh hub shows no Shippin button until one is set (S0, Nomen accepting Client ID Metadata Documents, is not done) | a Shippin card on `/admin`, on by default, off with one switch |
| 1.7 | the first MCP connection clearly says it made a new account | partial: only with `tenancy.anonymous_connect`, config-only, default off | connect Claude to a fresh hub; it and the person are told "a new account was made" |
| 1.8 | re-auth by a link (push, email, email link) for a machine already known; dormant machines become inactive or are removed after N days, set in the UI | partial: email sign-in link for people; no machine re-auth, no dormancy | [dormancy-and-reauth](dormancy-and-reauth.md) M1–M6 on the phone |

## Phase 2: machines

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 2.1 | connect with no sudo, every OS: Linux x86_64/aarch64, Windows x64/Arm64 | partial: Windows needs no admin; Linux requires root | a Linux user without sudo joins |
| 2.2 | label and manage machines "like Tailscale" | partial: list, add, revoke; no label or rename | rename and label from the phone |
| 2.3 | updates to machines (`machine_update`) | built | Texas updated from the phone |
| 2.4 | the server is Linux by default, the user may choose Windows, all turnkey | Linux built; Windows server absent (decided 2026-11-17: the same Docker stack, no native Windows hub) | `install.sh` on Linux; the compose stack under Docker Desktop on Windows |
| 2.5 | Android | the phone app only; Android as a machine has no plan | operator decides (below) |
| 2.6 | nothing personal hardcoded | `join.ps1` installs to `D:\localdev\devagotchi` when `D:\localdev` exists; "17 clients", the 16-machine limit and the 15-minute code are constants | removed or made settings |

## Phase 3: approvals and notifications

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 3.1 | push and email for approvals, choose which is on, re-send mail if it failed | partial: push only; approvals are never emailed | an approval arrives by email when push is off |
| 3.2 | approve with biometrics, Face ID or PIN per the person's setting | absent: a plain form | Face ID on the phone approves |
| 3.3 | mobile-tailored approval tickets, email backup, SMS via connectors or self-hosted | partial: phone page; no email, no SMS | an SMS through a connector |
| 3.4 | not every ticket needs approval when the person says so | built (standing approvals, approve ahead) | a kind approved ahead skips the phone |
| 3.5 | phone approvals for the scary things: deploys, migrations, push to main, spend over a limit | absent: approvals are advisory; nothing checks commands | a push to main waits for the phone |

## Phase 4: AgentHub

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 4.1 | **scan the project folders and get tickets**: a library of the plans and TODOs AI tools write, preview, then import | built: `devagotchi board scan [PATH] [--import]` (increment 189, #240, agenthub #4): rule files for Kiro, spec-kit, Cursor, Claude, `.ai/` plan tables, plan files, checklists and code markers; preview, import once by fingerprint, on a local board and a hub's Postgres board (proven in CI's `hub (postgres)`). Not yet: closing a ticket when its checkbox is ticked, an MCP `board_scan` tool | `devagotchi board scan` on a real repo on Texas, preview, import, tickets on the phone; a second scan imports nothing twice ([factory](agenthub-factory.md) F5) |
| 4.2 | import from GitHub Issues/Projects, Linear, Jira, CSV, Notion | partial: live GitHub webhook only, no backfill | a repo's open issues imported once |
| 4.3 | assign agents | built (roster worked out, not editable per board) | assign from the phone; the agent is told |
| 4.4 | hierarchy configurable in the UI (Org → Project → Area → Epic → Ticket by default) | absent | switch baselines on a board's settings |
| 4.5 | members, teams, projects and resources managed on the web | partial: orgs, teams, roles, projects; machines not shared per org or team | an org shares a machine with a team |
| 4.6 | hooks: prime, re-prime after compaction, heartbeat, stop gate, write gate | partial: no write gate; Codex plugin ships no hooks | Claude Code and Codex on Texas show each hook firing |
| 4.7 | ClickHouse stats and visuals, signals | partial: events exported; one number on the org page; no charts | a board's charts on the phone |
| 4.8 | AgentHub its own repo and module, its own prefix tables, the hub's Postgres or its own, ClickHouse optional | partial: own repo and `board_` tables; cannot use a separate Postgres; shares schema `devagotchi` | a board on its own Postgres |
| 4.9 | AgentHub's licence | the submodule is still BUSL-1.1 | operator decides (below) |

## Phase 5: Desktop Commander parity and the plugins

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 5.1 | every Desktop Commander capability | partial: tool by tool in [.ai/docs/desktop-commander-parity.md](../../docs/desktop-commander-parity.md) (#238): missing `write_pdf` (waits on the operator), PDF/Excel/DOCX reading and editing, search sessions (`stop_search`, `list_searches`; `search_files` pages by cursor instead), `give_feedback`, `get_tool_result` | DC's documented flows, each run from Claude against Texas |
| 5.2 | the Claude plugin: connector to the person's own hub, skills, hooks | built: `plugins/devagotchi` (hub address set per install, skills `machines` and `agenthub`), `plugins/devagotchi-agenthub` (hooks), held to the hub's tool list by `mcp/tests/plugins.rs` | `/plugin install` in Claude Code, Desktop and claude.ai; it reaches Texas |
| 5.3 | the Codex plugin, as complete as Claude's | partial: a manifest only; its connector file (`plugins/devagotchi/.mcp.json`) names the placeholder `https://hub.example.com/mcp`, the hub address cannot be set, no hooks, and `plugins/README.md` says it uses the hosted hub | `codex plugin add`, signed in, reaches Texas; the hooks fire in a Codex session |
| 5.4 | the plugins work with no hub: Devagotchi on this computer alone, as Desktop Commander does (`devagotchi mcp` over stdio, the local broker) | absent from the plugins: only `devagotchi connect` writes that setup (Claude Code, Codex, Claude Desktop) | install the plugin on a computer with no hub; Claude runs a command there |
| 5.5 | Grok, through the Claude marketplace format | unchecked: "Not yet checked in a real Grok session" (`plugins/README.md`) | install in Grok Build; it reaches Texas |
| 5.6 | a local UI the person opens themselves, not the server | absent | the operator's review |
| 5.7 | API keys with granular control and durations | built | a key limited to one machine and one hour |
| 5.8 | **a dev build that tracks every tool call**: each call, local (`devagotchi mcp`) and through the hub, kept with its tool, machine, target, AI client, duration, outcome, error code and the sizes in and out; a page and a command that show, per tool, calls, failures, and how long they take (median and slowest) | partial: the hub's audit chain keeps who, tool, machine, operation and outcome (`ops/audit.rs`), with no duration, error code, sizes or client; the local adapter keeps the last 1,000 calls in memory only, without durations (`mcp/src/adapter/history.rs`); the broker's log keeps refusals and settlements (`machine-engine/src/trace.rs`) | the conformance run (5.9) on Texas, then the page shows every tool it called, with its numbers |
| 5.9 | **the conformance run**: every tool, and every Desktop Commander capability in [desktop-commander-parity](../../docs/desktop-commander-parity.md), called the way an AI calls it, many times over, locally and through a hub, on Linux and Windows; each answer checked, a call that fails once in a hundred reported as such; and a check that fails when a tool exists with no conformance case, or a Desktop Commander capability has none | partial: `tests/adapter_probe.py` calls 18 tools once each through the local adapter; it misses `search_files`, `read_url`, `get_config`, `set_config_value`, `list_processes`, `kill_process`, `operation_cancel`, the history tools and every hub-only tool, repeats nothing, and is not compared with Desktop Commander's list | the run's report from Texas and from CI, every tool consistent across its repetitions |

## Phase 6: connectors and every setting in the UI

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 6.1 | connectors as nested folders; mail first: custom self-hosted SMTP plus turnkey Resend and the other big providers; the top 10–15 with schemas | partial: 11 services in one flat file; SMTP only for sign-in links; no mail connector | send mail through Resend and through own SMTP from Texas |
| 6.2 | every setting and configuration driven by the UI, grouped and guided | partial: on `/admin` and live since increment 198: tenancy limits and anonymous connect, the shortest password, `machines.auto_update`, `workflows.link_ttl_secs`, `analytics.retention_days`; every other key is listed on the page as set at deploy, with why, and a test fails on a key in neither ([free-edition](free-edition.md) U0). Still to come: SMTP (hub-settings M3), signal sinks (M5), token lifetimes | each one changed from a page on Texas |
| 6.3 | browser-specific UI | partial: a CSS rail at 1024px, untested | the desktop layout reviewed |
| 6.4 | "alpha = pre-release" obvious | built in `.ai`; nothing in the UI | an alpha badge in the hub |

## Phase 7: vibe-coder features, Lovable, the cache

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 7.1 | keys never in chat: a pasted key is caught, with how to rotate it | absent | paste a key; it is caught |
| 7.2 | a checkpoint before every AI task, one tap back | absent | restore from the phone |
| 7.3 | "your AI is stuck" alerts: retry, hand off, stop | absent | a stuck agent alerts the phone |
| 7.4 | Lovable: an MCP for the user's keys and secrets, reach Resend and Vaultix through the hub, import from Lovable | absent | a Lovable app sends mail through Texas |
| 7.5 | optional Dragonfly cache | absent ([ADR 0004](../../../docs/adr/0004-optional-cache-dragonfly.md)) | the hub with and without it |

## Phase 8: the site, the docs, the release

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 8.1 | the landing page as layman interactives drawn from the real product | partial: the hero is an interactive walkthrough instead of the video, with the comparison back (#221, the interim hero); the plan ([landing](landing.md), #223) replaces it scene by scene (S1 to S9, each from the product's own renderers and recorded examples); apps/website says "four tools" | the operator reviews each scene |
| 8.2 | the code makes the platforms obvious: hosted on Linux (likely arm64); Windows, Android, Linux, arm64 supported | partial: no single statement; README:5,7,117 and `apps/website/public/llms.txt` are wrong | one statement, checked by a test against the build targets |
| 8.3 | a release a solo developer installs from GitHub | notes ready: `CHANGELOG.md` Unreleased is the `v0.1.0` release, re-cut by the operator at the release commit ([v0.1-release-readiness](v0.1-release-readiness.md), #238); the old `v0.1.0` tag is from before most of this | a new tag installed on Texas from its release page |

## Phase 9: after the release, before the cloud

| # | Feature | In the code today | Proof |
|---|---|---|---|
| 9.1 | no legacy in the schema: one baseline migration with the final table names in place of 0001 to 0030, migration history tables in schema `devagotchi` with prefixes, stale SQLite comments gone ([texas-baseline](texas-baseline.md) M5; its own plan, since it changes every existing hub's database) | absent | a fresh install on Texas from the one migration |
| 9.2 | AgentHub's standalone board on Postgres too (decision 1 below; [structure-cleanup](structure-cleanup.md) S11) | done, increment 197 (2026-11-17): `board init` makes the board in the Postgres `DEVAGOTCHI_BOARD_STORAGE_URL` names, under a fixed local tenant; the SQLite board, its schema and upgrades are deleted; no importer (pre-release) | `rusqlite` gone from AgentHub's `Cargo.toml`; its tests run on Postgres |
| 9.3 | sign in through Cloudflare Access and Tailscale; Cloudflare Tunnel and Tailscale as machine link routes (D24); Wardenclyffe ([texas-baseline](texas-baseline.md) M6) | absent | Texas reached through each |
| 9.4 | AgentHub as its own server with its own release, protection and the Devagotchi pin (ADR 0005, [texas-baseline](texas-baseline.md) M7) | absent: AgentHub is served by the hub | an AgentHub release installed alone |
| 9.5 | the structure finished: each domain serves its own pages (decision 6), the other crates (S10), a test that every front door has its sections (S12) ([structure-cleanup](structure-cleanup.md), map in [structure-cleanup-map](structure-cleanup-map.md)) | S5 to S7 and S9 merged (#228 to #236, agenthub #1 to #3) | the layout tests, and `web/` left with the shell |
| 9.6 | **central logging via shippin.ai**: every hub's tool calls and events reach Shippin's logging (operator, 2026-11-17) | partial: the hub already streams its audit chain, board and tenancy events to any HTTPS endpoint (`notify/signals`, signal contract v1, `docs/signals.md`); Shippin's endpoint and how it takes them are not decided | a hub's calls seen in Shippin's logging |

Only after Phase 9 is proven: the cloud edition ([cloud-edition](cloud-edition.md)).

## Decisions for the operator

1. **SQLite that remains.** The hub and the link are Postgres only, and so is AgentHub: its
   standalone board moved to Postgres (done, increment 197, 2026-11-17; 9.2). SQLite is left in
   two places:
   - the machine engine's operation journal, on each machine;
   - the kernel's `devagotchi ai` capture journal, on each machine.

   Proposal: keep the two machine-local journals (they are on-machine state, like a phone's
   offline cache). The operator decides.
2. **Android**: the phone app only, or also as a machine the AI works on?
3. **AgentHub's licence**: move it from BUSL-1.1 to FSL-1.1-ALv2 like the main repository?
4. **"Python scans"**: should the scanners be written in Python, or should they scan Python and other
   projects? F5 proposes data-driven rule files, one per format.
5. **Tracking**: these rows as tickets on an AgentHub board (in Postgres) on Texas, each with
   its proof as the check, so progress shows on the phone.
6. **Where a domain's pages live** ([structure-cleanup](structure-cleanup.md), "Ruling
   proposed"): each domain serves its own pages from `<domain>/web/`, and the hub's `web/`
   keeps only the shell (proposed), or `web/` stays and is nested by domain. Step S8 waits on it.
7. **"AgentHub is limited" in the standalone edition**: where is the line? Proposed: the
   standalone edition keeps what one person and their AIs need: boards on the person's own hub
   (and the standalone board), tickets, claims and leases, notes, approvals on the phone, the hooks,
   `board scan`. The commercial module adds what teams and a business need: organisations,
   teams and shared machines (4.5), the configurable hierarchy (4.4), statistics in ClickHouse
   (4.7), importers beyond `board scan` (4.2), AgentHub on its own Postgres (4.8). Nothing is
   removed from the code for this; the commercial build turns it on.
