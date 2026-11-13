# Structure cleanup: one boundary, one home, one test convention

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-13: everything grouped, nested and consistent in file and
  folder structure regardless of boundary; configurable unless best practice says hardcode;
  AgentHub is its own boundary. Operator chose "structure cleanup first", before integrations.
- PR: not_created (one PR per increment below)
- Branch / baseline: `increment-N/structure-<step>`, each on `main` at d4166f8 or later
- Objective: every domain has exactly one home, the hub crate's top level holds only wiring,
  AgentHub is a crate of its own, and a test keeps the layout from drifting back.
- Non-goals: behavior changes; the connectors/integrations rename (its own plan, below);
  the config sweep (its own plan); new features. Every increment here is moves and renames.
- Canonical architecture / related plan: [patterns](../../patterns/README.md),
  [agenthub-module](agenthub-module.md), [connectors](connectors.md)

## Measured baseline (main d4166f8, 2026-11-13)

- `crates/hub/src` top level: 49 `.rs` files; 13 of them `*_tests.rs` beside the code, while
  26 other tests live as `<domain>/tests.rs`: two conventions.
- Two homes with near-identical names for different domains: `account.rs` + `account/`
  (this machine's sign-in to a hub: `devagotchi login|logout|whoami`, client side) and
  `accounts.rs` + `accounts/` (the hub's own user accounts, server side).
- AgentHub lives inside the hub crate: `agenthub/mod.rs` 2,286 lines, `agenthub/mcp.rs` 3,714,
  `agenthub/cli.rs` 1,611. It reaches into `crate::{store, account, config, auth}`.
- 23 of 242 Rust files exceed 1,000 lines; 73 exceed 500.

## Rulings this plan records (operator, 2026-11-13)

1. Rename both directions: inbound AI clients (`crates/connectors`, `/connect`,
   `connector.v1` schema) become **clients**; outbound third-party services take the name
   **connectors**. Executed in its own plan after this one; recorded here so no step
   below creates new names that the rename would have to undo.
2. First outbound connectors, end to end: GitHub App, Vaultix, Lovable, Vercel; then Replit,
   Bolt, Emergent and WordPress. Lovable, Replit, Bolt and Emergent need their APIs/MCPs
   verified before they get catalog entries (connector pattern rule 4).
3. The GitHub App connector is one public app, installed once per GitHub account or org
   with "All repositories"; one Devagotchi account holds any number of installations.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| S0 | Write `.ai/patterns/layout.md`: crate per boundary; `src/<domain>/` per domain with `mod.rs` as the domain's front door; tests at `<domain>/tests.rs`; crate top level is wiring only; size cap 500 lines for new files. Add a ratchet test listing today's oversize files and their line counts: a listed file may shrink, never grow, and no new file may exceed the cap | none | pattern doc reviewed; ratchet test fails on a deliberately grown file, then passes | pending |
| S1 | One test convention: move the 13 `*_tests.rs` to `<domain>/tests.rs` | S0 | `git diff -M` shows renames only; nextest count unchanged | pending |
| S2 | Rename `account` → `login` (matches `devagotchi login`), leaving `accounts` as the hub's | S0 | renames only; e2e green | pending |
| S3a | Split `agenthub/mod.rs` by domain in place: `ticket/`, `deps/`, `notes/`, `settings/`, `store/` (SQLite), keeping public paths via `pub use` from `agenthub/mod.rs` only | S1 | no file over the ratchet; nextest green | pending |
| S3b | Split `agenthub/mcp.rs` by tool group (claim/lease, tickets, notes, settings, approvals, inbox) | S3a | tool list from `tools/list` byte-identical before/after | pending |
| S3c | Move the board domain to `crates/agenthub`; the hub keeps hosting (Postgres store, HTTP, MCP mount) and depends on it; `agenthub` depends on no hub module | S3a, S3b | `cargo tree` shows no hub → agenthub cycle; full pre-push suite | pending |
| S4 | Group the remaining hub top-level files, one domain per PR: `identity/` (auth, oauth, identity, nomen, sealed), `machines/` (devices, link, machine_update, join, downloads, files, run), `tenancy/` (orgs, tenant_access, tenant_machines), `notify/` (push, mail, signals), `ops/` (analytics, audit, warehouse) | S1 | each PR renames only; top level ends with `lib.rs`, `hub.rs`, `web.rs`, `store.rs`, `config.rs` and `mod` lines | pending |

## Risks and approved decisions

- Large moves conflict with any open branch touching the same files. Mitigation: one domain
  per PR, merged fast; check open PRs before each starts.
- `pub use` re-exports could hide the new structure. Allowed only in a domain's `mod.rs`,
  never a top-level barrel.
- No behavior, storage or privacy change in any increment; a PR that needs one stops and
  gets its own plan.

## Verification receipts

Per increment: `cargo build --locked --bins`, `cargo nextest run --locked`,
`python3 tests/e2e.py --bin-dir target/debug`, and the rename-only diff check. Record each.

## Delivery

Implementation: pending. Validation: pending. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
