# Connectors: every major AI client, from one catalog

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-10: real connectors for the top 10-15 clients, defined by
  schemas and patterns, in context-aligned folders, to the standard of a platform team.
- PR: not_created
- Branch / baseline: `increment-124/connectors`, stacked on `increment-123/notes-that-reach-them`
- Objective: any supported AI client connects to a hub by following steps generated for that
  hub, and adding a client means adding one reviewed file.
- Non-goals: writing other clients' config files automatically beyond the three
  `devagotchi connect` already writes; provider CLI hooks; a marketplace listing.
- Canonical architecture / related plan: [pattern](../../patterns/connectors.md),
  [harness matrix](../../docs/adapters.md)

## Design

| Decision | Choice | Why |
|---|---|---|
| Source of truth | One JSON file per client, `crates/connectors/catalog/<vendor>/<id>.json` | Adding a client is a one-file review; no code path per client |
| Contract | Rust types, generating `schemas/connector.v1.schema.json` | The repository's rule: schemas come from canonical types and a test holds them equal |
| Boundary | New `devagotchi-connectors` crate: data, types, renderer; no network or disk | Hub, CLI and tests share one reader without depending on each other |
| Distribution | Compiled into the binary | A hub and its catalog can never disagree; nothing to fetch at runtime |
| Honesty | Every entry records its source, date and gaps; `null` means undocumented | Vendors change these; a stale fact must be visible as stale |
| Promotion | `docs` becomes `ci` only when a real client connects in a named test | Reading documentation is not proof that it works |

```text
crates/connectors/
  README.md                how to add a client
  src/lib.rs               types, catalog access, renderer
  src/catalog.rs           the compiled-in list, checked against the folder
  src/tests.rs             the catalog's rules, run as tests
  catalog/<vendor>/<id>.json
schemas/connector.v1.schema.json
crates/hub/src/web/connectors.rs   /connect and GET /api/v1/connectors
crates/mcp/src/connect.rs          devagotchi connect --list, ID --print
```

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Types, schema, renderer, catalog rules | none | `cargo test -p devagotchi-connectors` | done |
| M2 | 17 client files from vendor docs | M1 | Sources and date in each file | done |
| M3 | `/connect`, API, sitemap, landing link | M1 | Hub tests, rendered page | done |
| M4 | `devagotchi connect --list` and `ID --print` | M1 | CLI tests | done |
| M5 | Note aliases come from the catalog | M1, #170 | `inbox_tests.rs` | done |
| M6 | Move entries to `ci` as real clients are run | M2 | Named test per client | pending |
| M7 | Review and merge to main | M1-M5, operator approval | Merged PR | pending |

## Risks and approved decisions

Vendor documentation changes without notice, so entries carry dates and sources. Snippets are
shown to people and agents to run, so they are reviewed data compiled into the binary, never
fetched at runtime.

## Verification receipts

`cargo test --locked -p devagotchi-connectors -p devagotchi-mcp -p devagotchi-hub` on local Postgres.
`devagotchi connect --list` and `ID --print` run by hand. `/connect` at emulated iPhone width: 17 cards,
no sideways scroll, no console errors. One entry (`codex-cli`) is proven by a real client in CI; the other
16 come from vendor documentation read on 2026-11-10 and have never been connected from here.

## Delivery

Implementation: M1 to M5 done. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
