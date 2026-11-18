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
| M3 | Email (SMTP) from the page, sealed | M2 | `web/hub_settings_mail_tests.rs` (checked before saving, sealed, live, a test reaches the stub inbox, off stops mail, the file's `auth.email` wins) | done (increment 199) |
| M4 | ClickHouse connection from the page, sealed | M2 | `hub_settings_tests.rs` | done (increment 138, [AgentHub crate](agenthub-crate.md) A4) |
| M5 | Signal sinks from the page, sealed | M2 | `web/hub_settings_sinks_tests.rs` (refusals save nothing; runs at once with its bearer token; sealed; audited without it; kept on an empty field; removed stops it; a config file's sink is shown and its name refused) | done (increment 200) |
| M6 | OpenID Connect from the page. The card does not prefill an issuer or a label; the operator types both. Any provider, including Nomen, is that | M2 | `hub_settings_tests.rs` (empty issuer, a typed one saves and appears on the sign-in page); `provider_tests.rs` (the code exchange) | done (increment 134; the prefilled issuer was removed so a self-hosted hub does not name one) |
| M7 | Network providers | operator's decision | to write | pending |
| M9 | The settings the hub reads while it runs, on the page and live: tenancy limits, anonymous connect, shortest password, machine auto-update, workflow link lifetime, visit retention; the file's value wins and is shown; every other key listed as set at deploy, with why; a test fails on a key in neither (free-edition U0) | M1 | `config/live/tests.rs`, `web/hub_settings_live_tests.rs` | done (increment 198) |
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
