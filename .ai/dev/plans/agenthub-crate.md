# AgentHub as its own crate, on your Postgres and your ClickHouse

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-13: "get all of agenthub properly as its own module that
  can extend and integrate with an existing postgres and if they have it clickhouse".
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`; A1 is
  `increment-137/agenthub-crate` from `d4166f8` (PR #184, merged `332bf25`); A4 is
  `increment-138/agenthub-analytics-settings` from `332bf25`
- Objective: AgentHub is a library crate (`crates/agenthub`, `devagotchi-agenthub`) that owns
  the board, its storage and its analytics export. It runs on a Postgres the person already
  has (in a schema of its own, beside whatever else lives there) and sends its events to their
  ClickHouse when they have one, with both connections set and checked from the hub's UI. The
  hub, and later the cloud edition, use it through a public API instead of reaching into it.
- Ruled 2026-11-15 ([ADR 0005](../../../docs/adr/0005-agenthub-postgres-everywhere.md)): Postgres in every form;
  a library in Devagotchi (schema `devagotchi`, `board_*`) and in Shippin, a binary or a container
  on its own; SQLite boards deleted. A6 to A10 below.
- Non-goals: a second board implementation; moving people's data without a backup and a way
  back; ClickHouse as a requirement (Postgres stays the board of record).
- Canonical architecture / related plan: [AgentHub module](agenthub-module.md) (the folder,
  PR #178), [factory](agenthub-factory.md), [cloud edition](cloud-edition.md) (C2 extension
  points), [hub settings](hub-settings.md) (M4 ClickHouse in the UI), ADR 0001 (Postgres of
  record, ClickHouse for statistics), `crates/store/src/lib.rs`, `crates/agenthub/src/clickhouse.rs`.

## Where it stands (main, `d4166f8`)

| Need | Today |
|---|---|
| Its own module | A folder in the hub crate (`crates/hub/src/agenthub/`, 19.5k lines). The engine needs only small helpers from the hub; the MCP tools are methods on the hub's `Hub`; the CLI, the hosted registry and the issue web routes use hub accounts and web state |
| An existing Postgres | The hub's tables, boards included, live in their own schema (`devagotchi`, or `storage.schema` / `DEVAGOTCHI_HUB_STORAGE_SCHEMA`), made if missing, with tenant row-level security; a shared database such as Supabase works. But the board tables come from the hub's migration chain and reference the hub's tenant table, so AgentHub cannot sit on a database of its own |
| ClickHouse if they have it | Board events go through a Postgres outbox to ClickHouse (`warehouse.rs`), optional, degraded but never blocking when ClickHouse is down. Set only by config file and environment, not in the UI |

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| A1 | Two crates, no behaviour change: `crates/store` (`devagotchi-store`: the shared database layer, Postgres or SQLite, schema, tenant RLS, retrying transactions, the migrations) and `crates/agenthub` (`devagotchi-agenthub`: the board engine, leases, workflow, approvals, inbox, wake, welcome, the hosted Postgres board, the local-to-hosted import, issue types, the ClickHouse export). The hub keeps its glue: MCP tools (`impl Hub`), the board CLI and its hub client, the tenant registry, web routes. Hub paths (`crate::agenthub::…`, `crate::store::…`) keep working through re-exports | none | the whole workspace test suite on SQLite and Postgres (CI), `cargo tree` shows the agenthub crate does not depend on the hub | done (this PR) |
| A2 | AgentHub's own migration chain: board tables created and changed by `devagotchi-agenthub`'s migrations, tracked in their own table, idempotent over a hub that already made them (0014, 0015, 0016, 0018), with no reference to the hub's tenant table (tenant ids checked by format, tenants mirrored into AgentHub's schema) | A1 | parity test both backends; upgrade test from a hub at 0023 with boards in it; fresh-database test | done in free-edition R1 (increment 148, #196): AgentHub's own `migrations/` with history `_agenthub_migrations`, adopting a hub's existing boards; no foreign key to and no lock on a hub table, so no tenant mirror is needed |
| A3 | AgentHub on a separate Postgres: the hub opens the board store at its own URL and schema when given one (`agenthub.storage`, default: the hub's own), so boards can live in a database of their own. No tenant mirror (R1 removed the dependency); the RLS role made when the login role may, refused clearly when it may not | A2, A6 | tests on two databases; `tests/deploy_scripts.sh` | not started; unblocked by R1 |
| A4 | Both connections in the UI: an AgentHub card in `/admin` (grouped with storage): board database (the hub's, or a URL and schema), ClickHouse (URL, database, user, password sealed with `settings.key`), a Check button for each, the outbox backlog and last export; ClickHouse applies without a restart, the board database at the next start with the page saying so. Environment variables still win, and the card shows when they do | A3, hub-settings M4 | `hub_settings_tests`: owner only, same origin, bad address or database refused, ClickHouse down refuses and saves nothing, saved sealed and exporting at once with the password, kept on a second save, off, started again after a restart, shown not changed when the config sets it, refused on SQLite | done (increment 138), except the board database's address, which stays an environment setting: the hub cannot move itself while it runs, so the page shows where it is and the steps |
| A6 | **The schema is the host's** ([ADR 0005](../../../docs/adr/0005-agenthub-postgres-everywhere.md)): AgentHub's migrations name no schema and run on the host's `search_path`; Devagotchi passes `devagotchi` (nothing moves), standalone defaults to `agenthub`, Shippin names its own | A2 | AgentHub's tests in schema `agenthub` and in `devagotchi`; a Devagotchi database migrated before A6 opens unchanged | not started |
| A7 | **No SQLite board** (ADR 0005): `src/lib.rs`'s SQLite board, `project.rs`'s `board.sqlite3` and their file-based tests go; the protocol tests run on the hosted board; `devagotchi board` talks to a hub or an AgentHub server | A6 | `git grep rusqlite` empty in the agenthub repository; its tests on Postgres; this repository's e2e | not started |
| A8 | **AgentHub as a server**: an `agenthub` binary serving the board's MCP tools and HTTP API on one port, config by file and environment (`AGENTHUB_DATABASE_URL`, schema, sign-in issuer, Nomen by default), released per OS from the agenthub repository and as `ghcr.io/devagotchi/agenthub` | A6, A7 | its CI: the image starts on a fresh Postgres, `/healthz` ok, an MCP client creates, claims and completes a ticket | not started |
| A9 | **Outbox sinks**: the outbox that feeds ClickHouse also feeds a signed webhook, which is how Shippin and rrflow react to board changes | A8 | delivery at least once with stable ids, a bad signature refused, a down receiver retried with backoff and shown in `/healthz` | not started |
| A10 | **rrflowDB as a qualification target**: AgentHub's Postgres suite runs against rrflowDB's PostgreSQL profile once rrflow's H-08 lands | rrflow H-08 | the suite's result recorded on each rrflowDB candidate | waits on rrflow |
| A11 | **AgentHub as its own surface** (operator, 2026-11-06, `increment-113/plan-editions-agenthub`, absorbed here 2026-11-15): one authorization server, two protected resources on their own hosts. `agenthub.<domain>` (e.g. `agenthub.devagotchi.com`) serves the board MCP at `/mcp` with its own resource metadata (RFC 9728) naming the same authorization server, and tokens bound to it by audience (RFC 8707), so a token for one is not valid at the other; one sign-in, one account, two connectors. Same process to start, routed by `Host`. The machines MCP gains `agenthub_connect` (the AgentHub connector URL and a paste-ready line); `board_*` stay on the main `/mcp` for a transition period, marked deprecated, then go. Open: the transition period (proposed 30 days); whether `agenthub.<domain>/` is the boards' UI (proposed yes) | A8 | a client connected to each surface; a token from one refused at the other; `agenthub_connect` returns the line | not started |
| A5 | What others build on: a documented SQL surface (views over tickets, events and notes in AgentHub's schema, read-only role), the ClickHouse table's columns as a contract, and the crate's API for the cloud edition (C2) | A2 | docs and a test that the views answer | not started |

## Risks and approved decisions

- A1 moves code only: every hub path keeps working through re-exports, so the diff is the
  move plus visibility (`pub(crate)` items the hub uses become the crate's `pub` API).
- A2 touches data. Existing hubs (app.devagotchi.com is a test deployment, alpha) keep their
  tables where they are; AgentHub's chain adopts them (`IF NOT EXISTS`, recorded as applied).
  `deploy/compose/update.sh` backs up before every deploy.
- A3's separate database cannot share a transaction with the hub's tables. Today a board write
  touches only board tables plus a tenant row lock; the mirror replaces the lock. Checked in A2
  before A3 starts.

## Verification receipts

A1, 2026-11-13, Windows 11, Postgres 18 (docker `dvg-pg`), branch `increment-137/agenthub-crate`:
- `cargo test -- --test-threads=8` over the default members with
  `DEVAGOTCHI_TEST_POSTGRES_URL` set: every binary ok, 0 failed; the hub library ran 528 tests,
  the same count as `main` at `d4166f8` (no test lost in the move). After the ClickHouse export
  moved: `warehouse`, `config`, `analytics`, `agenthub` filters, 161 passed.
- `cargo build -p devagotchi-hub`: 19 warnings against 21 on `main`, none new (two items the
  hub never used are now the crate's public API).
- `cargo tree -p devagotchi-agenthub -e normal`: contracts, connectors, store and third-party
  crates only; no `devagotchi-hub`.
- Not run here: Linux CI jobs, `tests/deploy_scripts.sh`, a real ClickHouse (`analytics_probe`);
  CI ran them on PR #184: 35 passed, 3 skipped (after `deny.toml` let the crate link reqwest).

A4, 2026-11-14, Windows 11, Postgres 18 (docker `dvg-pg`), branch
`increment-138/agenthub-analytics-settings`:
- `hub_settings_tests::a_clickhouse_saved_on_the_settings_page_is_checked_sealed_and_exported_to_at_once`
  on SQLite and Postgres (two fresh databases made, so the Postgres half ran), against a
  stand-in ClickHouse that is switched down and up.
- `place_tests`: a store's place shows no user, password or query.
- Not run: a real ClickHouse from the page (the stand-in answers every query); the browser.

## Delivery

Implementation: A1 done (merged), A4 done. Push: not_verified. PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
