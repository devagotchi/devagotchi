# A hub keeps everything in Postgres: no SQLite in `hub serve`

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha, pre-release)
- Issue / request: operator, 2026-11-14: "WHY IS SQLITE EVEN STILL IN THIS APP ... IS IT
  PROPERLY SETUP TO WORK WITH POSTGRES?"; "get it done canonically consistent colocated code";
  "I AM IN PRE-RELEASE I DO NOT GIVE A FUCK ABOUT PREVIOUS DATA ... NO COMPATIBILITY NO
  LEGACY". ADR [0001](../../../docs/adr/0001-hosted-board-postgres-and-clickhouse.md) and
  [0002](../../../docs/adr/0002-bound-board-offline-cache-and-queued-drafts.md) (2026-11-09).
- PR: one per milestone
- Branch / baseline: from `5c28f8c` (v0.1.0)
- Objective: a running hub stores nothing in SQLite. Every record it keeps is in its Postgres,
  under the tenant's row-level security where it belongs to a tenant, shared by every hub
  instance, in `pg_dump`. SQLite stays only where ADR 0002 puts it: standalone local boards, a
  bound client's offline cache and pending queue, and each machine's own job journal.
- Non-goals: imports, compatibility shims, keeping old files or modes. Pre-release: existing
  SQLite data, the live test hub's included, is dropped. Keys kept as files in the state
  directory (`settings.key`, `session.key`, `vapid.pk8`, link keys) stay files.
- Canonical architecture / related plan: ADR 0001, ADR 0002;
  [self-hosted-and-provenance](self-hosted-and-provenance.md) P3 waits for M1.

## What is true today (each checked on `5c28f8c`, 2026-11-14)

| Claim | How it was checked |
|---|---|
| `hub serve` refuses to start without Postgres | `crates/hub/src/lib.rs:859` |
| Postgres holds accounts, tenants, identities, passwords, OAuth clients/codes/tokens, boards, orgs, API keys, settings, connections | `crates/store/migrations/postgres/0001`-`0025` |
| The same running hub (`auth.mode hub`) also writes four SQLite files | `devices.sqlite3` (`lib.rs:524`: machines, join codes, push subscriptions); `audit.sqlite3` (`lib.rs:891`: the audit chain); `sessions.sqlite3` (`web/sessions.rs:26`: browser sessions); `signins.sqlite3` (`auth.rs:203`: Nomen sign-ins) |
| A second account system, `auth.mode devagotchi`, is SQLite only | `identity.rs` (`identities.sqlite3`, 15 tables), `oauth.rs` (`oauth.sqlite3`, 9 tables); `docs/auth.md:7` |
| The store crate has a SQLite backend and a SQLite twin of all 25 migrations, and falls back to `accounts.sqlite3` | `crates/store/src/lib.rs:180-200`; `migrations/sqlite/`; `Backend::Sqlite` branches in `api_keys.rs`, `orgs.rs`, `preferences.rs`, `web/hub_settings.rs` |
| A plan still says "SQLite for a standalone one; every test on both" | `docs/product-and-platform-plan.md:424` (2026-11-01, before ADR 0001) |
| Table names are inconsistent | 0001-0012, 0014-0019 use domain prefixes; 0013 `passwords`, 0020-0025 do not |

## Rules

1. One place per kind of record: `crates/store/migrations/postgres/NNNN_<domain>.sql` and one
   module `crates/hub/src/accounts/<domain>.rs`. The old file (`devices.rs`, `audit.rs`, ...)
   is deleted or becomes that module; no second implementation remains.
2. Table names carry their domain (`machine_`, `audit_`, `auth_`, `hub_`), schema
   `devagotchi`; `tenant_id` with forced RLS for tenant records.
3. No SQLite twin for any new migration.
4. No import, no fallback, no old mode kept. Delete, don't bridge.
5. Each milestone adds a guard test that fails on `5c28f8c` and passes after it.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Machines: `machine_devices`, `machine_enroll_codes`, `machine_push_subscriptions`; `accounts/machines.rs`; `devices.rs` deleted; callers async | none | hub tests on Postgres; guard: a hub that joins a machine and subscribes a phone has no `devices.sqlite3` | not started |
| M2 | Audit: `audit_entries` (append-only for `devagotchi_hub`), chain appended under `pg_advisory_xact_lock`; `audit.rs` over Postgres; `/healthz` reads it | none | two-pool concurrent append keeps one verifying chain; guard: no `audit.sqlite3` | not started |
| M3 | Browser sessions: `auth_web_sessions`; Nomen tokens stay sealed under `session.key` | none | session survives a restart; guard | not started |
| M4 | Nomen sign-ins: `auth_nomen_confirmed` | none | guard | not started |
| M5 | Delete `auth.mode devagotchi`: `identity.rs`, `oauth.rs`, their files, config variant, tests, docs. `auth.mode hub` is the one account system | none | build; hub tests; `docs/auth.md` describes one system | not started |
| M6 | Store crate Postgres only: `Backend::Sqlite`, `sqlite_url`, the `accounts.sqlite3` fallback, `migrations/sqlite/` and every `Backend::Sqlite` branch deleted; tests run on Postgres | M5 | `cargo test -p devagotchi-store -p devagotchi-hub` on Postgres; `grep -rln rusqlite crates/hub crates/store` empty | not started |
| M7 | Names: `passwords`, `person_preferences`, `api_keys`, `sign_in_settings`, `analytics_settings`, `connections` renamed with their domain prefix | M6 | hub tests | not started |
| M8 | Docs agree: `product-and-platform-plan.md:424` superseded by ADR 0001; READMEs list the state directory as keys only; `docs/auth.md` | M6 | doc diff | not started |
| G | Guard over all: `hub serve` with every feature exercised leaves no `*.sqlite3` in its state directory | M1-M5 | the test, in CI | not started |

## Risks and approved decisions

- Approved: ADR 0001/0002 and the operator's words above (no data kept, no legacy).
- `devices.rs` and `audit.rs` are synchronous and called from about 48 places; the store is
  async. M1 and M2 move those callers to async.
- The live test hub loses its machines, push subscriptions, audit rows and browser sessions
  when M1-M4 deploy: machines join again, phones turn notifications on again.

## Verification receipts

None yet. Each milestone records the guard test, its failure on `5c28f8c`, its pass on the PR
head, and the CI run.

## Delivery

Implementation: not started. Validation: not started. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT STARTED**
