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
- `ui/` is the Svelte app, added in step 6.

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
| 4 | Absorb the Go AgentHub (S1) by domain, in parallel worktrees. First a generated ledger (every Go file and function, its target, ported, equivalent or dropped with a reason) and a test that fails while a row is open. Then one complete PR per group, the Go tests ported first. The hub's engine and interface split is done per domain here | every S1 row closed | pending |
| 5 | S2 to S4 (128 paths of docs, packaging, enrollment, auth and connectivity) as one batch. S5 is closed in the ledger: its content came in through #252 (increments 201 to 205) | S1 to S5 closed | pending |
| 6 | The UI ([ui.md](../../patterns/ui.md)): ColdLight tokens and primitives with their stories, then every screen converted, domain by domain, in the order the domains are ported | the stories and the routes for every domain | pending |
| 7 | Alpha acceptance on the operator's own server and phone | his receipts | pending |

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

## 4. What this page replaces

These plans keep their detail and history. Their order and their target layout are replaced
by this page:

- [standalone-build](standalone-build.md): the waves, the tree and the naming rules.
- [rewrite](rewrite.md): B5 and B6.
- [structure-cleanup](structure-cleanup.md): S8 to S12.
- [absorb-preserved-sources](absorb-preserved-sources.md): the packet order. Its S1 to S5
  source accounting stays the record that step 4's ledger closes.

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
