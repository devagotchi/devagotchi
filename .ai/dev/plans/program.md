# The program: one structure, the operator's code absorbed into it, then the UI

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Request: the operator, 2026-11-24 and 2026-11-25. All existing code has to be properly
  organized and built for scale, following and extending his earlier instructions, kept
  ongoing and consistent. His code is absorbed first. The work goes in complete batches, not
  tiny PRs. The code is mind-mappable, with boring, predictable patterns.
- PR: this page lands with step 2 (the layers)
- Branch / baseline: `increment-297/layered-structure`, from main `7eda906`
- Objective: one living plan that holds every operator instruction, the target structure,
  the order of the work and its status. A new instruction is added here, so the work stays
  consistent across sessions.
- Non-goals: the cloud edition (it waits until this program's alpha is accepted).
- Authority: [.ai/operator-rules.md](../../operator-rules.md) holds over this page and every
  other file. This page applies it and never edits it.

## 1. The operator's instructions, gathered

The operator rules page (2026-11-23) is the authority. The statements below were made since
then and are recorded here until he moves them into it.

- **Absorb before anything new** (2026-11-24, 2026-11-25). Existing authored work comes in
  first: open PRs, branches, local copies and the preserved sources (§4). Do it in complete
  batches. No new authoring while his work waits.
- **Absorption route: B, into Rust** (2026-11-25). The Go AgentHub (`agent-hub/server`, 30
  packages, about 14,600 non-test lines) is re-authored in Rust by domain, into the structure
  in §2.
- **AgentHub** (2026-11-24). It is its own repository under the devagotchi organisation,
  pulled into Devagotchi as the Rust version. It has a release tailored to the self-hosted,
  stand-alone Devagotchi. It shares the hub's Postgres through its own prefixed tables
  (`board_`).
- **Runtime** (2026-11-24). It is not one binary of everything. It is properly separated
  sub-processes, bundled at the end into one binary that wraps them, for clean deployment
  in Docker, containers and VMs.
- **Structure** (2026-11-24, 2026-11-25). The file structure is clear and orthogonal and
  follows industry practice (for example `interfaces/`, `engine/`). It is never flat, so the
  UI can derive its tabs and grouping from files and folders.
- **UI** (2026-11-23, 2026-11-24, 2026-11-25).
  - Svelte drives it, and **all existing UI converts to Svelte** (decided 2026-11-25): the
    hub's server-rendered pages, the original React AgentHub UI, the website and the desktop
    inspector. The pattern is [ui.md](../../patterns/ui.md). Style Dictionary generates all
    CSS: semantic and composite tokens on
    ColdLight, his own component system.
  - Primitives come first; blocks, templates and patterns are made from them. Variants
    live with the code, and every component has a Storybook story.
  - The UI is dumb: it parallels the engine and only drives it.
  - The current hand-written CSS is replaced, not extended.
- **Experience** (2026-11-25). The UI is for people, not a developer's console.
  - **Advanced rendering for easy visuals.** Information is rendered so it is easy to see
    and take in.
  - **The conflict popup.** Where a change conflicts (a file changed since it was read, an
    import that collides), a friendly popup offers incoming, existing, or merge both. It
    previews the change in clear red and green.
  - **Nothing hardcoded.** Everything is configurable.
  - **Hubs built for scale.** The UI drives the complex work behind it, with clear logging
    and auditing.
  - **Constructive failures.** A failure explains itself and tries to put things right.
  - **Versions managed by code.** Versions are consistent and set in code, so a solo
    developer can maintain them.
  - **Friendly dashboards.**
- **The doorway** (2026-11-20).
  - The hub's entrance and sign-in are always available, and the basics harden them: IP
    rules, blocking countries.
  - Past sign-in, the solo developer properly connects their existing Tailscale or
    Cloudflare Tunnels. Wardenclyffe is offered too.
  - These are integrations, not the way into the hub.
- **CI** (2026-11-24). Cut back until there is a proper alpha (done, #290).
- **Pre-release** (2026-11-25). There is no release yet. Compatibility layers or version
  negotiation are added only for a reason.
- **Scope of the first release.** Everything Desktop Commander offers, done better and with
  its users' complaints resolved. AgentHub as a real software factory. The plugins (Claude
  Code, Codex, custom and open clients). Then the commercial Shippin project takes
  Devagotchi as the entrance to a Lovable-like devspace.

## 2. The target structure

**Layers (layout rule 0).**
- `platform/` holds `contracts`, `store` and `ipc`.
- `engine/` holds `machine-engine`, `hub`, `kernel` and `connectors`.
- `interfaces/` holds `mcp`.
- `runtime/` holds `devagotchi`, the wrapper, and `link`.
- `agenthub/` is the submodule.
- `apps/` holds the desktop app and the website.
- `ui/` is the Svelte app, added in step 10.

**Every domain folder has the same files, in every crate.** Only the roles a domain has
exist, and none is ever named differently:

```text
<domain>/
  mod.rs        the front door: Owns / Serves / Settings / Depends on, the mod lines, the names others use
  model.rs      its types and its rules, no IO
  store.rs      its Postgres (or journal) access, its tables by prefix
  service.rs    its operations: the one owner every interface calls (G1)
  mcp.rs        its MCP tools
  http.rs       its JSON API, which the UI reads
  web/          its server-rendered pages, until the Svelte UI replaces them
  cli.rs        its command-line verbs
  tests/        a file per subject
```

**The UI mirrors the domains.** `ui/src/routes/(app)/<domain>/<tab>/`: a folder per domain is
a navigation group, and a subfolder is a tab. Each domain's `meta.ts` gives its label, order
and icon, and the navigation is built from those files (`import.meta.glob`). Nothing is
wired by hand. ColdLight lives in `ui/src/lib/coldlight/`: Style Dictionary tokens,
primitives, blocks and patterns, and their Storybook stories.

**Held by checks.** `runtime/devagotchi/tests/layout.rs` holds the layers and the domain
files, so a domain missing a role it claims, or a role file under another name, fails. A
later test checks that every domain exists consistently in `engine/`, `interfaces/` and `ui/`.

## 3. The order, and where it stands

| Step | What | Done when | State |
|---|---|---|---|
| 1 | Absorb every open PR and branch | GitHub has only main and no open PR | done 2026-11-25: #284 to #291 merged; #291 (file access) with its review fixes |
| 2 | The layers: every crate moved, every path and doc updated, `AGENTS.md` on the wrapper with sub-processes, layout rule 0 | moves only; the suite and CI green | done: #292, with the image's build context fixed in #293; main `9aae814` green on every leg, image published |
| 3 | The domain files in every crate. The hub's 63 flat `web/` files go to their domains. Defects found on main get fixed here: the contract drift (`scripts/generate-contracts.cjs --check` fails on main), and unused code in `engine/hub/src/machines/access/` (`Requested`, `Revoked`, `revoke_machine`, `agent_resumed`) | the layout check holds the roles | in progress: the pages moved into their domains (`web/` keeps only the shell: `mod.rs`, `routes.rs`, `pages.rs`, `hub.css` and the shell's folders), and each domain merges its own `routes()`. Still open: the role-file names in every domain, `/go/*` routes (`workflows/web` is at its ratchet), identity's handlers in `web/routes.rs`, the contract drift and the unused code |
| 4 | bebop B0 (§6): the rule format, the structure engine, XML and plain-text output, and examples as tests. The first rules are the layout tests (the 500-line ratchet, no file beside a folder of its name, no `*_tests.rs`), plus the naming rules N1 to N5 (§5). Today's violations go on a shrink-only baseline | the layout tests run as bebop rules; every rule's examples pass | pending |
| 5 | The names (§5), under bebop's naming rules: every crate, layer and hub domain renamed; table prefixes moved by one clean migration per domain, here and in AgentHub; MCP tool prefixes; `.ai/` per boundary (§8). The naming baseline goes to zero | no naming violation, no baseline | pending |
| 6 | The rest of step 3: the role files in every domain; `/go/*` and identity's handlers out of `web/routes.rs`; the contract drift; the unused code | the role rules hold with no baseline | pending |
| 7 | bebop B1 and B3: proposing and approving rules (only the operator approves), hooks on every edit and at the end of a turn, pre-commit and CI, and the internal/official build split | an agent's edit gets its XML back; a rule cannot be weakened without approval; the official build holds no devtool | pending |
| 8 | Absorb S1, the Go AgentHub (`agent-hub`), by domain in parallel worktrees, with bebop B2's code rules. The S1 ledger records 737 files: 2 authored, 62 partial and 673 unverified. They include `server/` (the groups below), `bridge/` (the Go machine bridge: tools, processes, runner, GUI, file operations, search, xlsx) and `web/src` (the React UI, converted in step 10). Each group is one complete PR, with the Go tests ported first. The hub's engine and interface split happens per domain here | every S1 row closed | pending |
| 9 | S2 to S4 (128 paths of docs, packaging, enrollment, auth and connectivity) as one batch. S5 is closed in the ledger: its content came in through #252 (increments 201 to 205) | S1 to S5 closed | pending |
| 10 | bebop B4 (the UI rules), then the UI ([ui.md](../../patterns/ui.md)): ColdLight tokens and primitives with their stories, then every screen converted, domain by domain, in the order the domains are ported | the stories and the routes for every domain | pending |
| 11 | Alpha acceptance on the operator's own server and phone | his receipts | pending |

