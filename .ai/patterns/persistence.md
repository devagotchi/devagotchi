# Pattern: persistence (Postgres through `devagotchi-store`)

How Devagotchi keeps rows:

- One Postgres engine, `platform/store`.
- Migrations owned by the crate that owns the tables: `engine/hub/migrations`, plus
  `agenthub/migrations` in the AgentHub submodule.
- Tenants kept apart by the database as well as by the hub.
- Tests on a throwaway Postgres.

Postgres only, with no SQLite twin (ADR 0001, ADR 0005, [layout.md](layout.md) rule 8).
Follow this for every new table, query and store test. The shared rules in
[README.md](README.md) hold here too and are not repeated: G1 (`Store` owns SQL,
transactions and tenant scope), G8 (tenant from authentication, forced RLS, `tenant_id = $n`),
G9 (one clock, nothing outside the store inside `transaction!`), G10 (where tests go), G11
(no file paths in new migration headers), G12 (checks are ratchets) and G13 (fixtures and
`or_skip!`).

## Exemplars

- A domain store: `engine/hub/src/connections/stored.rs` (`stored`, `COLUMNS`,
  `Accounts::connections`, `connection`, `save_connection`, `remove_connection`, `touch_connection`,
  lines 28-160). Each method is one `transaction!`, scopes first and still names
  `tenant_id = $1`. The three writes take `now` before the block, and `save_connection` and
  `remove_connection` write the chain record with `tenants::record_in` in the same transaction.
  Do not copy its `.max(0) as u64` at lines 35 and 38 (rule 19).
- A hub-wide store: `engine/hub/src/ops/hub_settings.rs` (`Accounts::hub_settings` and the
  writes after it).
- The engine: `platform/store/src/lib.rs` (`transaction!` 111-139, `Store::open_in` 165-289,
  `scope` 309-321, `tenant_of` 323 on, `begin`/`finish` 401-442, `collided` 468-473).
- RLS and a lookup function: `engine/hub/migrations/0021_api_keys.sql:23-43`. Its table name
  (`api_keys`, renamed by 0029) and its header (a code path and a `../sqlite` line) predate
  rules 7 and 11; do not copy those.
- A prefixed tenant table with a child key: `agenthub/migrations/0002_board_sources.sql`.
  Its `IF NOT EXISTS` guards are AgentHub's adoption style (see Deviations), not the hub's.
- A hub-wide table: `engine/hub/migrations/0036_signal_sinks.sql` (body only: its header names
  a file that does not exist, rule 11).
- An identity sequence with least-privilege grants: `engine/hub/migrations/0027_audit.sql:9-29`.
- A store test: `engine/hub/src/machines/devices/mod.rs` (`test_registry`, 462-468) with
  `engine/hub/src/machines/devices/tests.rs`
  (`a_code_binds_one_key_once_and_only_for_its_device`).

## The shape

### A migration (tenant table)

```sql
-- <What it keeps> (<increment or plan>, <domain>): <one row per ...>. <What is never stored
-- in the clear: only its SHA-256 / sealed>. Under the tenant's row-level security. Add-only.
CREATE TABLE devagotchi.<prefix>_<things> (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  id TEXT NOT NULL,
  ...,
  created_at BIGINT NOT NULL,              -- Unix seconds (AgentHub: ms)
  PRIMARY KEY (tenant_id, id)              -- tenant_id leads; a child's FK is (tenant_id, parent)
);
ALTER TABLE devagotchi.<prefix>_<things> ENABLE ROW LEVEL SECURITY;
ALTER TABLE devagotchi.<prefix>_<things> FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_only ON devagotchi.<prefix>_<things>
  USING (tenant_id = current_setting('devagotchi.tenant', true))
  WITH CHECK (tenant_id = current_setting('devagotchi.tenant', true));
-- Only if a key must lead to its tenant before one is known:
CREATE FUNCTION devagotchi.<thing>_tenant(key_hash text)
RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT tenant_id FROM <prefix>_<things> WHERE hash = key_hash
$$;
REVOKE ALL ON FUNCTION devagotchi.<thing>_tenant(text) FROM PUBLIC;
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.<prefix>_<things> TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.<thing>_tenant(text) TO devagotchi_hub;
  END IF;
END $$;
```

