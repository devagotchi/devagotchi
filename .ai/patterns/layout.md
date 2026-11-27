# Pattern: one boundary, one home

Where Rust code lives in this workspace. Follow it for every new file and every move.
The [structure cleanup plan](../dev/plans/structure-cleanup.md) brings the existing code to
it one step at a time; until a step lands, the code it covers stays where it is.

0. **A layer per kind of code, the same domain names in every layer.** The workspace's top
   level is its layers, nothing else (operator, 2026-11-25; [the program](../dev/plans/program.md)):

   | Layer | Holds | Crates today |
   |---|---|---|
   | `platform/` | what every part stands on: wire contracts, the Postgres store, local IPC | `contracts`, `store`, `ipc` |
   | `engine/` | what the product does, a folder per domain | `machine-engine`, `hub`, `kernel`, `connectors` |
   | `interfaces/` | how the world reaches the engine: MCP, HTTP, CLI | `mcp` |
   | `runtime/` | the wrapper binary and the sub-processes it starts | `devagotchi`, `link` |
   | `agenthub/` | AgentHub, its own repository, pulled in as a submodule | `agenthub` |
   | `apps/` | the desktop inspector, the website | `apps/desktop`, `apps/website` |
   | `devtool/` | the tools that build and check the workspace, never in the official build | `bebop` |

   The hub still holds both its engine and its interfaces per domain; they move into
   `interfaces/` domain by domain as the program ports each domain (program, step 8).
1. **A crate per boundary.** A boundary is something that can be built, tested and depended
   on without the rest: the broker, the link, the hub, AgentHub. A boundary never reaches
   into another crate's modules; it depends on that crate's public surface or on
   `platform/contracts`.
2. **A folder per domain.** Inside a crate, each domain is `src/<domain>/`, and
   `src/<domain>/mod.rs` is its front door: the module docs, the `mod` lines and the names
   the rest of the crate may use. A domain has exactly one home. No `<domain>.rs` beside a
   `<domain>/` folder, and no two homes whose names differ by a letter. Checked by bebop's
   `layout.module-home` rule (`.ai/rules/layout/module-home/`): the modules that were both when
   the check arrived are on its baseline, which only gets shorter.
3. **Tests sit with their domain.** `src/<domain>/tests.rs`, declared from the domain's
   `mod.rs` as `#[cfg(test)] mod tests;`. No `<domain>_tests.rs` at the crate's top level.
   Tests that drive a built binary stay in the crate's `tests/` folder. Checked by bebop's
   `layout.tests-with-their-domain` rule (no `*_tests.rs`), with a shrink-only baseline.
4. **The crate's top level is wiring only.** `lib.rs` or `main.rs`, the `mod` lines, and the
   few files that compose domains into the crate's entry points. Logic that belongs to one
   domain lives in that domain.
5. **Re-exports stay at the front door.** `pub use` belongs in a domain's `mod.rs`. There is
   no top-level barrel that re-exports several domains: a path says where the code lives.
6. **A new file holds at most 500 lines.** Past that, split by what the code is about, not
   by line count. Tests count: a long `tests.rs` becomes `tests/` with a file per subject.
7. **The front door describes the domain.** A domain's `mod.rs` module docs carry the same
   sections, in this order, so one file tells a person or an agent what the domain is:
   *Owns* (its tables, by prefix), *Serves* (its routes, MCP tools and pages), *Settings*
   (each with the page it is set on, or "deploy-time" for the few the hub needs to start),
   *Depends on* (the other domains and crates it uses). (Free edition plan, R0.)
8. **Tables carry their domain.** Every table in the hub's Postgres schema `devagotchi` starts
   with its domain's prefix: identity `auth_` `oauth_`; tenancy `tenancy_` `org_` `project_`;
   AgentHub `board_`; environment catalogs `catalog_`; shared PostgreSQL request receipts `store_`; hub settings and secrets `hub_`; machines `machine_`; notify `push_`
   `mail_` `signal_`; ops `audit_` `analytics_`; connectors `connector_`; kernel (Devagotchi's
   own AI: the lexicon) `kernel_`. Postgres only: no SQLite twin (ADR 0001). A new domain adds
   its prefix here and in the test.

## The table prefixes

Rule 8 is enforced by `every_table_in_the_hub_schema_carries_its_domain_prefix` in
`platform/store/src/tests.rs`: it migrates a new database on the test Postgres and lists the
schema's tables; every one carries a prefix (the five made before the rule were renamed by
migration 0029: `auth_passwords`, `auth_preferences`, `auth_api_keys`, `auth_sign_in_settings`,
`connector_keys`). It runs in CI's `hub-postgres` job.

## The rules that hold this page

The checks are bebop rules (`devtool/bebop/README.md`), each a folder under `.ai/rules/` with
its `rule.toml`, its shrink-only `baseline.txt` and its good and bad examples:

| Rule | Holds |
|---|---|
| `layout.file-size-cap` | rule 6, the size ratchet below |
| `layout.module-home` | rule 2: no `<x>.rs` beside `<x>/` |
| `layout.tests-with-their-domain` | rule 3: no `*_tests.rs` |
| `naming.layer-folders` | rule 0 and N4: the top level is the singular layers and the agreed folders |
| `naming.forbidden-folder-names`, `naming.folder-case` | N1 |
| `naming.crate-name-matches-folder` | N2 |
| `naming.domain-role-files` | N5, in the hub's domains |

`cargo run -p devagotchi-bebop -- check --all` runs them; CI runs them through
`the_workspace_keeps_every_enforced_rule` in `devtool/bebop/tests/workspace.rs`.

## The size ratchet

Rule 6 is enforced by bebop's `layout.file-size-cap`. It walks every `.rs` file in the layers,
`agenthub/` and `apps/` (skipping what `.gitignore` ignores and dot folders) and compares it
with `.ai/rules/layout/file-size-cap/baseline.txt`, the list of files that were already over
the cap when the rule arrived, each with its line count. Lines are counted as `wc -l` does.

- A listed file may shrink. It may never hold more lines than its entry.
- A file that is not listed may not exceed the cap.
- An entry whose file is gone fails: a move takes its entry along, a deletion removes it.

The list only gets shorter and its numbers only get smaller:

- **A file shrank.** Lower its number, or run `cargo run -p devagotchi-bebop -- baseline
  tighten`, which lowers every entry to today's count and drops entries now at or under the
  cap (and, for the other rules, entries that are fixed). It never adds an entry and never
  raises a number.
- **A file moved.** Change the path on its line by hand and keep the number, so the review
  shows the same count at the new path.
- **A file needs to grow, or a new file needs more than 500 lines.** Split it. Raising a
  number or adding a line to the list is a change to this pattern, agreed with the operator
  first, never a way to make the test pass.

What the rules do not check yet: rules 1, 4, 5 and 7 are held by review until bebop's later
phases (B2's code rules) give them rules of their own.