**Step 4's groups:**

| Group | Go packages |
|---|---|
| machines | `machines`, `machineops`, `bridgehub`, `transport` |
| access and policy | `access`, `auth`, `browserauth`, `policy`, `policygate`, `policyrules`, `changepolicy` |
| providers and secrets | `providers`, `subs`, `secretbox` |
| catalog and settings | `catalog`, `presets`, `schemacheck`, `config` |
| guides | `guides`, `guidekit` |
| activity and audit | `actionfeed`, `auditkeep` |
| interfaces | `devapi`, `httpapi`, `mcptools`, `stream` |

`store` and `db` go into each domain's `store.rs`; `lifecycle` and `health` go into
`runtime/`.

## 5. Names (agreed by the operator, 2026-11-25)

The operator asked for clear, self-describing, consistent names, each with a canonical prefix.
bebop's `naming` rules hold these.

- **N1. A folder names what it holds.** It is kebab-case `<qualifier>-<subject>`
  (`postgres-store`, `machine-broker`). Never `core`, `common`, `shared`, `utils`, `helpers`,
  `misc`, `lib`, `base` or `manager`.
- **N2. One prefix names a thing everywhere:**

  | Where | Form |
  |---|---|
  | crate | `devagotchi-<folder>` |
  | Rust module | the folder in snake_case |
  | Postgres tables | `<domain>_` |
  | pages | `/app/<domain>/…` |
  | JSON API | `/api/v1/<domain>/…` |
  | MCP tools | `<domain>_<verb>` |
  | UI route folder | `ui/src/routes/(app)/<domain>/` |

  The Desktop Commander tools keep their upstream names (`read_file`, `edit_file`, …), so the
  parity is exact.