A hub-wide table (settings, machines, the audit chain: no tenant's data) has a header that
says "Hub-wide" and why, no policy, and the guarded GRANT (`0036_signal_sinks.sql:18-23`). A
table with an identity or serial column also grants `USAGE` on its sequence
(`0027_audit.sql:22-29`).

### A domain store

```rust
use sqlx::Row;
use crate::store::{secs, unsecs, transaction};

fn stored(row: &sqlx::any::AnyRow) -> Result<Thing, sqlx::Error> {
    Ok(Thing { name: row.try_get("name")?, created_at: unsecs(row.try_get("created_at")?), ... })
}
const COLUMNS: &str = "name, ..., created_at";

impl Accounts {
    pub async fn things(&self, tenant: &str) -> BridgeResult<Vec<Thing>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;           // first, before any RLS table
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {COLUMNS} FROM <prefix>_things WHERE tenant_id = $1 ORDER BY name"
            )))
            .bind(tenant)
            .fetch_all(&mut *connection)
            .await?;
            Ok(rows.iter().map(stored).collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub async fn save_thing(&self, tenant: &str, by: &str, name: &str) -> BridgeResult<()> {
        let now = now_secs();                                       // before the block (G9)
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            sqlx::query("INSERT INTO <prefix>_things (tenant_id, name, created_at) VALUES ($1, $2, $3)
                         ON CONFLICT (tenant_id, name) DO UPDATE SET ...")
                .bind(tenant).bind(name).bind(secs(now))
                .execute(&mut *connection).await?;
            tenants::record_in(connection, tenant, by, "thing.saved", name, now).await?; // same tx
            Ok(())
        })
    }
}

/// A step other transactions compose: it never opens its own.
pub(crate) async fn thing_in(connection: &mut AnyConnection, tenant: &str) -> Result<Option<Thing>, Failed> { ... }
```

## Rules

### Migrations

1. **The crate that owns the tables owns the migrations.**
   - `<crate>/migrations/` is embedded by `sqlx::migrate!` into `pub static MIGRATIONS: Migrator`
     with `ignore_missing: true`.
   - A crate other than the hub sets its own `table_name` (`_agenthub_migrations`), so the two
     histories never meet.
   - The crate has a `build.rs` that prints `cargo:rerun-if-changed=migrations`.
   - A hub's database runs `store::DATABASE` in order: the hub's migrations first, then
     AgentHub's.
   - Evidence: `engine/hub/src/store/mod.rs` (`MIGRATIONS`, `DATABASE`, lines 11-19),
     `agenthub/src/migrations.rs:11-15`, `engine/hub/build.rs`,
     `agenthub/build.rs`.
2. **Name and number.** A migration is `NNNN_<snake_case>.sql`, numbered one above the highest
   on `main` in that directory (hub: 0036 today). A number is never reused: hub 0014, 0015,
   0016 and 0018 are retired (they moved to AgentHub's 0001). On a rebase, renumber yours
   above main's highest.
3. **An applied migration never changes, not even a comment** (G11). sqlx compares each applied
   migration's checksum, and the checksum covers the whole file. A change is a new migration.
   The stale comments already in applied files wait for the baseline
   (`.ai/dev/plans/texas-baseline.md` M5).
4. **LF only.** Each repository's `.gitattributes` keeps `*.sql text eol=lf`: the hub's at
   `.gitattributes:5`, AgentHub's at `agenthub/.gitattributes:3`. Each crate's tests call
   `devagotchi_store::testing::assert_line_endings(&MIGRATIONS)`.
5. **Add by default; remove by plan.** Add tables, columns with defaults, indexes and grants.
   A rename or drop is a migration of its own under a plan, as 0007, 0029 and 0030 were.
   `a_store_a_newer_hub_migrated_still_opens` (`engine/hub/src/store/tests.rs`) keeps an older
   build opening a newer store, which is what rolling an image back needs. This is not a
   compatibility layer (C17 in the rewrite plan): in alpha a table may be renamed or dropped
   when its plan says so. What cannot change is an applied file (rule 3).
6. **Hub DDL names its schema.** Every table, function and sequence a hub migration makes after
   0007 is `devagotchi.<name>`. AgentHub's do the same today; ADR 0005 intends a host-given
   schema (see Deviations).
7. **Every table carries its domain prefix** ([layout.md](layout.md) rule 8). A new prefix goes
   in layout rule 8 and in `DOMAIN_PREFIXES` (`engine/hub/src/store/tests.rs`) in the same PR.
8. **A tenant's rows:**
   - `tenant_id TEXT NOT NULL` leads the primary key, and a child's foreign key is
     `(tenant_id, parent...)`.
   - RLS is ENABLEd and FORCEd, with policy `tenant_only` USING and WITH CHECK on
     `current_setting('devagotchi.tenant', true)`.
   - Any other table is hub-wide, and its header says why. The one hub-wide table that holds
     `tenant_id` is `analytics_events` (made as `hub_analytics_events` in 0009, renamed by
     0030; its header says why at 0009:19-20).
9. **Grants are explicit and guarded.**
   - Every new table gets the least privilege it needs, granted to `devagotchi_hub` inside
     `IF EXISTS (SELECT 1 FROM pg_roles ...)`.
   - Sequences and functions always get explicit grants.
   - 0007's `ALTER DEFAULT PRIVILEGES` covers only tables, and only those created later by the
     same role.
   - Where another crate's migrations may have run first, the hub grants afterwards (0035).
10. **Cross-tenant questions go through a `SECURITY DEFINER` function.**
    - It has `SET search_path = devagotchi, pg_temp` and `REVOKE ALL ... FROM PUBLIC`, and is
      granted inside the guard.
    - It answers the narrowest thing the question needs and never a tenant's row: a tenant id
      (`tenant_of`, `api_key_tenant`, `invite_tenant`, `offer_tenant`, `workflow_tenant`,
      `tenants_in`, `outbox_pending_tenants`), ids with a role (`orgs_of`, `projects_of`), a
      count (`live_unsecured`, `outbox_backlog`) or a single flag (`machine_pushes_of`).
    - A cross-tenant write (`outbox_resend`) is the operator's maintenance verb and is named so
      in its comment.
11. **The header** says what the table keeps, its domain, whether it is under RLS or hub-wide
    (and why), what is never stored in the clear, and "Add-only". It names the domain or plan,
    never a file path (G11): a path in an applied migration cannot be corrected when the file
    moves.

### Queries

12. **Only through `Store`** (G1).
    - Writes, and every read of an RLS table, run in one `transaction!` (SERIALIZABLE; it
      retries on 40001, 40P01 and 23505).
    - `store.read()` is only for hub-wide tables and lookup functions.
    - `Store::begin()` and `finish()` are public only because `transaction!` expands in its
      caller. No production code calls them. One test does
      (`engine/hub/src/agenthub/tests/standalone.rs`, `the_hubs_role_exports`, to check the
      pool's role).
    - `pool()` and `admin()` exist only under `cfg(any(test, feature = "testing"))`, and only
      dev-dependencies enable `testing`.
    - The one connection outside `Store` is the board-change `PgListener`
      (`engine/hub/src/agenthub/mcp/boards.rs`, `postgres_listener`). It runs only `LISTEN` and
      reads no table.
13. **Scope first.** In a transaction that touches an RLS table, `store.scope(connection,
    tenant)` comes before the first such statement. The hub's identity code uses
    `Accounts::enter`, which runs `tenant_of` and then `scope`. AgentHub uses
    `HostedBoard::scope`, which also locks the project row and returns the database clock (G9).
    Where the tenant comes from is G8.
14. **Name the tenant anyway** (G8). Every statement on an RLS table carries `tenant_id = $n`.
    RLS is "a second wall behind the hub's own checks" (`engine/hub/README.md`, "This is a
    second wall"). A sweep such as `DELETE ... WHERE tenant_id = $1 AND expires_at <= $2` then
    says it clears one tenant's rows.
15. **The block may run again.** G9 says what stays outside `transaction!` and which clock a
    decision uses. Inside it:
    - A refusal is `Err(BridgeError::new(code, msg).into())`: it rolls back and is not retried.
    - A store error is `?`: a collision runs the block again from the top.
16. **Compose with `*_in`.** A step shared by transactions is
    `async fn <verb>_in(connection: &mut AnyConnection, ...) -> Result<T, Failed>` (or
    `sqlx::Error`), and it never opens a transaction. The tenant chain is written with
    `tenants::record_in` (`engine/hub/src/identity/accounts/tenants/mod.rs`), in the
    transaction whose change it records.
17. **SQL is a `&'static str` with `$n` binds.**
    - `AssertSqlSafe` wraps only strings built from compile-time constants (`COLUMNS`),
      `valid_schema`-checked names, or placeholder numbers. A value is never interpolated.
    - A dynamic bind list hand-numbers `$n` from a counter, as
      `agenthub/src/clickhouse.rs` (`export_tenant`, lines 313-334) does.
    - Do not use `sqlx::QueryBuilder` on the Any pool: `AnyArguments` keeps the default `?`
      placeholder, which Postgres rejects.
18. **Tables unqualified, functions qualified.** Rust SQL names tables bare, because `Store`
    sets `search_path` to `devagotchi, <home>` on every connection (`platform/store/src/lib.rs:220`).
    It calls lookup functions as `devagotchi.<fn>(...)`.
19. **Types the Any driver carries.**
    - Times are BIGINT Unix seconds (AgentHub: milliseconds) and cross with
      `store::secs`/`unsecs`, never `.max(0) as u64`.
    - JSON is TEXT, and flags are 0/1 INTEGER (convention: Bool would also work).
    - Rust decodes the column's own width: BIGINT is `i64`, INTEGER is `i32`.
    - Each table has one row mapper and one `COLUMNS`, and reads with `try_get` by name.
    - The exception is `audit_entries.at`, which is TEXT on the hash chain (0027:11).

### Role

20. **The hub logs in as the role that migrates and works as `devagotchi_hub`.**
    - `Store` switches with `SET ROLE` only where the database granted that role the schema
      (`platform/store/src/lib.rs:221-250`, tested by
      `the_hub_role_is_taken_only_where_the_database_granted_it` in `platform/store/src/tests.rs`).
    - `enforced()` is reported on `/healthz` as `store.tenant_rls`
      (`engine/hub/src/ops/health.rs:51`).
    - No code outside `platform/store` sets `devagotchi.tenant`, `SET ROLE` or `search_path`.

### Tests

21. **Fixtures are the store's** (shape and skip: G13).
    - A domain's fixture is built on `crate::store::testing::postgres().await?` (the hub's
      whole database) or `devagotchi_store::testing::postgres_with(&[&MIGRATIONS])` (another
      crate).
    - One fixture call per test. No `Vec` of stores to loop over, no `if let Some(test) =
      postgres().await`, and no hand-read environment variable.
22. **Below the API through `admin()`, isolation through `pool()`.** Rows a test plants go in
    through `store.admin()`, the login role. Assertions about what the hub can see run on
    `store.pool()` or `store.read()`, the hub's role. A catalog query that must see every table
    runs on `admin()`, on `pg_catalog` (`information_schema` shows only what the current role
    may access).
23. **Each crate with migrations proves its schema:** LF (`assert_line_endings`), prefixes,
    migration names and checksums, and, for the hub's database, RLS, grants and lookup-function
    locks on every table (see The mechanical check).

## Where files go

- Engine and test helpers: `platform/store/src/` (`lib.rs`, `settings.rs`, `testing.rs` behind
  feature `testing`). `lib.rs` is due to split (S10, Known violations).
- The hub's handle: `engine/hub/src/store/mod.rs` (`MIGRATIONS`, `DATABASE`, `open_in`,
  `for_command`, `testing`).
- A table's SQL lives in the domain that owns the table (layout rules 2 and 7), named under
  **Owns** in `<domain>/mod.rs`. A domain that splits its rows from its logic uses `stored.rs`
  or `stored/`, as connections, kernel, offers and workflows do.
- A domain's store tests follow G10, with the fixture beside the code under `#[cfg(test)]`.
- Schema-wide tests (prefixes, LF, and after B6 RLS, grants and migration names):
  `engine/hub/src/store/tests.rs`; AgentHub's in `agenthub/src/migrations/tests.rs`.
  Today the RLS behaviour test is in `engine/hub/src/identity/accounts/tenants/tests.rs`.

## The canonical test

```rust
// engine/hub/src/machines/devices/mod.rs
#[cfg(test)]
pub(crate) async fn test_registry() -> Option<std::sync::Arc<Devices>> {
    let test = crate::store::testing::postgres().await?;
    Some(std::sync::Arc::new(Devices::new(test.store)))
}
// engine/hub/src/machines/devices/tests.rs
#[tokio::test]
async fn a_code_binds_one_key_once_and_only_for_its_device() {
    let devices = or_skip!(test_registry());
    // behaviour through the API, then assertions
}
```

To run locally, have a Postgres on 127.0.0.1:55432 with user `postgres` and trust
authentication (on Linux, `tests/throwaway_postgres.sh` starts one from the system's server
binaries), then run
`DEVAGOTCHI_TEST_POSTGRES_URL=postgres://postgres@127.0.0.1:55432/postgres cargo nextest run -p devagotchi-hub --success-output final`.
Each test gets a fresh `dvg_test_<secs>_<hex>` database, and ones older than two hours are
swept once per process (`platform/store/src/testing.rs`, `empty_postgres` and `sweep`), much as
`#[sqlx::test]` does.

## Anti-patterns

- `for test in stores().await { ... }`, or a `Vec` built from `empty_postgres()`: the loop runs
  zero times without the URL and shows as passed.
- `if let Some(test) = postgres().await { ... }` or a bare `let ... else { return; }` around a
  store fixture.
- Reading an RLS table on `store.read()` or before `scope`: it returns nothing, silently.
- A `DELETE ... WHERE expires_at <= $1` on an RLS table that reads like a hub-wide sweep.
- Editing an applied migration, or saving one with CRLF.
- A file path in a migration header.
- A table without a prefix, without a grant (the hub's role then cannot see it), or with
  `tenant_id` and no forced policy.
- A `SECURITY DEFINER` function without `SET search_path ... pg_temp` or without
  `REVOKE ... FROM PUBLIC`.
- `sqlx::QueryBuilder` on the Any pool (it emits `?`).
- `devagotchi.<table>` in Rust SQL.
- `.max(0) as u64` on a stored time.

## The mechanical check

What holds today: `every_hub_migration_is_embedded_with_the_repositorys_line_endings` and
`every_table_in_the_hub_schema_carries_its_domain_prefix` (`engine/hub/src/store/tests.rs`),
AgentHub's `agenthub/src/migrations/tests.rs` (LF, own history and prefix, re-run),
`the_hub_role_is_taken_only_where_the_database_granted_it` (`platform/store/src/tests.rs`) and
`on_postgres_the_hubs_role_reads_and_writes_one_tenants_rows_only`
(`engine/hub/src/identity/accounts/tenants/tests.rs`). Rules 2, 3, 8-10 and 12-18 are held by
review until the checks below land (rewrite plan B6). None of them has been written or run, and
the files they name under `runtime/devagotchi/tests/` and `migrations/checksums.txt` do not exist
yet. Each lands green with the fixes it guards (G12).

1. **Catalog test** (rules 8, 9, 10), in `engine/hub/src/store/tests.rs`, run by hub-postgres:
   `every_tenant_table_is_held_by_forced_row_level_security_and_every_lookup_is_locked_down`.
   It takes `or_skip!(postgres())` and queries on `test.store.admin()`, so it covers the hub's
   and AgentHub's tables.
   - (a) RLS. `SELECT c.relname::text, c.relrowsecurity, c.relforcerowsecurity, EXISTS (SELECT 1
     FROM pg_policies p WHERE p.schemaname = 'devagotchi' AND p.tablename = c.relname AND
     p.policyname = 'tenant_only' AND p.qual LIKE '%devagotchi.tenant%' AND p.with_check LIKE
     '%devagotchi.tenant%') FROM pg_class c JOIN pg_attribute a ON a.attrelid = c.oid AND
     a.attname = 'tenant_id' AND NOT a.attisdropped WHERE c.relnamespace =
     'devagotchi'::regnamespace AND c.relkind = 'r'`. Assert all three flags for every row
     except `const HUB_WIDE_WITH_TENANT: [&str; 1] = ["analytics_events"]`. Today it returns 37
     rows: 36 tables under forced RLS (22 hub, 14 board) and `analytics_events`.
   - (b) Grants. Every `pg_tables` row with `schemaname = 'devagotchi'` has
     `has_table_privilege('devagotchi_hub', format('devagotchi.%I', tablename), 'SELECT')`, and
     every `pg_class` with `relkind = 'S'` in the schema has
     `has_sequence_privilege('devagotchi_hub', c.oid, 'USAGE')`.
   - (c) Lookup functions. Every `pg_proc` with `pronamespace = 'devagotchi'::regnamespace AND
     prosecdef` has `proconfig @> ARRAY['search_path=devagotchi, pg_temp']`,
     `has_function_privilege('devagotchi_hub', p.oid, 'EXECUTE')`, and `NOT EXISTS (SELECT 1
     FROM aclexplode(COALESCE(proacl, acldefault('f', proowner))) x WHERE x.grantee = 0 AND
     x.privilege_type = 'EXECUTE')`. The `COALESCE` matters: a function never revoked has
     `proacl` NULL, which means PUBLIC EXECUTE, and `aclexplode(NULL)` yields no rows.
   - It replaces the hand-kept `TENANT_TABLES`.
2. **Prefix test on the catalog** (rules 7, 22): `every_table_in_the_hub_schema_carries_its_domain_prefix`
   lists `SELECT tablename::text FROM pg_tables WHERE schemaname = 'devagotchi'` on `admin()`.
   With 1(b), no ungranted or unprefixed table escapes.
3. **Migration file lint** (rules 2, 3), no Postgres, so it runs in every OS job:
   `every_migration_file_is_named_numbered_and_never_reused` in `engine/hub/src/store/tests.rs`
   and `agenthub/src/migrations/tests.rs`.
   - Read `concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")`; every name matches
     `^\d{4}_[a-z0-9_]+\.sql$`, versions are unique, and the file count equals
     `MIGRATIONS.iter().count()`.
   - The hub asserts no version is in `const RETIRED: [i64; 4] = [14, 15, 16, 18]`.
   - A new `migrations/checksums.txt` in each crate holds `<version> <hex checksum>` per
     migration and is compared in the same test. A line is only ever appended for a new
     migration; an existing line never changes.
4. **SQL source scan** (rules 12-18), a new `runtime/devagotchi/tests/sql.rs`, sharing
   `layout.rs`'s walker (G12). It walks `engine/hub/src` and `agenthub/src` and skips
   test files and `#[cfg(test)]` modules.
   - (a) A `devagotchi.<prefix>_<name>` inside a string literal that is not followed by `(`.
     Function calls such as `devagotchi.machine_pushes_of(` stay qualified (rule 18).
   - (b) `AssertSqlSafe(` outside an allow-list: `platform/store/src/lib.rs`,
     `platform/store/src/testing.rs`, `engine/hub/src/connections/stored.rs`,
     `engine/hub/src/machines/devices/mod.rs`, `agenthub/src/clickhouse.rs`.
   - (c) `QueryBuilder` anywhere while the store is Any.
   - (d) `.begin()` or `.finish(` on a `Store` outside `platform/store`. `pool()` and `admin()`
     are already compile-gated.
   - (e) Rule 14 ratchet: a SQL literal that names an RLS table and lacks `tenant_id` fails,
     except entries in an allow-list of today's identity sites that only shrinks, to 0 in B6.
5. **Test shapes** (rule 21): the `Vec`-of-stores, `if let Some(... postgres().await)` and bare
   `let ... else { return; }` shapes are to be failed by the skip-shape scan in a new
   `runtime/devagotchi/tests/test_shape.rs` (G13, B6), not here.

## Upstream

- sqlx `migrate!` (build.rs `rerun-if-changed`): https://docs.rs/sqlx/latest/sqlx/macro.migrate.html
- Migration file names `<VERSION>_<DESCRIPTION>.sql`: https://docs.rs/sqlx/latest/sqlx/migrate/trait.MigrationSource.html
- `Migrator` (checksums, `table_name`, `ignore_missing`, locking): https://docs.rs/sqlx/latest/sqlx/migrate/struct.Migrator.html
- Line endings and checksums; sequential versions supported: https://docs.rs/sqlx-core/0.9.0/src/sqlx_core/config/migrate.rs.html
- The Any driver (`install_default_drivers`): https://docs.rs/sqlx/0.9.0/sqlx/any/index.html
- Any's type set (no JSON, UUID, TIMESTAMPTZ; Bool is there): https://docs.rs/sqlx/latest/sqlx/any/enum.AnyTypeInfoKind.html
- `Arguments::format_placeholder` defaults to `?`; `AnyArguments` keeps it: https://docs.rs/sqlx-core/0.9.0/src/sqlx_core/arguments.rs.html and https://docs.rs/sqlx-core/0.9.0/src/sqlx_core/any/arguments.rs.html
- `QueryBuilder`: https://docs.rs/sqlx/latest/sqlx/struct.QueryBuilder.html
- `query!` and offline data: https://github.com/launchbadge/sqlx/blob/main/FAQ.md
- `AssertSqlSafe`: https://docs.rs/sqlx/0.9.0/sqlx/struct.AssertSqlSafe.html
- `#[sqlx::test]`: https://docs.rs/sqlx/0.9.0/sqlx/attr.test.html
- Row security (FORCE, USING, WITH CHECK, owners and BYPASSRLS): https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- Writing `SECURITY DEFINER` functions safely: https://www.postgresql.org/docs/current/sql-createfunction.html
- Default privileges (per role, per object class): https://www.postgresql.org/docs/current/sql-alterdefaultprivileges.html
- `set_config(..., is_local = true)` lasts to the end of the transaction: https://www.postgresql.org/docs/current/functions-admin.html
- Serializable retries (40001, unique violations under overlap): https://www.postgresql.org/docs/current/transaction-iso.html
- `information_schema.tables` shows only accessible tables: https://www.postgresql.org/docs/current/infoschema-tables.html

## Deviations from upstream, judged

| Deviation | Upstream | Verdict |
|---|---|---|
| Any driver on a Postgres-only store | concrete driver, `query!` checked at compile time, `.sqlx` offline data, `QueryBuilder` | No longer justified now that the SQLite twin is gone (ADR 0001, 0005). Moving to `PgPool` is a store-wide plan: it changes `transaction!`, `Failed` and every `*_in` signature, and it would unlock `QueryBuilder` and `query!`. Until that plan, rules 12-19 hold. |
| Own test harness, not `#[sqlx::test]` | `#[sqlx::test]` with `DATABASE_URL` | Justified: it needs two Migrators with separate histories and an AnyPool. `testing.rs` reproduces the per-test database and the sweep. |
| Tests skip without the URL | `#[sqlx::test]` requires `DATABASE_URL` | Justified for a laptop with no Postgres, only under G13: hub-postgres runs `rdeps(devagotchi-store)` with `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1`, so in CI a missing URL fails. In place since f13b8e1 (B0). |
| Sequential `NNNN` versions | timestamps recommended for parallel branches | Justified: sequential versions are supported, there is one writer per crate, and rule 2 covers rebases. |
| 0/1 flags, TEXT JSON, BIGINT times | `BOOLEAN`, `JSONB`, `TIMESTAMPTZ` | JSON and timestamps are forced by Any's type set. Bool would work; integers stay for consistency until the baseline. |
| AgentHub migrations hard-code `devagotchi.` | ADR 0005: the host gives the schema | Unresolved. It needs AgentHub's baseline (texas-baseline M5). |
| AgentHub migrations are idempotent (`IF NOT EXISTS`) | plain DDL | Justified for AgentHub alone: its 0001 adopts tables the hub's 0014-0018 made (`0001_boards.sql:1-4`), and `run_again_the_migrations_find_everything_made` proves a re-run. Hub migrations stay plain. |

## Known violations

Verified at main 2b3feec (AgentHub at 5c1499c). A path with no crate in front is under
`engine/hub/src/`; an AgentHub `src/` path is under `agenthub/`. B0 to B6 are the
rewrite plan's batches.

### Rule 3 and 11 (headers of applied migrations)

- `engine/hub/migrations/0001`-`0025` (each header has a "Kept in step with ../sqlite/..."
  line), and headers that name source files which have moved or gone: for example `0007` (lines
  13 and 155 name `store.rs`), `0009:1` (`src/analytics.rs`), `0021:1`
  (`accounts/api_keys.rs`), `0027:1` and `0027:7` (`engine/hub/src/audit.rs`, `audit.rs`),
  `0036:1` (`notify/signals/stored.rs`) -> leave them byte for byte; they go only with the
  baseline (texas-baseline M5). New headers follow rule 11.

### Rule 6 (schema)

- `agenthub/migrations/0001_boards.sql` (`CREATE SCHEMA IF NOT EXISTS devagotchi` and
  every `devagotchi.board_*`), `platform/store/src/lib.rs` (`TABLES_SCHEMA`): ADR 0005 says
  AgentHub's migrations name no schema -> do not edit applied files; in AgentHub's baseline,
  write unqualified DDL and make the tables' schema an `open_in` parameter that replaces
  `TABLES_SCHEMA`.

### Rules 8-10 and 23 (schema proofs)

- `engine/hub/src/identity/accounts/tenants/tests.rs` (`TENANT_TABLES`): lists 12 tables;
  10 forced-RLS hub tables and all 14 `board_*` tables are missing -> keep the behaviour test
  and add check 1, which derives the list from the catalog (B6).
- `engine/hub/src/store/tests.rs` (`every_table_in_the_hub_schema_carries_its_domain_prefix`):
  lists tables on `store.read()` (the hub's role) from `information_schema.tables`, which hides
  an ungranted table -> check 2: `pg_tables` on `admin()`, without the `_sqlx` filter (B6).

### Rule 14 (`tenant_id` in every statement on an RLS table)

- `engine/hub/src/identity/accounts/oauth/mod.rs` (lines 228, 282, 302, 369, 394, 409, 421,
  506, 522, 546, 623): `oauth_codes` and `oauth_tokens` statements have no tenant predicate;
  the sweeps at 228 and 546 clear only the scoped tenant's rows but do not say so -> add
  `tenant_id = $n` and bind the tenant, e.g. `DELETE FROM oauth_codes WHERE tenant_id = $1 AND
  expires_at <= $2` (B6).
- `engine/hub/src/identity/accounts/passkey/accounts.rs` (198, 207, 226, 231, 247, 304, 333,
  339, 356): `auth_sessions`, `auth_passkeys`, `auth_links` -> `DELETE FROM auth_passkeys WHERE
  tenant_id = $1 AND id = $2 AND account = $3` and the like, binding `account.tenant` (B6).
- `engine/hub/src/identity/accounts/passwords/mod.rs` (166, 205, 233, 322, 339, 376, 387, 418,
  434; also 246 and 250 in the `#[cfg(test)]` `set_password`): `auth_passwords`,
  `auth_identities`, `auth_sessions`, `auth_passkeys`, `auth_links` -> add `tenant_id = $n`
  with the tenant that `enter()` or `tenant_of` returned (B6).
- `engine/hub/src/identity/accounts/sessions/mod.rs` (111, 119, 169, 184, 205, 210):
  `auth_sessions` -> e.g. `DELETE FROM auth_sessions WHERE tenant_id = $3 AND (expires_at <= $1
  OR last_seen + $2 <= $1)`; `insert_session` already receives `tenant` (B6).
- `engine/hub/src/identity/accounts/identities.rs` (104, 266, 400, 407, 423),
  `identity/accounts/claim/mod.rs` (114, 215), `identity/accounts/links.rs` (118, 155),
  `identity/accounts/tenants/mod.rs` (199, 219), `identity/accounts/api_keys/mod.rs` (447):
  `auth_identities`, `auth_passwords`, `auth_accounts`, `auth_links`, `auth_api_keys` -> add
  `tenant_id = $n`; `touch` and `secure_in` hold `account.tenant`, `links::issue_in` has it as
  `holder.tenant`, and api_keys 447 runs after `api_key_tenant()` (B6).

### Rule 18 (tables unqualified)

- `engine/hub/src/kernel/stored.rs` (14 sites) and `engine/hub/src/kernel/authority.rs`
  (9 sites): tables named `devagotchi.kernel_lexicon_*` -> drop the `devagotchi.` on table
  names, e.g. `INSERT INTO kernel_lexicon_entries ...`; keep it on function calls (B6).

### Rule 19 (stored times)

- `engine/hub/src/connections/stored.rs` (`stored`, 35 and 38),
  `identity/accounts/api_keys/mod.rs` (190), `identity/accounts/sign_in_settings.rs` (62),
  `ops/analytics_settings.rs` (49), `web/sessions.rs` (181): row times decoded with
  `.max(0) as u64` -> `unsecs(row.try_get("created_at")?)`, and `.map(unsecs)` for an
  `Option` (B6).

### Rule 21 and G13 (fixtures and skips)

Fixed in f13b8e1 (B0): `server_url()`, the REQUIRE flag, the `SKIPPED:` text, and `store/src/tests.rs`.

- `.github/workflows/ci.yml` (job `hub-postgres`, lines 91 and 97) and
  `runtime/devagotchi/tests/hub_recovery.rs`
  (`hub_set_password_reads_stdin_keeps_the_owner_and_never_prints_the_password`): the job
  narrows `-p devagotchi` to `--test board`, so `hub_recovery` never runs with the URL; its
  note says `skipped:` -> one step, `cargo nextest run --locked --profile ci -E
  'rdeps(devagotchi-store)'` with both REQUIRE flags (B0); the test uses `or_skip_blocking!`
  (B6).
- `engine/hub/src/store/mod.rs` (`testing::stores`, 87-89) and
  `agenthub/src/tests/mod.rs` (`stores`, 116): `Vec`-returning fixtures -> delete;
  tests call `or_skip!(postgres())` (hub B6, AgentHub B3).
- Seven `Vec<(TestStore, Accounts)>` helpers: `identity/accounts/tests.rs` (`stores` 6,
  `hosted_stores` 785), `identity/accounts/oauth/tests.rs` (`stores` 14),
  `identity/accounts/claim/tests.rs` (`stores` 9), `identity/accounts/sessions/tests.rs`
  (`stores` 13), `identity/accounts/tenants/tests.rs` (`stores` 22),
  `tenancy/access/tests.rs` (`stores` 44, with `Arc<Accounts>`); and
  `identity/accounts/tenants/tests.rs` (`stores_from_before_tenants` 61) -> each returns
  `Option`, e.g. `async fn store() -> Option<(TestStore, Accounts)> { let test = postgres().await?;
  let accounts = Accounts::open(test.store.clone()).await.unwrap(); Some((test, accounts)) }`;
  `before_tenants() -> Option<(String, TempDir)>` (B6).
- Four `Vec<Option<String>>` helpers: `offers/tests.rs` (`stores` 22), `ops/tests.rs`
  (`stores` 20), `tenancy/tests/access.rs` (`stores` 69), `workflows/tests.rs` (`stores` 68)
  -> `let url = or_skip!(empty_postgres());` (B6).
- 103 `for ... in ...stores...` lines (96 in tests, 7 inside the helpers above) in
  `identity/accounts/tests.rs` (22), `identity/accounts/oauth/tests.rs` (11),
  `identity/accounts/tenants/tests.rs` (10), `workflows/tests.rs` (9),
  `identity/accounts/claim/tests.rs` (9), `identity/accounts/sessions/tests.rs` (8),
  `tenancy/orgs/tests.rs` (5), `tenancy/access/tests.rs` (5), `store/tests.rs` (4: 43, 53,
  92, 200), `ops/analytics/tests.rs` (4), `offers/tests.rs` (3),
  `tenancy/tests/access.rs` (2), `ops/tests.rs` (2), `web/operator_tests.rs` (1),
  `web/analytics_tests.rs` (1), `tenancy/operator/tests.rs` (1),
  `identity/accounts/passwords/tests.rs` (1), `agenthub/tests/hosted.rs` (1), and AgentHub's
  `src/tests/hosted.rs` (17, 294) and `src/tests/hosted_lifecycle.rs` (15, 152): each loop runs
  zero times without the URL -> `let (test, accounts) = or_skip!(store());` (or
  `let test = or_skip!(postgres());`), then the old body unindented (hub B6, AgentHub B3).
- `engine/hub/src/web/analytics_tests.rs`
  (`the_operator_sees_the_counts_and_the_funnel_move_and_nobody_else_sees_anything`, 50-58)
  and `engine/hub/src/web/operator_tests.rs`
  (`the_operator_lets_a_waiting_person_in_and_their_next_call_works`, 91-98): an inline
  `Vec<Option<String>>` looped with `or_skip!` inside -> `let url =
  or_skip!(crate::store::testing::empty_postgres()); dashboard(or_skip!(hub_on(Some(&url),
  None))).await;` (and the same with `operator_page`) (B6).
- `engine/hub/src/identity/accounts/api_keys/tests.rs` (`keys_hold_on_postgres`, 257-260) and
  `engine/hub/src/identity/preferences/tests.rs`
  (`preferences_hold_on_postgres_under_row_level_security`, 185-188): `if let Some(test) =
  testing::postgres().await` passes without the URL -> `let test =
  or_skip!(testing::postgres()); holds(test.store.clone()).await;`, and rename each as a claim
  (B6).
- `engine/hub/src/cli/tests.rs` (258) and `engine/hub/src/identity/cli/tests.rs` (66, 187):
  synchronous tests skip with `let Some(url) = runtime.block_on(empty_postgres()) else {
  return; };` -> `or_skip_blocking!` (B6).
- `engine/hub/src/store/tests.rs`
  (`without_a_url_there_is_no_store_and_the_configs_is_used_as_written`, 31-33): returns
  silently when `DEVAGOTCHI_HUB_STORAGE_URL` is set -> `url_for` takes the environment as
  `&dyn Fn(&str) -> Option<String>` (G8), so the test passes its own and never returns early
  (B6).

### Where files go

- `engine/hub/src/web/sessions.rs` (395 lines; inline `mod tests` at 273): it keeps
  `auth_web_sessions`, an identity table, from `web/`, and its tests run SQL as `admin()` ->
  move to `identity/web/sessions.rs` with its tests beside it, in S8 (waits on decision D6).
- `engine/hub/src/connections/mod.rs`, `offers/mod.rs`, `workflows/mod.rs`, `agenthub/mod.rs`,
  `store/mod.rs`: no `# Owns`, and no front door names `connector_keys`, `tenancy_offers`,
  `tenancy_workflows` or `tenancy_clients`; `engine/hub/src/tenancy/mod.rs:11-13` calls the
  organisations' tables `tenancy_*`, but they are `org_*` and `project_*` -> add `# Owns` to
  each, e.g. connections: "In the hub's Postgres: `connector_keys` (migration 0025, renamed by
  0029), under the tenant's row-level security (`stored.rs`)."; tenancy: "the organisations'
  `org_*` and `project_*` tables (`orgs`)" (B6, with S12's front-door test).
- `platform/store/src/lib.rs` (522 lines, `runtime/devagotchi/tests/layout/oversize.txt:55`): all
  the engine's logic and unrelated helpers (`write_private`, `random`) at the crate's top
  level -> S10 split into `engine.rs` (`open_in`, `SET ROLE`), `transaction.rs`
  (`transaction!`, `Failed`, `begin`, `finish`, `collided`), `tenancy.rs` (`scope`,
  `tenant_of`, `Lookup`, `tenants_in`, `live_unsecured`, `valid_tenant_id`) and the existing
  `settings.rs`; `lib.rs` keeps `mod` lines and `pub use`; lower the ratchet entry (B6).

### G11 (docs that name files)

- `.ai/patterns/layout.md:39-40`: says the prefix test is in `platform/store/src/tests.rs` ->
  `engine/hub/src/store/tests.rs`, plus "AgentHub checks its own tables in
  `agenthub/src/migrations/tests.rs`
  (`on_its_own_every_table_is_a_board_table_with_its_own_history`)." (B0).
- `engine/hub/README.md:503`, `tests/throwaway_postgres.sh:5`, `docs/security-baseline.md:50`:
  cite `platform/store/migrations/postgres/...` or `migrations/postgres/...`, which no longer
  exist; security-baseline also lists only 0007's eight RLS tables -> README: "The schema is
  migrated at start: the hub's tables from `engine/hub/migrations/`, then AgentHub's from
  `agenthub/migrations/` (history `_agenthub_migrations`)."; the script and
  security-baseline: `engine/hub/migrations/0007_tenant_rls.sql`, and "forced on every table
  holding a tenant's rows (22 hub, 14 board)" (B0).
