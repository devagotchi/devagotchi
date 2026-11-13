# Hub settings: every setting a guided page

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: Google sign-in configured from the admin panel and saved to
  the database; every configuration guided and grouped; cloud limited to what is truly cloud.
- PR: not_created
- Branch / baseline: `increment-126/hub-settings`, stacked on `increment-125/api-keys`
- Objective: a person running a hub never edits a file or environment variable to configure it.
- Non-goals: moving code to the private repository, which does not exist yet.
- Canonical architecture / related plan: [editions](../../docs/editions.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | `/admin`, owner only, grouped sections | none | `hub_settings_tests.rs` | done |
| M2 | Google and GitHub stored sealed in Postgres, live without restart | M1 | `hub_settings_tests.rs` | done |
| M3 | Email (SMTP) from the page, sealed | M2 | to write | pending |
| M4 | ClickHouse connection from the page, sealed | M2 | to write | pending |
| M5 | Signal sinks from the page, sealed | M2 | to write | pending |
| M6 | OpenID Connect providers from the page: Sign in with Shippin (Nomen) prefilled, any other by name, public clients | M2 | `hub_settings_tests.rs` | done (increment 134) |
| M7 | Network providers | operator's decision | to write | pending |
| M8 | Review and merge to main | operator approval | merged PR | pending |

## Risks and approved decisions

Client secrets are sealed under a key outside the database, so a copy of the database alone gives
nothing. Restoring needs the state volume too. ClickHouse never holds settings or secrets.

## Verification receipts

`cargo test -p devagotchi-hub` on local Postgres.

## Delivery

Implementation: M1 and M2. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