- **N3. Domains are plural count nouns** (`machines`, `workflows`, `notifications`). Mass
  nouns stay as they are (`identity`, `tenancy`, `audit`).
- **N4. Layer folders are singular:** `platform/`, `engine/`, `interface/`, `runtime/`,
  `devtool/`. `platform/` keeps the industry's name for what everything stands on.
- **N5. Every domain has the same role files:** `mod.rs`, `model.rs`, `store.rs`,
  `service.rs`, `mcp.rs`, `http.rs`, `web/`, `cli.rs`, `tests/`.

**Crates:**

| Now | Becomes | Crate |
|---|---|---|
| `platform/contracts` | `platform/contracts` | `devagotchi-contracts` |
| `platform/store` | `platform/postgres-store` | `devagotchi-postgres-store` |
| `platform/ipc` | `platform/local-ipc` | `devagotchi-local-ipc` |
| `engine/machine-engine` | `engine/machine-broker` | `devagotchi-machine-broker` |
| `engine/kernel` | `engine/lexicon` | `devagotchi-lexicon` |
| `engine/connectors` | `engine/client-catalog` | `devagotchi-client-catalog` |
| `engine/hub` | `engine/hub` | `devagotchi-hub` |
| `interfaces/mcp` | `interface/local-mcp` | `devagotchi-local-mcp` |
| `runtime/devagotchi` | `runtime/supervisor` | `devagotchi-supervisor` (the binary stays `devagotchi`) |
| `runtime/link` | `runtime/machine-link` | `devagotchi-machine-link` |
| (new) | `devtool/bebop` | `devagotchi-bebop` (internal build only) |

