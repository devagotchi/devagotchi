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

## The shape: the approved layout pattern, applied

The shape is [.ai/patterns/layout.md](../../patterns/layout.md) (operator, 2026-11-13, merged
in #185) and its steps in [structure-cleanup](structure-cleanup.md): a crate per boundary;
inside a crate a folder per domain, `src/<domain>/mod.rs` its front door; tests at
`<domain>/tests.rs`; the crate's top level wiring only; 500 lines a new file, held by the size
ratchet. No crate or folder called `core`, `common`, `utils` or `shared`. This plan adds two
things to that pattern (R0) and names where each piece of the free edition lives in it:

- **The front door describes the domain.** Every domain's `mod.rs` module docs carry the same
  sections: *Owns* (its tables, by prefix), *Serves* (routes, MCP tools, pages), *Settings*
  (each with its page), *Depends on*. A person or an agent reads one file and knows the domain.
- **Each domain's tables carry its prefix**, in schema `devagotchi`, Postgres only; a test
  fails on a table without one.

| Where | Owns | Table prefix | From today |
|---|---|---|---|
| crate `machine-engine` | running programs and sessions, files, search, the job journal (on the machine; SQLite there is the machine's own, ADR 0002) | none (machine-local) | `crates/core`, `crates/broker` |
| crates `link`, `ipc` | the machine's outbound link and local IPC (already crates) | none | as today |
| crate `agenthub` (git submodule of `devagotchi/agenthub`) | boards, tickets, notes, approvals, workflows, wake; usable alone on any Postgres (and ClickHouse) | `board_` | `crates/agenthub`, `crates/hub/src/agenthub/`, migrations 0014/0016 |
| crate `tenancy` | many accounts and tenants on one hub, `/operator`, limits; **not in the default build** | `tenancy_` | `tenant_*.rs`, `orgs*`, `web/operator.rs`, `web/hosted.rs`, the RLS policies |
| hub `src/identity/` | accounts, email + password, Google/GitHub/OIDC/Shippin sign-in, browser sessions, passkeys (offered after the first machine), the OAuth server AI apps sign in with, the device page | `auth_`, `oauth_` | structure-cleanup S4 `identity/` plus `accounts/`, `web/sign_in.rs`, `web/passkeys.rs`, `web/sessions.rs` |
| hub `src/machines/` | the registry, joining (device grant), updates, provenance later | `machine_` | structure-cleanup S4 `machines/` |
| hub `src/notify/` | Web Push and mail (SMTP or a Resend connector) | `push_`, `mail_` | structure-cleanup S4 `notify/` |
| hub `src/ops/` | audit chain, the ClickHouse export | `audit_`, `analytics_` | structure-cleanup S4 `ops/` |
| hub `src/connectors/` | outside services through the hub (structure-cleanup ruling 1 names them **connectors**; #188 named them "connections", which the rename plan undoes) | `connector_` | `connections.rs`, `accounts/connections.rs` |
| hub `src/site/` | public pages, copy, assets, docs, `llms.txt` | none | `site/`, `web/landing*`, `web/public.rs`, `web/discovery.rs` |
| hub `src/mcp/` | the MCP tools, grouped by domain (Machines, Files, Processes, Search, AgentHub, Connectors, Setup) | none | `hub.rs`, `output_schema.rs`, the tool routers |
| hub top level | `lib.rs`, `config.rs`, `store.rs`, `web.rs`: wiring only | none | structure-cleanup S4 end state |

## The UI: every domain is run from its own pages

Operator, 2026-11-14: "IT NEEDS TO HAVE UI TO SUPPORT IT". Standing rule: every setting is set
in the UI, guided, grouped and next to what it affects. So each domain's front door names its
pages:

| Domain | What the person does there |
|---|---|
| `identity` | `/admin` → Sign-in: Sign in with Shippin (on by default, one switch to turn off), Google with your own client, GitHub, any OpenID Connect, email + password; `/account` → Sign-in and security: change password, linked sign-ins, passkey (offered after the first machine) |
| `machines` | Machines: add (the one line per OS), approve a machine's code, rename, revoke, update, where it runs |
| `mail` | `/admin` → Mail: SMTP or a Resend connection, send a test |
| `push` | You → Notifications: this phone on or off, send a test, the phones that get them |
| `connectors` | You → Connections today (exists); named Connectors by the rename plan |
| `analytics` | `/admin` → Statistics: ClickHouse (exists) |
| `audit` | `/admin` → Activity: the record, filter by machine, agent, person |
| `agenthub` | Boards and tickets (exist), board settings |
| `tenancy` | `/operator`, only in a build with it |
| `hub-server` | `/admin` → Hub: address, updates (track, version running), backups |

Deploy-time values that cannot be set from a page because the hub needs them to start (its
address, its Postgres URL) are listed as such in the hub's `config.rs` front door and nowhere else. A CI
check compares every config key with the domain pages and that list.

## Sign in with Shippin, on from the first start

Operator, 2026-11-14: "shippin.ai available from deployment that can be toggled off so Google
auth is always available". A self-hosted hub shows **Continue with Shippin** (Google and email
through Shippin) with nothing set up, and the owner can turn it off on `/admin` → Sign-in.

Checked 2026-11-14: Sign in with Shippin exists as an OpenID Connect provider
(`config/providers.rs`, `accounts/identities.rs`), but a hub must be registered with Nomen by
hand (`devagotchi hub connect-nomen`); `https://id.shippin.cloud/.well-known/openid-configuration`
has no `registration_endpoint`; it accepts public clients (`none`) and the device grant.

What makes it work with no registration: Nomen accepts **Client ID Metadata Documents** (the
direction MCP takes in its 2026-07-28 revision). A hub's client id is a URL on the hub,
`https://<hub>/oauth/client.json`, naming its redirect address; Nomen fetches it, checks the
redirect matches, and treats it as a public client with PKCE. Nothing is registered and no
secret exists. This is a change in Nomen (`D:\localdev\nomen-react-go`), made and tested there
first (S0).

AgentHub runs its own migrations against the hub's one Postgres; how sqlx 0.9 keeps its
version history apart from the hub's (a migration table of its own, or a schema of its own) is
settled in R1 with a test, not assumed.

## Milestones

| # | Task | Acceptance evidence | State |
|---|---|---|---|
| R0 | Extend `.ai/patterns/layout.md`: the front-door sections (*Owns*, *Serves*, *Settings*, *Depends on*) and table prefixes; a test that every migration's tables carry a domain prefix | the test fails on an unprefixed table; the pattern doc diff | done in increment 145 (receipt below) |
| R1 | AgentHub as `devagotchi/agenthub` (new public repo), pulled in as a submodule at `crates/agenthub`, owning its `board_*` migrations; the hub's `agenthub/` folder becomes its MCP and page wiring only | AgentHub's own CI runs its tests on Postgres; this repo builds with the submodule | not started |
| R2 | [structure-cleanup](structure-cleanup.md) S1 to S4, reconciled with #184 first (its open item); `crates/core` becomes `machine-engine` | each step's own evidence there | not started |
| R3 | Postgres only: machines, push, audit, browser sessions, Nomen sign-ins move into their domains' tables; `auth.mode devagotchi`, the store's SQLite backend and twin migrations, and the hub's SQLite board fallback are deleted | a hub test runs `hub serve` with every feature exercised and finds no `*.sqlite3` in the state directory; `grep -rln rusqlite crates/hub crates/store` empty | in progress, increment 147 (#195): machines, push, audit, browser sessions and Nomen sign-ins on Postgres; `auth.mode devagotchi`, the SQLite backend and twin migrations deleted; every table prefixed (receipt below). Left: the hub's SQLite board fallback and the rusqlite in `crates/hub` (board CLI, board import, `ai` journal), which move out with R1/R2 |
| R4 | `tenancy` as its own crate, out of the default build; a one-owner hub has no tenant code in its binary | the default binary's symbols carry no `tenancy`; a hub built with it passes the tenant tests | not started |
| U0 | The UI contract: each domain's pages listed in its front door; `/admin` grouped by domain; the CI check that every config key has a page or is a listed deploy-time value | the check fails on a config key with neither | not started |
| S0 | Nomen accepts Client ID Metadata Documents (public client, PKCE, redirect checked against the hub's document); in `nomen-react-go`, with its tests | Nomen's tests; a hub on a fresh domain signs in through id.shippin.cloud with nothing registered | not started |
| D1 | Sign-in as Desktop Commander's: **Continue with Shippin** on by default (Google and email through Shippin; off with one switch on `/admin` → Sign-in), Continue with Google when the owner adds their own client, email + password (Sign in / Sign up), Forgot password by email; nothing else before the AI app gets its connection | `tests/first_connect_browser.py` on a fresh hub with no settings: the Shippin button is there, turning it off removes it; live check on app.devagotchi.com | not started |
| D2 | Machine connect as Desktop Commander's: one line per OS, no sudo, installs for the user and starts with the computer; the machine shows a code, the person approves it in the browser they are signed in with (device grant), and the AI says the machine is online | e2e on Linux and Windows CI: line → code → approve → `machines` online → `run echo ok` | not started |
| D3 | Tool parity: search sessions, interactive processes and sessions, process list and kill, `edit_block` by range and fuzzy, `write_pdf`, reading PDF/Excel/DOCX, settings get/set, recent tool calls; each with the machine's grants deciding what runs | one test per tool against a real broker; the tool list compared to teardown §1.3 in a test | not started |
| D4 | Local mode as Desktop Commander's: one line adds Devagotchi to Claude Code, Codex, Claude Desktop, Cursor on the same computer | a test per client config writer | not started |
| D5 | Listing in Claude's and ChatGPT's connector directories: everything their submission asks for is in place (docs, privacy, support URLs exist since #183) | the submission checklist, filled | not started |
| E1 | Passkeys after the first connection: no passkey anywhere before a machine is online (landing copy, claim, sign-in); the account page offers one after | `live_check.py` and `first_connect_browser.py` assert no "passkey" before the machine is online | not started |
| E2 | Then the expansions in their own plans: phone (push exists), AgentHub features, provenance (`self-hosted-and-provenance.md` P1-P4), connections | per plan | not started |

## Information needed from Jesse

1. **S0:** Nomen (`nomen-react-go`) gains Client ID Metadata Documents, and id.shippin.cloud
   runs that build. Say whether I make that change in the Nomen repository, and who deploys
   id.shippin.cloud.
2. **Mail for Forgot password (D1):** a Resend key entered on `/admin` → Mail, or SMTP.
3. **R1:** the new public repository `devagotchi/agenthub` is created by me with `gh repo create`
   once you say so here.
4. Google with the owner's own client stays optional: with Shippin on, Google works without it.

## Risks and approved decisions

- Approved by the operator's words above. Pre-release: the live test hub's machines, push
  subscriptions, audit rows and sessions are dropped at R3; machines join again.
- R2 moves most of the code; it lands as moves only, so a reviewer can check it with
  `git diff -M --stat` and the tests prove nothing changed.
- app.devagotchi.com uses multi-tenancy today; after R4 it runs a build with `tenancy` enabled.

## Verification receipts

R0 (increment 145, Windows 11, rustc 1.99.1, Postgres 18 in Docker, 2026-11-14):
`cargo test -p devagotchi-store --features testing` with `DEVAGOTCHI_TEST_POSTGRES_URL` set: 2
passed, including `every_table_in_the_hub_schema_carries_its_domain_prefix`. With
`connections` taken off `UNPREFIXED` it fails naming `connections`; restored, it passes.
`cargo test -p devagotchi --test layout`: 7 passed (`crates/store/src/lib.rs` 900 to 874,
its tests moved to `src/tests.rs`).

R3, increment 147 so far (Windows 11, Postgres 18 in Docker, 2026-11-15; PR #195 at
`69d1c7d`): `cargo test --no-fail-fast --features devagotchi-store/testing` with
`DEVAGOTCHI_TEST_POSTGRES_URL` set: 641 passed, 0 failed (hub 504, store 3, layout 7,
one_binary 7). `python tests/e2e.py` on a new database: E2E PASSED, 291 checks. Docker: the
compose stack first ran `ghcr.io/devagotchi/devagotchi:main` (`ca35ac0`); its hub container
was replaced by this branch's image on the same database and volume: migrations to 0029
applied; one row seeded in each of the five old-named tables came through under its new name,
no old name and no unprefixed table left, `api_key_tenant` and `machine_pushes_of` answer
(also as `devagotchi_hub`), row-level security forced; `/healthz` ok, `tenant_rls` true, audit
chain verifying on Postgres; the old hub's `*.sqlite3` files untouched since it stopped.

## Delivery

Implementation: not started. Validation: not started. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT STARTED**
