# The free edition: one developer's hub on their VM, hypermodular, Postgres, Desktop Commander parity first

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha, pre-release:
  no data kept, no compatibility, no legacy paths)
- Issue / request: operator, 2026-11-14, in his words:
  - "THE FREE VERSION IS DESIGNED FOR ONE DEV TO DEPLOY ON THEIR VM, HAVE POSTGRES ACTUALLY MAKE
    SENSE AND HAVE AT MINIMAL EVERY CAPABILITY AND PROCESS FROM DESKTOP COMMANDER FOR
    ADOPTABILITY. IT THEN EXPANDS: OFFERS PHONE, AGENTHUB, MORE."
  - "the multi-tenant can stay but it cannot be in core, it needs to be properly nested. core is
    not even supposed to exist ... hypermodular self describing folder and files for true scale
    adopting strict patterns based on their stack"
  - "AGENTHUB IS NOT ITS OWN REPO SUBMODULE PROPERLY PULLED IN TO CREATE ITS OWN CLEAR PREFIX
    TABLES"; "THE DESKTOP COMMANDER LOGIN IS STILL NOT IN THIS CODEBASE"
  - "passkeys should not be on a new account, it should be after they have established the
    connection"; "no compatibility, no legacy"
- Supersedes: `postgres-only-hub.md` and `passkeys-after-connect.md` (never merged), the
  2026-11-11 ruling in [cloud-edition](cloud-edition.md) that kept `tenancy.mode multi` in the
  core, and `docs/product-and-platform-plan.md:424` ("SQLite for a standalone one").
- Keeps: ADR 0001 (Postgres of record, ClickHouse) and ADR 0002 (SQLite only as a client's
  offline cache and pending queue); [spec-first-connect](../../../docs/spec-first-connect.md);
  [desktop-commander-teardown](../../../docs/desktop-commander-teardown.md) as the parity
  reference.
- PR: one per milestone, in the order below
- Branch / baseline: from `5c28f8c` (v0.1.0)

## What is true today (each checked on `5c28f8c` or the live hub, 2026-11-14)

| Fact | Evidence |
|---|---|
| `crates/hub` is the dumping ground: 188 files, 95,944 of about 121,000 Rust lines | `find crates/hub -name '*.rs'`, `wc -l` |
| Its `src/` has 52 loose top-level files and 17 folders, with overlaps: `account/` and `accounts/`, `analytics/` and `warehouse/`, `agenthub/` inside the hub beside `crates/agenthub`, `identity.rs` (2,093 lines, the old account system) beside `accounts/` | `ls crates/hub/src` |
| `crates/core` is the machine-side engine (run programs, files, job journal), named "core" | `ls crates/core/src` |
| AgentHub is a crate in this repo, not its own repository; its `board_*` tables are in the hub's store migrations (0014, 0016) | `crates/agenthub`, `crates/store/migrations/postgres` |
| Boards and accounts run on Postgres in `hub serve` | `lib.rs:1047` `using_postgres`; live `/healthz` `"store":{"tenant_rls":true}` |
| Still SQLite in a running hub: machines, join codes, push subscriptions, audit, browser sessions, Nomen sign-ins; plus the old `auth.mode devagotchi` (all SQLite), the store's SQLite backend and twin migrations, and the hub's SQLite board fallback | `lib.rs:524`, `:891`; `web/sessions.rs:26`; `auth.rs:203`; `identity.rs`, `oauth.rs`; `store/src/lib.rs:180-200`; `agenthub/mcp.rs:375`, `:506` |
| Multi-tenancy is spread through the hub (accounts, oauth, web, store RLS, `/operator`, limits) | `tenancy.mode multi` in `config/tenancy.rs`, `tenant_*.rs`, `web/operator.rs` |
| Desktop Commander sign-in parity: email + password works live; Google is built but not configured (no button live); Forgot password says "not available yet" (no mail configured live) | `curl https://app.devagotchi.com/auth/login`; `/healthz` `"mail":{"configured":false}` |
| Desktop Commander machine connect: `curl … \| sh` **without sudo**, the machine shows a code, the person approves it in the browser they are signed in with (device grant), session saved. Ours: `curl <hub>/join/<code> \| sudo sh` with a one-time code, no browser approval | teardown §8.1, §3.3; `src/join/join.sh` |
| Desktop Commander tools we do not have: search sessions (`start_search`, `get_more_search_results`, `stop_search`, `list_searches`), interactive processes (`interact_with_process`, `list_sessions`, `read_process_output` by offset), `list_processes`, `kill_process`, `edit_block` by range and fuzzy, `write_pdf`, `read_file` of PDF/Excel/DOCX/URL, `get_config`/`set_config_value`, `get_recent_tool_calls` | teardown §1.3; the Devagotchi tools this session sees from app.devagotchi.com |
| Passkeys come before a first connection: landing copy (21 mentions in `landing.json`) and the self-hosted claim ("make my passkey") | `crates/hub/site/default/content/`; `deploy/compose/README.md` |