**Hub domains:**

| Now | Becomes | Table prefix (today's in brackets) |
|---|---|---|
| `ops` | `audit` and `analytics` | `audit_`, `analytics_` |
| `notify` | `notifications` | `notifications_` (`push_`, `mail_`, `signal_`) |
| `catalog` | `capability_catalog` | `capability_catalog_` |
| `kernel` | `lexicon` | `lexicon_` (`kernel_`) |
| `serve` | `http_server` | none |
| `site` | `site_content` | none |
| `login` and `identity` | `identity` (merged) | `identity_` (`auth_`, `oauth_`) |
| `config` | `deploy_config` (the config file; `/admin` stays `settings`) | none |
| `settings` | `settings` | `settings_` (`hub_`) |
| `connections` | `connectors`, as ruled 2026-11-13: the services agents call are connectors, AI clients are clients | `connectors_` (`connector_`) |
| `mcp` | `mcp_server` | none |
| `store` | `database` | none |
| `tests` | `source_checks` | none |
| `web` | `web_shell` | none |
| `machines`, `tenancy`, `workflows`, `offers` | unchanged | `machines_`, `tenancy_` (`tenancy_`, `org_`, `project_`), `workflows_`, `offers_` |
| AgentHub (its own repository) | unchanged | `agenthub_` (`board_`) |

Pre-release: every table rename is one clean migration per domain, with no compatibility shim.

## 6. bebop, the rules devtool (agreed 2026-11-25)

**What it is.** bebop lives in `devtool/bebop/`: a Rust crate with its own binary. It is never
in the official build.
- The internal build is the workspace plus `devtool/`, and `devagotchi` with an `internal`
  feature for dev stats and diagnostics.
- The official build is the release pipeline's `devagotchi` alone.
- A check proves the split.

**A rule is data.** Each rule is `<boundary>/.ai/rules/<category>/<id>.rule.yml`, held by the
boundary that owns it. Nothing is shared until its reuse is confirmed. A rule records:
- `id` and `category`;
- `statement`, the operator's words;
- `why`, with the upstream practice it follows;
- `scope`: paths, domains and languages. A rule covers every file in scope, whatever its
  provenance: written by hand or by an AI, absorbed, or taken from a library. Provenance is
  recorded and never exempts;
- `check`: the engine and its rule;
- `guidance`: what good looks like, the fix, and an example in the code;
- good and bad examples, which bebop runs as the rule's own tests;
- `status` (proposed, approved, enforced, retired), `version`, and an approval record.

**The engines.** Structure (folders and names, native), ast-grep (code shape), wrapped linters
(clippy, stylelint, eslint and its Svelte plugin) and cargo tests. They all report through one
format.

**Proposing a rule.**
1. `bebop rule propose "<words>"` drafts the category, scope, check and examples, and dry-runs
   the rule on today's code.
2. Every ambiguity becomes a question that must be answered. Nothing is defaulted silently.
3. Only the operator approves. An AI proposes, and never approves, weakens, retires or
   exempts. A check refuses a loosened rule without an approval record, and `.ai/rules/**` is
   owner-reviewed.
4. Existing violations go on a shrink-only baseline.
5. `bebop docs` renders the pattern pages from the rules, so the docs never drift from what is
   enforced.

**Output.** One finding, two forms: XML for an AI, and readable text for a person. The XML
looks like this:

```xml
<bebop-report rules="42" violations="1">
  <violation rule="ui.component-has-story" category="ui" severity="error"
             file="ui/src/lib/coldlight/primitives/Button/Button.svelte" line="1">
    <statement>Every ColdLight component has a Storybook story beside it.</statement>
    <why>A story shows every state; reviewers and the visual tests read it.</why>
    <expected>ui/src/lib/coldlight/primitives/Button/Button.stories.svelte</expected>
    <fix>Add Button.stories.svelte with one story per variant in Button.variants.ts.</fix>
    <example path="ui/src/lib/coldlight/primitives/Badge/Badge.stories.svelte"/>
  </violation>
</bebop-report>
```

Every finding says what good looks like and how to reach it. An error exits non-zero.

**Where it runs:** on every file an agent edits (a hook, its XML handed straight back), at the
end of an agent's turn, at pre-commit, and in CI.

**The order:**

| Phase | What |
|---|---|
| B0 | Format, structure engine, output, examples as tests, the layout and naming rules |
| B1 | Propose and approve |
| B2 | ast-grep and the code rules taken from the patterns |
| B3 | Hooks, CI, the build split |
| B4 | The UI rules |

## 7. The lanes

Each lane has:
- a skill, which explains how to do the work;
- its rules and checks, which enforce it;
- a hook, which says when it runs;
- a hand-off: the lane writes its status into this plan, and the next lane starts only from
  it.

A skill loads by relevance, so it guides. The checks and hooks enforce, whether or not a
skill was loaded.

| Lane | Does | Ends when |
|---|---|---|
| plan | the plan, agreed with the operator; no code | the operator approves |
| absorb | existing code in, ledger first | the ledger's rows are closed |
| engine | domain code in the role files | its rules hold |
| ui | ColdLight and Svelte, under [ui.md](../../patterns/ui.md) | its rules and stories hold |
| verify | checks, CI and receipts | green, with receipts |

## 8. `.ai` per boundary

Every boundary has its own `.ai/`:
- a README with Owns, Serves, Settings and Depends on;
- its patterns, its rules and its plan.

The boundaries are `platform/`, `engine/hub`, `engine/machine-broker`, `interface/`,
`runtime/`, `ui/` with ColdLight, and AgentHub's repository.

The root `.ai/` keeps only what governs everything: the operator rules, this program and the
lanes. The shared rules G1 to G14 move into the boundaries that use them. A fact lives at the
narrowest boundary that owns it, and a rule is promoted only when two boundaries confirm the
same reuse. ColdLight stays inside Devagotchi's UI boundary. Components from other projects
(`platform_devpulse/packages/coldlight` among them) come in only through intake, held to
`ui.md`.

## 9. What this page replaces

These plans keep their detail and history. Their order and their target layout are replaced
by this page:

- [standalone-build](standalone-build.md): the waves, the tree and the naming rules.
- [rewrite](rewrite.md): B5 and B6.
- [structure-cleanup](structure-cleanup.md): S8 to S12.
- [absorb-preserved-sources](absorb-preserved-sources.md): the packet order. Its S1 to S5
  source accounting stays the record that step 8's ledger closes.

## Verification receipts

Step 2 checks, at this branch on main `7eda906`, Windows 11 (Docker Desktop not running, so
no local Postgres):

- `cargo metadata` and `cargo fmt --check` pass.
- `cargo build --locked --bins` and `cargo check -p devagotchi-desktop` build.
- `cargo test --workspace` without Postgres: every test passes, the layout test included.
  The Postgres tests skip locally, and CI's `hub (postgres)` and `e2e (ubuntu-24.04)` run
  them.

## Delivery

Implementation: step 2 in this PR. Validation: local as above; CI pending. Push: pending.
PR: pending. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
