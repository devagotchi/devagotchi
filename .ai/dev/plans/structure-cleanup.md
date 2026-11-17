# Structure cleanup: one boundary, one home, one test convention

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-13: everything grouped, nested and consistent in file and
  folder structure regardless of boundary; configurable unless best practice says hardcode;
  AgentHub is its own boundary. Operator chose "structure cleanup first", before integrations.
- PR: one per increment below. S0: [#185](https://github.com/devagotchi/devagotchi/pull/185), open.
  The map and steps S5 to S12: `increment-176/structure-map`. S5: `increment-177/structure-folders`
- Branch / baseline: `increment-N/structure-<step>`, each on `main` at d4166f8 or later.
  S0: `increment-138/structure-layout`
- Objective: every domain has exactly one home, the hub crate's top level holds only wiring,
  AgentHub is a crate of its own, and a test keeps the layout from drifting back.
- Non-goals: behavior changes; the connectors/integrations rename (its own plan, below);
  the config sweep (its own plan); new features. Every increment here is moves and renames.
- File-by-file map: [structure-cleanup-map](structure-cleanup-map.md), every Rust file, where it
  goes, which step takes it there and the one job it owns.
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

## Re-measured (main 5104f35, 2026-11-16)

288 Rust files in `crates/` and `apps/` (AgentHub's are in its own repo since #184). Against
[layout](../../patterns/layout.md):

- Rule 2, a domain file beside its own folder: 28 files. In the hub: `config.rs`, `site.rs`,
  `store.rs`, `agenthub/issue_lifecycle.rs`, `mcp/callers.rs`, `web/landing.rs` and seven in
  `identity/accounts/` (api_keys, claim, oauth, passkey, passwords, sessions, tenants). The rest
  are in machine-engine (9), mcp (3), link (1), ipc (1) and contracts (1).
- Rule 3, `*_tests.rs` beside the code: 15 files, 11 of them in the hub's `web/`, one each in
  `config/`, `site/`, link and contracts. Open #221 adds two more in `web/`.
- Rule 4, the hub's `lib.rs` is not wiring only: 1,942 lines holding the command line, the
  server, bearer checks, the link upgrade and `/healthz`.
- Rule 6: 67 files are on the oversize list (7 of them AgentHub's); 20 hold more than 1,000 lines.
- Rule 7: 6 of the hub's 12 domain front doors carry the Owns / Serves / Settings / Depends on
  sections (identity, machines, mcp, notify, ops, tenancy).
- Co-location: the hub's `web/` holds 53 files, the pages of every domain side by side
  (passkeys, orgs, approvals, workflows, analytics, the landing page). A change to passkeys
  touches `identity/accounts/passkey.rs` and `web/passkeys.rs`, two trees apart.

The hub's top level, now and when S5 to S9 are done:

```text
crates/hub/src (now)                     crates/hub/src (target)
lib.rs        1942  CLI, serve, auth...  lib.rs           mod lines, cli(), board_cli(), serve_with()
config.rs     1544  + config/            config/          mod.rs (HubConfig, load) + a file per section
site.rs        717  + site/              site/            mod.rs, model/ split by JSON file, web/
store.rs        94  + store/             store/           mod.rs, tests.rs
extensions.rs       edition hooks        extensions.rs    stays: wiring, what an edition adds
test_keys.rs        test RSA keys        test_keys.rs     stays: test wiring
                                         cli/             USAGE, Command, parse, manage  (from lib.rs)
                                         serve/           AppState, run, app router      (from lib.rs)
                                         settings/        hub settings page (hub_)       (from web/)
agenthub/                                agenthub/        + web/ board, ticket, approvals; mcp/ by tool group
connections/                             connections/     + web.rs
identity/                                identity/        + bearer.rs, web/ (sign-in, passkeys, oauth, device,
                                                            claim, email, keys, me...), connect/web.rs
login/        mod.rs 1153                login/           mod.rs, stored.rs, issuer.rs, hub.rs
machines/                                machines/        + link/http.rs, link/admit.rs, link/frames.rs, web/
mcp/                                     mcp/             callers/, surface/ (Hub, shapes, machine tools)
notify/                                  notify/          + web.rs
offers/                                  offers/          + web.rs
ops/                                     ops/             + health.rs, web/ (analytics, counted)
tenancy/                                 tenancy/         + web/ (orgs/, operator, hosted); orgs/ split
web/          53 files, all domains      web/             the shell: mod.rs, pages.rs, routes.rs,
                                                            app/{mod,home,tests}.rs, tests/mod.rs
workflows/                               workflows/       + web/ (page, cookies, actions)
```

### Ruling proposed (one decision, for the operator)

Each domain serves its own pages from `<domain>/web/` (or `<domain>/web.rs` when it is one
file), and `crate::web` keeps only the shell: layout helpers, the HTML kit, the router that
merges each domain's `routes()`, the phone app's frame and the whole-router test harness. This
is what rule 7 already asks of a front door ("Serves: its routes, MCP tools and pages") and
keeps a domain's store, pages and tests in one folder. The alternative is to keep `web/` and
nest it by domain (`web/identity/...`); it fixes the flat folder but leaves every domain in two
trees. S8 waits for this ruling; S5 to S7 do not depend on it.

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
| S0 | Write `.ai/patterns/layout.md`: crate per boundary; `src/<domain>/` per domain with `mod.rs` as the domain's front door; tests at `<domain>/tests.rs`; crate top level is wiring only; size cap 500 lines for new files. Add a ratchet test listing today's oversize files and their line counts: a listed file may shrink, never grow, and no new file may exceed the cap | none | pattern doc reviewed; ratchet test fails on a deliberately grown file, then passes | implemented in #185; awaiting operator review |
| S1 | One test convention: move the 13 `*_tests.rs` to `<domain>/tests.rs` | S0 | `git diff -M` shows renames only; nextest count unchanged | done for the hub's top level in increment 157: each domain's tests sit in it (`agenthub/tests/` with ahead, hosted, notes, served and settings; `connections/tests.rs`, `offers/tests.rs`, `workflows/tests.rs`, `store/tests.rs`; the settings page's beside it in `web/`), and `account`, `offers`, `workflows` and `connect` (now `identity/connect/`) have `mod.rs` front doors. Increment 158: `mcp/` (`hub.rs` as `mcp/surface.rs`, its front door naming `Hub`, `Machine` and `MachineUpdate`; `output_schema.rs`; `plugin_tests.rs` as `mcp/tests.rs`), so the hub's top level is `lib.rs`, `config.rs`, `store.rs`, `site.rs`, `extensions.rs` and `test_keys.rs` beside the domain folders. Left: `site.rs` keeps its file (its `include_str!` paths are relative to it); `web/`'s own `*_tests.rs` |
| S2 | Rename `account` → `login` (matches `devagotchi login`), leaving `accounts` as the hub's | S0 | renames only; e2e green | done in increment 159: `crates/hub/src/login/` (this machine's sign-in to a hub: `devagotchi login`, `logout`, `whoami`, and the client AgentHub's command line calls through); the hub's own accounts are `identity::accounts` |
| S3a | Split `agenthub/mod.rs` by domain in place: `ticket/`, `deps/`, `notes/`, `settings/`, `store/` (SQLite), keeping public paths via `pub use` from `agenthub/mod.rs` only | S1 | no file over the ratchet; nextest green | pending |
| S3b | Split `agenthub/mcp.rs` by tool group (claim/lease, tickets, notes, settings, approvals, inbox) | S3a | tool list from `tools/list` byte-identical before/after | pending |
| S3c | Move the board domain to `crates/agenthub`; the hub keeps hosting (Postgres store, HTTP, MCP mount) and depends on it; `agenthub` depends on no hub module | S3a, S3b | `cargo tree` shows no hub → agenthub cycle; full pre-push suite | pending |
| S4 | Group the remaining hub top-level files, one domain per PR: `identity/` (auth, oauth, identity, nomen, sealed), `machines/` (devices, link, machine_update, join, downloads, files, run), `tenancy/` (orgs, tenant_access, tenant_machines), `notify/` (push, mail, signals), `ops/` (analytics, audit, warehouse) | S1 | each PR renames only; top level ends with `lib.rs`, `hub.rs`, `web.rs`, `store.rs`, `config.rs` and `mod` lines | `machines/` done in increment 146 (devices, link, machine_update as `update`, join as `join/mod.rs` with its scripts, downloads, files, run; renames and paths only, 536 hub tests before and after). `ops/` done in increment 152 (#200: audit, analytics, warehouse); `notify/` in increment 153 (#201: push with the push subscriptions, mail, signals). Increment 154: `accounts/` sheds what is not identity, each into its domain as `<domain>/stored.rs` (the domain's rows, kept as methods of the account store): `analytics_settings` to `ops/`, `connections`, `offers` and `workflows` to theirs; Increment 155: `tenancy/` (orgs, `tenant_access` as `gate`, `tenant_machines` as `machines`, `accounts/access` with its plans split out, `accounts/operator`, and the tenant and org tests as `tenancy/tests/`), which R4 lifts into its own crate; `accounts/tenants` (the home tenant and the row-level security scope every hub has) stays with the accounts. Increment 156: `identity/` (`auth`, `nomen`, `sealed`, `preferences` and `accounts/` with identities, email, passwords, passkey, sessions, oauth, clients, api_keys, sign_in_settings, claim and tenants; the provider, key and person-page tests as `identity/tests/`), its front door naming `Subject`, `Sealer` and `PreferenceStore` for the rest of the hub; `access_tests` joined `tenancy/tests/`. Left at the top level: `account` (S2 renames it `login`), `connect`, `connections` (the connectors rename), `offers`, `workflows`, `site`, `extensions`, `output_schema`, and the `*_tests.rs` of agenthub, connections, offers, workflows, plugins, the store and the settings page (S1). Order changed by the operator's free-edition plan (2026-11-14): S4 goes domain by domain ahead of S1, and each domain's `*_tests.rs` moves with it |
| S3a, S3b | As planned above, now inside the hub's hosting of AgentHub: `agenthub/mcp/mod.rs` (2,998 lines) splits by tool group | S5 | tool list byte-identical | S3c done by #184 (board in `crates/agenthub`, store in `crates/store`); S3a moves to AgentHub's repo (S11); S3b is in S9 |
| S5 | Rule 2 in the hub where no open PR reaches: `config`, `store`, `agenthub/issue_lifecycle`, `mcp/callers` and seven in `identity/accounts/` become their folder's `mod.rs` | S0 | renames only (`git diff -M` 100%), oversize paths moved with their numbers, hub tests unchanged | increment 177 |
| S6 | The hub's `lib.rs` to wiring only: `cli/`, `serve/`, `identity/bearer.rs`, `machines/link/http.rs`, `ops/health.rs`, `operator_event` to `ops/audit.rs`, its tests with each (map row `lib.rs`) | S5 | `lib.rs` under 100 lines; `devagotchi hub --help` and `/healthz` byte-identical; nextest count unchanged | pending |
| S7 | `config/mod.rs` split by section (auth, storage, site, boards, signals, web; brokers and grants into `machines.rs`), each section validating itself; `hub_tests.rs` and the inline tests to `config/tests/` | S5 | every config fixture parses and refuses as before; nextest count unchanged | pending |
| S8 | Each domain serves its own pages (ruling above), one domain per PR: identity, tenancy and settings, agenthub, machines with ops, notify, connections, offers and workflows, then site after #221 | ruling, S6 | renames plus the router's `mod`/`merge` lines; e2e and browser suites green; `web/` ends with the shell's 7 files | pending, waits for the ruling |
| S9 | Files over 1,000 lines split by subject (map rows marked S9): `site/model.rs`, `mcp/surface.rs`, `agenthub/mcp/mod.rs`, `machines/link/mod.rs`, `identity/accounts/mod.rs`, `tenancy/orgs/mod.rs`, `login/mod.rs`, and four test files over 800 lines to `tests/` folders; `site.rs` folds after #221 | S5 | each oversize entry removed or lowered, never raised; nextest count unchanged | pending |
| S10 | The other crates: rule 2 and 3 in machine-engine (after #222), mcp, link, ipc and contracts; `broker.rs`, `ai/mod.rs`, `connect.rs`, `windows/security.rs` and the store's `lib.rs` split (map) | S0 | renames plus splits; Windows CI green for ipc and machine-engine | pending |
| S11 | AgentHub's repo: delete the SQLite board in `lib.rs` (ADR 0005, Postgres everywhere), fold `hosted.rs` into `hosted/`, split `cli/mod.rs`; its own plan in that repo | S0 | AgentHub's `test (postgres)` green; no `rusqlite` in its `Cargo.toml` | pending |
| S12 | Rule 7 on every front door: agenthub, connections, login, offers, workflows, config, store, site and web gain Owns / Serves / Settings / Depends on; then a test that checks the sections, so rule 7 is held by CI, not review | S8 | the new test fails on a front door missing a section | pending |

## Risks and approved decisions

- Large moves conflict with any open branch touching the same files. Mitigation: one domain
  per PR, merged fast; check open PRs before each starts.
- `pub use` re-exports could hide the new structure. Allowed only in a domain's `mod.rs`,
  never a top-level barrel.
- No behavior, storage or privacy change in any increment; a PR that needs one stops and
  gets its own plan. That covers the no-legacy schema work (one baseline migration in place of
  0001 to 0030, so the 0029 and 0030 renames and the stale SQLite comments go; the migration
  history tables into the `devagotchi` schema): it changes every existing hub's database, the
  hosted one included, so it is its own plan, not a structure step.
- Open PRs that touch mapped files, 2026-11-16: #221 (site and the landing page; S8's site PR
  and S9's `site.rs` wait for it, and its two new `*_tests.rs` go to `site/web/tests/`), #222
  (machine-engine `execution`; S10's machine-engine PR waits for it).
- Open, for the operator: #184 (`increment-137/agenthub-crate`, plan
  [agenthub-crate](agenthub-crate.md)) merged to `main` at 332bf25 on 2026-11-13, while
  S0 was in review. It did S3c ahead of S3a and S3b: the board is in `crates/agenthub` and
  the store in `crates/store`. It also went against S1's convention twice
  (`agenthub/hosted/tests.rs` became `agenthub/hosted_tests.rs`, `store/tests.rs` became
  `store_tests.rs`), so the hub's top level now has 50 `.rs` files, 14 of them `*_tests.rs`.
  The baseline section above and the S1 and S3 rows describe d4166f8 and need reconciling
  with that plan before S1 starts. S0 merged `main` and re-measured the ratchet list there.
- S0 choices awaiting review: a listed file that is gone fails the ratchet (a move edits its
  line in the list, so later moves are renames plus the list); the cap covers every `.rs`
  file under `crates/` and `apps/`, tests included.

## Verification receipts

Per increment: `cargo build --locked --bins`, `cargo nextest run --locked`,
`python3 tests/e2e.py --bin-dir target/debug`, and the rename-only diff check. Record each.

### S0 (#185, Linux x86_64, rustc 1.99.1, 2026-11-13)

Run on candidate 22999dd, on `main` d4166f8:

| Gate | Result |
|---|---|
| `cargo build --locked --bins` | finished; 21 warnings in `devagotchi-hub`, none new |
| `cargo nextest run --locked` (0.9.152) | 675 passed, 0 skipped; 7 are the new `layout` tests |
| `cargo nextest run --locked -p devagotchi-hub`, `DEVAGOTCHI_TEST_POSTGRES_URL` set (throwaway Postgres 16) | 532 passed |
| `cargo deny --all-features check` (0.20.2) | advisories, bans, licenses, sources ok |
| `python3 tests/e2e.py --bin-dir target/debug` (throwaway Postgres 16) | E2E PASSED |
| `tests/security_boundary.py`, `tests/mcp_protocol.py`, `tests/mcp_smoke.py` | OK, exit 0, exit 0 |
| `node scripts/generate-contracts.cjs --check` | current |
| `devagotchi ai check` | contracts and capture schema agree |
| Ratchet, one line appended to `crates/hub/src/agenthub/mcp.rs` | fails: "grew from 3714 to 3715 lines" |
| Ratchet, new 501-line `crates/hub/src/zz_new.rs` | fails: "501 lines, over the cap of 500" |
| Ratchet, both reverted | passes; `DEVAGOTCHI_WRITE_LAYOUT=1` changes nothing |
| Diff | additions only: 4 new files and 2 index lines; no `.rs` file moved or edited |

After #184 merged, CI's `core` job failed on the merge with `main`: the ratchet named 14
files, the ones #184 moved, added or grew. The fix merged `main` 332bf25 into the branch
and re-measured the list there: 72 files over the cap (23 over 1,000) of 250. Against the
d4166f8 list: 9 entries left (moved, or now under the cap), 8 arrived (7 in
`crates/agenthub` and `crates/store`, and `hosted_tests.rs`), 5 are lower, and
`crates/hub/src/lib.rs` went from 2,714 to 2,716, which #184 added before the rule existed. On the merged tree the `layout`
tests pass (7 of 7); the rest of the suite on the merge is CI's run.

Not run locally: the real-ClickHouse export test, Windows, the browser suites (CI). The
pattern doc review is the operator's and is pending.

## Delivery

S0: implementation done; validation passed locally (receipts above), CI and operator review
pending; pushed to `increment-138/structure-layout`; PR #185 open; main: not merged.
S1 to S4: as the table says. S5: increment 177, draft PR. S6 to S12: pending, PR not_created.
**DELIVERY INCOMPLETE: NOT MERGED TO MAIN**
