# AgentHub as its own crate, on your Postgres and your ClickHouse

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-13: "get all of agenthub properly as its own module that
  can extend and integrate with an existing postgres and if they have it clickhouse".
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`; A1 is
  `increment-137/agenthub-crate` from `d4166f8`
- Objective: AgentHub is a library crate (`crates/agenthub`, `devagotchi-agenthub`) that owns
  the board, its storage and its analytics export. It runs on a Postgres the person already
  has (in a schema of its own, beside whatever else lives there) and sends its events to their
  ClickHouse when they have one, with both connections set and checked from the hub's UI. The
  hub, and later the cloud edition, use it through a public API instead of reaching into it.
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
| A2 | AgentHub's own migration chain: board tables created and changed by `devagotchi-agenthub`'s migrations, tracked in their own table, idempotent over a hub that already made them (0014, 0015, 0016, 0018), with no reference to the hub's tenant table (tenant ids checked by format, tenants mirrored into AgentHub's schema) | A1 | parity test both backends; upgrade test from a hub at 0023 with boards in it; fresh-database test | not started |
| A3 | AgentHub on a separate Postgres: the hub opens the board store at its own URL and schema when given one (default: the hub's own), so a person's existing database holds their boards. Tenants mirrored on first use; RLS role made when the login role may, refused clearly when it may not | A2 | tests on two databases; `tests/deploy_scripts.sh` | not started |
| A4 | Both connections in the UI: an AgentHub card in `/admin` (grouped with storage): board database (the hub's, or a URL and schema), ClickHouse (URL, database, user, password sealed with `settings.key`), a Check button for each, the outbox backlog and last export; ClickHouse applies without a restart, the board database at the next start with the page saying so. Environment variables still win, and the card shows when they do | A3, hub-settings M4 | hub_settings tests; browser test | not started |
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
  CI runs them on the PR.

## Delivery

Implementation: A1 done. Push: not_verified. PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