## The shape: modules that describe themselves

No crate or folder called `core`, `common`, `utils` or `shared`. Every module is a folder with
the same parts, so a person or an agent reads one `README.md` and knows what it owns:

```
modules/<name>/
  README.md        what it is; the tables it owns (prefix); the routes, MCP tools and pages it
                   serves; what it depends on; its settings and where in the UI they are
  Cargo.toml       one crate, named devagotchi-<name>
  migrations/      Postgres only, every table named <prefix>_*, in schema devagotchi
  src/             lib.rs re-exports its surface; one file per concern, none over ~800 lines
  ui/              its pages and copy, when it has any
  tests/
```

| Module | Owns | Table prefix | From today |
|---|---|---|---|
| `machine-engine` | running programs and sessions, files, search, the job journal (on the machine; SQLite there is the machine's own, ADR 0002) | none (machine-local) | `crates/core`, `crates/broker` |
| `machine-link` | the machine's outbound link and local IPC | none | `crates/link`, `crates/ipc` |
| `machines` | the registry, joining (device grant), updates, provenance later | `machine_` | `devices.rs`, `join*`, `link.rs` hub side, `machine_update.rs`, `tenant_machines.rs` |
| `identity` | accounts, email + password, Google/GitHub/OIDC/Nomen sign-in, browser sessions, passkeys (offered after the first machine), the OAuth server AI apps sign in with, the device page | `auth_`, `oauth_` | `accounts/`, `account/`, `oauth.rs`, `web/sign_in.rs`, `web/passkeys.rs`, `web/sessions.rs`, `nomen*` |
| `audit` | the hash-chained record of every call | `audit_` | `audit.rs` |
| `push` | Web Push to phones and browsers | `push_` | `push.rs`, `web/notify.rs` |
| `mail` | sending mail: SMTP or a connection (Resend) | `mail_` | `mail.rs`, `web/email.rs` |
| `connections` | outside services through the hub | `connection_` | `connections.rs`, `accounts/connections.rs` |
| `analytics` | the ClickHouse export | `analytics_` | `analytics*`, `warehouse*`, `signals*` |
| `site` | public pages, copy, assets, docs, `llms.txt` | none | `site/`, `web/landing*`, `web/public.rs`, `web/discovery.rs` |
| `mcp-surface` | the MCP tools, grouped by domain (Machines, Files, Processes, Search, AgentHub, Connections, Setup) | none | `hub.rs`, `output_schema.rs`, tool routers |
| `tenancy` | many accounts and tenants on one hub, `/operator`, limits; **not in the default build**, wired only when a hub enables it | `tenancy_` | `tenant_*.rs`, `web/operator.rs`, `web/hosted.rs`, RLS policies |
| `hub-server` | starts the hub: config, HTTP, wiring the modules; nothing else | none | `lib.rs`, `config*`, `web/routes.rs` |
| `agenthub` (git submodule `devagotchi/agenthub`) | boards, tickets, notes, approvals, workflows, wake; usable alone on any Postgres (and ClickHouse) | `board_` | `crates/agenthub`, `crates/hub/src/agenthub/`, migrations 0014/0016 |

Each module runs its own migrations against the hub's one Postgres; how sqlx 0.9 keeps their
version histories apart (a migration table per module, or a schema per module) is settled in
R0 with a test, not assumed.

## Milestones

| # | Task | Acceptance evidence | State |
|---|---|---|---|
| R0 | The module contract: `modules/README.md` (the shape above), a `README.md` template, and a check in CI that every module has its README sections and that every table in its migrations carries its prefix; the migration mechanism per module decided and tested | the check fails on a module without a README or with an unprefixed table | not started |
| R1 | AgentHub as `devagotchi/agenthub` (new public repo), pulled in as a submodule at `modules/agenthub`, owning its `board_*` migrations; the hub's `agenthub/` folder becomes its MCP and page wiring only | AgentHub's own CI runs its tests on Postgres; this repo builds with the submodule | not started |
| R2 | Split `crates/hub` into the modules above, moving files with no behaviour change; `crates/core` becomes `machine-engine` | full CI green; no file in `hub-server` beyond wiring | not started |
| R3 | Postgres only: machines, push, audit, browser sessions, Nomen sign-ins move into their modules' tables; `auth.mode devagotchi`, the store's SQLite backend and twin migrations, and the hub's SQLite board fallback are deleted | a hub test runs `hub serve` with every feature exercised and finds no `*.sqlite3` in the state directory; `grep -rln rusqlite modules/{identity,machines,audit,push,hub-server}` empty | not started |
| R4 | `tenancy` as its own module, out of the default build; a one-owner hub has no tenant code in its binary | the default binary's symbols carry no `tenancy`; a hub built with it passes the tenant tests | not started |
| D1 | Sign-in as Desktop Commander's: Continue with Google, email + password (Sign in / Sign up), Forgot password by email; nothing else before the AI app gets its connection | `tests/first_connect_browser.py`; live check on app.devagotchi.com once Google and mail are set (below) | not started |
| D2 | Machine connect as Desktop Commander's: one line per OS, no sudo, installs for the user and starts with the computer; the machine shows a code, the person approves it in the browser they are signed in with (device grant), and the AI says the machine is online | e2e on Linux and Windows CI: line → code → approve → `machines` online → `run echo ok` | not started |
| D3 | Tool parity: search sessions, interactive processes and sessions, process list and kill, `edit_block` by range and fuzzy, `write_pdf`, reading PDF/Excel/DOCX, settings get/set, recent tool calls; each with the machine's grants deciding what runs | one test per tool against a real broker; the tool list compared to teardown §1.3 in a test | not started |
| D4 | Local mode as Desktop Commander's: one line adds Devagotchi to Claude Code, Codex, Claude Desktop, Cursor on the same computer | a test per client config writer | not started |
| D5 | Listing in Claude's and ChatGPT's connector directories: everything their submission asks for is in place (docs, privacy, support URLs exist since #183) | the submission checklist, filled | not started |
| E1 | Passkeys after the first connection: no passkey anywhere before a machine is online (landing copy, claim, sign-in); the account page offers one after | `live_check.py` and `first_connect_browser.py` assert no "passkey" before the machine is online | not started |
| E2 | Then the expansions in their own plans: phone (push exists), AgentHub features, provenance (`self-hosted-and-provenance.md` P1-P4), connections | per plan | not started |

## Information needed from Jesse

1. **Google sign-in (D1):** a Google OAuth client, type Web application, redirect
   `https://app.devagotchi.com/auth/provider/google/callback`; you enter its id and secret on
   `/admin` → Sign-in (never pasted to an agent).
2. **Mail for Forgot password (D1):** a Resend key entered on the hub's mail settings, or SMTP.
3. **R1:** the new public repository `devagotchi/agenthub` is created by me with `gh repo create`
   once you say so here.

## Risks and approved decisions

- Approved by the operator's words above. Pre-release: the live test hub's machines, push
  subscriptions, audit rows and sessions are dropped at R3; machines join again.
- R2 moves most of the code; it lands as moves only, so a reviewer can check it with
  `git diff -M --stat` and the tests prove nothing changed.
- app.devagotchi.com uses multi-tenancy today; after R4 it runs a build with `tenancy` enabled.

## Verification receipts

None yet.

## Delivery

Implementation: not started. Validation: not started. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT STARTED**
