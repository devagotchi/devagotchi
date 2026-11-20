# The standalone build: what is left, in order, and where each piece lives

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Request: the operator, 2026-11-20: plan everything that is left so that it is built with
  consistent patterns and senior practice: folders nested where a domain needs them, proper
  names, clear files and clear separations. The aim is a self-hosted edition the operator can
  test end to end, so that the cloud version can follow.
- PR: not_created
- Branch / baseline: `increment-275/standalone-build-plan`, from main `726af4e` (B4).
- Objective: one order for everything between today's `main` and a standalone edition proven
  on the operator's own server and phone. Each piece gets the folder and files it lives in,
  the pattern it follows and the test that proves it.
- Non-goals: the cloud edition. No new feature beyond [the path](path.md) step 1 and the
  increments of [the self-hosted edition](one-user-edition.md), except the operator's
  2026-11-20 clarification: one environment may have local users. Its branch integration,
  Docker test deployment and local-user acceptance are tracked in
  [release readiness](v0.1-release-readiness.md#self-hosted-baseline-2026-11-20).
- Who owns what, so there is one truth:

  | Question | Owner |
  |---|---|
  | What "done" means, and which rows the standalone edition needs | [path](path.md) |
  | Each increment's behaviour, tests and dependencies | [one-user edition](one-user-edition.md) (208 to 270) |
  | B5 and B6 | [rewrite](rewrite.md) |
  | The S8 to S12 moves | [structure cleanup](structure-cleanup.md) |
  | PDF | [write-pdf](write-pdf.md) (P10a to P10g) |
  | The dev build, tracing and diagnostics | [dev-build-and-diagnostics](dev-build-and-diagnostics.md) (D1 to D7) |
  | How code is written | [the patterns](../../patterns/README.md) |
  | **The order across all of them, the target tree, the naming rules and their status** | **this page** |

  Some increments name a file at its path at `f1b48e8`. Where that file has moved since,
  the tree in §5 and §6 wins.

## 1. Where `main` is (726af4e, read 2026-11-20)

- **Merged:**
  - rewrite B0 to B4 (#259 to #264);
  - one-user increments 208 (#253), 209, 225, 245 and 246 (#258);
  - earlier: 193 (test accounts), 197 (AgentHub on Postgres), 198 to 200 (`/admin` live
    settings, mail, sinks) and 201 to 207.
- **Built for every commit (B2a):** every push to `main` publishes
  `ghcr.io/devagotchi/devagotchi:main` and `:sha-<short>`. The release run for `726af4e`
  succeeded.
- **Proven on a real device:** nothing yet. Path Phase 0 has not run.
- **The test deployment is old.** It runs a build from before B1: its `machines` tool still
  says to pass a machine as `broker`, which B1 renamed to `machine`.

## 2. How the order is decided

- **O1. Prove before building more.** Path Phase 0 runs first, on the operator's server and
  phone. What it finds is fixed before the next feature wave starts. Pure moves (W1) may run
  while Phase 0 runs, because they change no behaviour.
- **O2. Move, then change.** A move lands as its own PR, before the PR that changes the moved
  code.
  - A move is renames, `mod` lines and the router's `merge` lines, with no behaviour change.
  - The route table and the tool list stay byte-identical across it (§8).
  - Reviews stay readable, and `git log --follow` keeps each file's history.
- **O3. Patterns before features in the same files.** S8 comes before B5, and B5 before the
  phone pages. B6 comes before any increment that adds a table (238, 258, 259, 5.8).
- **O4. The operator's priority order otherwise:**
  - Desktop Commander;
  - Shippin and management;
  - the update path;
  - the phone's high bar;
  - notifications everywhere.

  The plugins come forward right after M1 (operator, 2026-11-18).
- **O5. One concern per PR.** Branches are named `increment-N/<topic>`. Each PR is merged on
  green. Account for every local and remote branch against current `main`, including the
  AgentHub submodule repository. Merge dependency branches first, retain unique changes,
  and verify squash/rebase merges through their PR and resulting diff. Delete a branch only
  after its absorption is verified. An unavailable remote leaves this check unverified.

## 3. The waves

| Wave | What | Pieces, in order | Starts after | Proven by |
|---|---|---|---|---|
| W0 | Prove what is built | Test deployment on `:main` (0.6, operator); 210 manual join and `/dist/`; 211 plugins point at the person's hub, README tells the truth; then the operator runs path 0.0 to 0.5 | nothing | path Phase 0 proofs. Each finding becomes a row in §10 |
| W1 | Every domain serves its own pages; one home per module | §5 and §6, one PR per step (the W1 steps below) | W1.1, W1.2, W1.11: nothing. W1.3 to W1.9: D6 (§9) | route table and tool list byte-identical; test count unchanged; layout tests; `beside.txt` deleted |
| W2 | B5: pages, identity, configuration, fluid styles | [rewrite](rewrite.md) B5 on the moved tree | W1.9 (B5's guard changes touch pages in every domain) | rewrite B5 checks; the style checks in §4 (N9) |
| W3 | The phone (milestone M1) | 247, 248, 249, 250, 251, 252, 236, 257; then 253, the real-phone receipt, round 1 | W2 | `android_browser.py` at 390 and 412 px; 253's receipt |
| W4 | The plugins, brought forward | 260, 261, 262 (AgentHub repository), 264, 267, path 5.4 (no hub) | W3 | an approval and a ticket note each raise a notification in a real Claude Code session and a real Codex session |
| W5 | B6: persistence and the shape of the tests | [rewrite](rewrite.md) B6 | W2 | rewrite B6 checks |
| W6 | Desktop Commander, all of it (M2), and the conformance run | 212; D1, D2 (tracing); 214 to 219; 221, 222; P10a to P10g (223); 256; 220; 5.8; 5.9; 224 | W1.11, W5 | the parity table has no "Missing" row the operator kept; the conformance report (5.9), every tool consistent across repeats |
| W7 | Shippin and management (M3) | 226 to 235 | W2 | each increment's acceptance |
| W8 | The update path (M4) | 237 to 243, 270; 244 last | W5 | 243's CI job; an update started from the phone |
| W9 | Live and the dev build (M5) | 254, 255; D3 to D7 | W3, W6 | 253, round 2, on the operator's phone |
| W10 | Notifications everywhere (M6) | 258, 259, 263, 265, 266, 268, 269 | W5, W4 | each increment's receipt |
| Exit | The standalone edition | path step 1 rows on the operator's server and phone; the operator cuts v0.1.1 | all | path receipts |

The W1 steps. Each is one PR of moves only (§8, item 2). A file over the cap is split by
subject in the PR that moves it.

| Step | What | Splits it carries |
|---|---|---|
| W1.1 | Layout rule 2 in the hub: `site.rs`, `site/walkthrough.rs`, `config/live.rs`, `machines/files.rs`, `machines/operations.rs` and `ops/audit.rs` each become `<x>/mod.rs`; `source_tests.rs` becomes `tests/sources.rs` | `site.rs` (658) |
| W1.2 | The shell first, so later moves only remove from it. Write the golden route table. Then: the harness from `web/tests/mod.rs` to `web/testing/`; `web/pages.rs` to `web/pages/mod.rs`; `hub.css` to `web/style/`; `web/assets.rs`; the shrink-only `*_tests.rs` check (N6) | `web/tests/mod.rs` (1204) |
| W1.3 | identity: `identity/web/` and `identity/connect/web.rs` | `passkeys.rs`, `passkey_tests.rs`, `providers.rs`, `oauth.rs` |
| W1.4 | settings (new domain) with notify's mail and sinks cards, because the `hub_settings_*` files split between them | `notify/mail.rs` |
| W1.5 | agenthub: board, ticket, approvals | `web/approvals/mod.rs` (601) |
| W1.6 | machines | none |
| W1.7 | site: landing, media, public, discovery and their tests | `landing.rs` (654) |
| W1.8 | tenancy and ops: hosted, operator, orgs, analytics, counted | `orgs.rs` (1082), `analytics.rs` (607) |
| W1.9 | connections, offers, workflows. `crate::web` is now the shell alone | `workflow.rs` (1086) |
| W1.10 | The `*_tests.rs` list is empty, so the check becomes absolute and the list is deleted | none |
| W1.11 | Layout rule 2 in contracts, ipc, machine-engine and mcp (§6). `beside.txt` is deleted and the check is absolute | `broker.rs` (1153), `connect.rs` (802) |
| W1.12 | Checks: `crate::web` holds only the shell (N2); forbidden and compound names (N4, N5) | none |
| W1.13 | S12: a test that every front door has its four sections | none |

Two notes on this order:
- **W6 and W4 may swap.** Nothing between them depends on the order; only B6 has to come
  before a table is added. Swap them if Desktop Commander should come before the plugins.
- **256 moves into W6,** because 220 needs its page.

## 4. Names and folders (each rule has a check)

| # | Rule | Check |
|---|---|---|
| N1 | A domain is `src/<domain>/` with `mod.rs` as its front door. The front door's docs have *Owns*, *Serves*, *Settings* and *Depends on* (layout rules 2 and 7) | `layout.rs` rule 2 check (absolute after W1.11); the S12 front-door test (W1.13) |
| N2 | A domain's pages live in `<domain>/web/`. `web/mod.rs` exports `pub fn routes() -> Router<AppState>`; `crate::web` merges them and holds only the shell (§5) | a test that `crate::web` holds only the shell's files (W1.12) |
| N3 | A page that has its own style, script or tests is a folder: `<domain>/web/<page>/{mod.rs, page.css, page.js, tests.rs}`. A page with none of these is `<page>.rs`. Until a page's folder exists, its sheet is `<page>.css` beside `<page>.rs` (the board and the ticket since 2026-11-20); the move into the folder renames it `page.css`. `mod.rs` holds the handlers (read, guard, one call to the owner, POST-303-GET). `view.rs` holds the markup once `mod.rs` passes 300 lines | review; the 500-line ratchet |
| N4 | A file is named for what it holds: a noun, snake_case. A collection page is plural (`machines`), one thing is singular (`machine`). No `utils`, `helpers`, `common`, `misc` or `manager` | a forbidden-names check in `layout.rs` (W1.12) |
| N5 | No compound names where a folder says it. `hub_settings_mail.rs` becomes `notify/web/admin/mail.rs`, not `settings/web/settings_mail.rs`. A path never repeats its own domain | the same check: a file's stem may not start with its parent folder's name |
| N6 | Tests are `<x>/tests.rs` or `<x>/tests/<subject>.rs`, never `<x>_tests.rs` (layout rule 3; 20 such files today) | `layout.rs`: no `*_tests.rs`, held by a shrink-only list that is empty and deleted at W1.10 |
| N7 | `web/` holds no SQL. A page calls its domain's owner (G1), which calls `Store` | B6 `sql.rs`: no `sqlx` under any `web/` |
| N8 | Contracts are family modules (`crates/contracts/src/<family>.rs`, re-exported from `lib.rs`). A new wire shape goes in its family, never in `lib.rs` | review; `lib.rs` at or under 431 lines |
| N9 | Styles: `@layer reset, tokens, base, components, pages`. Tokens come from `tokens.json`, as one fluid `clamp()` scale. A page's sheet holds only `@layer pages { [data-page="<page>"] … }` and never restyles another page's classes | `web/tests/styles.rs` (W2) parses every `page.css` |
| N10 | Assets: each page's script and sheet sit beside its Rust. The shell's `web/assets.rs` lists every asset once, from its owner, under a content-hashed name (248) | `web/tests/assets.rs` |
| N11 | New Python tests follow `tests/<area>_<kind>.py` (`probe`, `browser`, `device`). A suite with many cases gets a folder (`tests/conformance/`) | `test_shape.rs` (B6) |
| N12 | No private host names, no secrets and no personal paths in code, docs, commits or PR bodies. The repositories are public | review; `naming_tests` (moved to `site/web/tests/naming.rs`) |

## 5. The hub's target tree (after W1)

`←` names where a file comes from today. Every move takes its `oversize.txt` entry along at the
same number (layout pattern).

```text
crates/hub/src/
  lib.rs  extensions.rs  test_keys.rs           unchanged (wiring)
  tests/sources.rs                              ← source_tests.rs (cross-domain source scans)

  web/                                          THE SHELL ONLY
    mod.rs                                      kit, guards, Who
    routes.rs                                   merges each domain's routes(); nothing else
    pages/{mod.rs ← pages.rs, head.rs}
    style/{reset,tokens,base,components}.css    ← hub.css, split by layer in W2
    assets.rs                                   one table of every asset (N10)
    assets/{app.js, sw.js}                      the shell's own scripts
    app/{mod.rs, home.rs, tests.rs}             the phone frame and Home
    one_user/{mod.rs, tests.rs}                 the one-user edition's surface rules
    testing/mod.rs                              the whole-router harness, from tests/mod.rs, for every domain's tests
    tests/{mod.rs, routes.rs}                   shell tests; the golden route table (§8)

  identity/web/
    mod.rs
    claim.rs                                    ← web/claim.rs (+ tests/claim.rs ← claim_tests.rs)
    device.rs  email.rs  session.rs  sessions.rs
    keys.rs                                     ← web/app/keys.rs
    me.rs                                       ← web/app/me.rs
    nomen.rs                                    ← nomen_login.rs (+ tests/nomen.rs ← web/tests/nomen_login.rs)
    sign_in/{mod.rs ← sign_in.rs, page.css}
    passkeys/{mod.rs, …, passkey.js, tests/…}   ← passkeys.rs 1112, passkey_tests.rs 1534 (split by subject)
    providers/{mod.rs ← providers.rs, apply.rs ← provider_apply.rs, cards.rs ← sign_in_cards.rs, tests/shippin.rs ← shippin_tests.rs}
    oauth/{mod.rs, …}                           ← web/oauth.rs 751 (split by subject)
    tests/
  identity/connect/web.rs                       ← web/connectors.rs (/connect)

  settings/                                     NEW DOMAIN: /admin, owns hub_ tables
    mod.rs
    store.rs                                    ← ops/hub_settings.rs
    web/{mod.rs ← hub_settings.rs, owner.rs ← hub_settings_owner.rs, live.rs ← hub_settings_live.rs,
         tests/{admin.rs ← hub_settings_tests.rs, live.rs ← hub_settings_live_tests.rs}}

  notify/
    mail/{mod.rs ← mail.rs 539, cell.rs ← web/mail_cell.rs}
    web/{mod.rs, phone.rs ← web/notify.rs, push.js,
         admin/{mail.rs ← hub_settings_mail.rs, sinks.rs ← hub_settings_sinks.rs},
         tests/{mail.rs, sinks.rs}}

  agenthub/web/
    mod.rs                                      ← web/app/agenthub/mod.rs
    board.rs                                    ← web/app/agenthub/board.rs
    ticket/{mod.rs ← ticket.rs, actions.rs ← ticket_actions.rs, tests.rs ← ticket_tests.rs}
    approvals/{mod.rs ← web/approvals/mod.rs, ahead.rs, tests.rs ← web/app/approval_tests.rs}

  machines/
    files/mod.rs ← files.rs    operations/mod.rs ← operations.rs
    web/{mod.rs ← web/machines.rs, machine.rs ← web/app/machine.rs, tests.rs ← web/tests/machines.rs}

  site/
    mod.rs ← site.rs 658 (split)    walkthrough/mod.rs ← walkthrough.rs
    tests/{assets.rs ← assets_tests.rs, brand.rs ← brand_tests.rs}
    web/{mod.rs, public.rs, discovery.rs, media.rs,
         landing/{mod.rs ← landing.rs, compare.rs, network.rs, popups.rs, walkthrough.rs,
                  page.css ← landing.css, landing.js, reveal.js,
                  tests/{landing.rs, walkthrough.rs}},
         tests/{site.rs, naming.rs, discovery.rs, media.rs}}

  tenancy/web/{mod.rs, hosted.rs, operator.rs (+ tests), orgs/{mod.rs, …, invite.js} ← orgs.rs 1082 (split)}
  ops/
    audit/mod.rs ← audit.rs
    web/{mod.rs, analytics.rs (+ tests ← analytics_tests.rs), counted.rs}
  connections/web.rs                            ← web/app/connections.rs
  offers/web.rs                                 ← web/offers.rs
  workflows/web/{mod.rs, …, workflow.js}        ← web/workflow.rs 1086 (split: page, actions, cookies)
  config/live/mod.rs                            ← config/live.rs
```

`owner-password.js` goes to whichever page loads it. W1.4 records which one.

## 6. The other crates' target trees

```text
crates/contracts/src/
  wire.rs                                RequestBody, ResponseBody, WireResponse, RequestBody::domain() (212)
  interactive/{mod.rs, tests.rs}         system/{mod.rs, tests.rs}             (rule 2)
  calls.rs (214)   documents.rs (221 to 223; PDF and Office shapes stay out of files.rs)

crates/machine-engine/src/
  broker/mod.rs        ← broker.rs 1153: Broker, BrokerConfig, ExecPolicy, open, handle, shutdown
  broker/start.rs      ← normalize, start, with_launch
  broker/operations.rs ← wait, operations_changed, get, list, resize, cancel, validate_id, check_size
  broker/state.rs      ← Operation, State, Shared and its methods
  broker/search_sessions.rs (215)        broker/system/{mod.rs, web/{mod.rs, tests.rs}}
  execution/{mod.rs, platform/{mod.rs, linux/mod.rs, windows/mod.rs, …}}   (rule 2)
  files/{mod.rs, lines/mod.rs, manage/mod.rs, documents/{mod.rs, office.rs, pdf.rs}}
  journal/{mod.rs, tests.rs}

crates/mcp/src/
  adapter/mod.rs ← adapter.rs
  adapter/files/{mod.rs ← file_tools.rs, documents.rs}    adapter/system.rs ← system_tools.rs
  adapter/calls.rs (214)
  capabilities/mod.rs   image/mod.rs   run/mod.rs          (rule 2)
  connect/{mod.rs, …}   ← connect.rs 802 (split by client)

crates/ipc/src/windows/mod.rs ← windows.rs

crates/documents/                        NEW CRATE, proposed in place of P10's crates/pdf (§9 D9)
  src/lib.rs                             wiring only
  src/pdf/{mod.rs, markdown.rs, world.rs, edit.rs, stamp.rs, read.rs, raster.rs, worker.rs}
  src/office/{mod.rs, docx/{read.rs, write.rs, xml.rs}, xlsx/{read.rs, write.rs}}
  src/fonts/{mod.rs, pack.rs}            the shared font pack, read by the hub too

plugins/
  devagotchi/                connector and skills (existing)
  devagotchi-agenthub/       hooks (existing); + monitors/ (264); + .codex-plugin/ (267)
  devagotchi-local/          NEW (path 5.4): `devagotchi mcp` over stdio, no hub
apps/vscode/                 NEW (268), beside apps/desktop
```

The machine-engine's broker files each stay under 500 lines. `broker/mod.rs` keeps only the
front door and the dispatch, which 212 turns into a match on `RequestBody::domain()`. A new
request then adds no line to it.

## 7. Where each wave's new code goes

| Increment | New or changed files (the tree above) |
|---|---|
| 210 | `machines/join/dist.rs` (new: `dist_routes()`); the `machine_add` manual text (`machines/manage.rs`); `machines/web/tests.rs`; `tests/compose_join.py` |
| 211 | `plugins/`, `README.md`, `crates/devagotchi/tests/readme_truth.rs` (new) |
| B5 | per [rewrite](rewrite.md) B5, on the paths of §5. Styles: `web/style/*.css` and each page's `page.css` |
| 236 | `ops/health.rs` (build, commit, channel); the About section in `identity/web/me.rs` |
| 247 to 251 | `identity/web/sign_in/page.css`; `web/assets.rs` (hashed names, 248); `web/style/components.css` (48 dp, 249; depth and type, 250); `site/pwa.rs` (251) |
| 257 | `notify/push/` and `web/assets/sw.js` |
| 260, 261 | `apps/desktop` (260); `identity/connect/web.rs` (261) |
| 262 | AgentHub repository: `crates/agenthub/src/cli/watch/{mod.rs, feed.rs, tests.rs}` |
| 264, 267 | `plugins/devagotchi-agenthub/{hooks,monitors}/`, `.codex-plugin/` |
| 5.4 | `plugins/devagotchi-local/`; `crates/hub/src/mcp/tests/plugins.rs` holds it to the local tool list |
| 212 | `contracts/src/wire.rs`; `machine-engine/src/broker/mod.rs` |
| D1, D2 | the request id in `contracts/src/wire.rs`; `tracing` in the broker, `devagotchi mcp`, the link and the hub; `machine-engine/src/trace.rs` becomes the subscriber's setup |
| 214 | `contracts/src/calls.rs`; `machines/calls/{mod.rs, table.rs, tests.rs}`; `mcp/src/adapter/calls.rs` |
| 215 | `contracts/src/search.rs`; `machine-engine/src/broker/search_sessions.rs`; `machines/search.rs` |
| 216 | `contracts/src/interactive/mod.rs`; `broker/{input,output}.rs`; `adapter/terminal.rs` |
| 217 | `contracts/src/system/mod.rs`; `broker/settings.rs`; `adapter/system.rs`; `machines/system.rs` |
| 218, 219 | `machine-engine/src/files/`; `adapter/files/`; `machines/files/` |
| 221, 222, P10 | `crates/documents/`; `machine-engine/src/files/documents/`; `contracts/src/documents.rs`; `adapter/files/documents.rs`; `machines/files/documents.rs` |
| 256 | `agenthub/web/approvals/one.rs` |
| 220 | `machines/review.rs`; `machines/link/` (the stop frame); `contracts/src/link/mod.rs` |
| 5.8 | audit rows gain `duration_ms`, `error_code`, `bytes_in`, `bytes_out` and `client` (a migration under the B6 store pattern); `mcp/src/adapter/history.rs` keeps the same fields; the page is 233's `settings/web/activity.rs` |
| 5.9 | `tests/conformance/{run.py, report.py, cases/<tool>.py}`; `crates/hub/src/mcp/tests/conformance.rs` fails on a tool with no case and on a Desktop Commander row with none |
| 226 to 235 | `identity/web/providers/` (226 to 228); `settings/web/{shippin.rs, activity.rs, limits.rs}` (229, 230, 233, 234); `identity/web/{security.rs, ai_clients.rs}` (231, 232); `machines/web/settings.rs` (235) |
| 237 to 244, 270 | `serve/shutdown.rs` (237); a NEW DOMAIN `updates/{mod.rs, release.rs (240), host.rs (238), provenance.rs (239), web/{mod.rs, hub.rs (241), machines.rs (242)}, tests/}`, whose table prefix `update_` is added to layout rule 8 when 238 stores; `deploy/compose/` (host side) |
| 254, 255 | `machines/web/job/{mod.rs, page.css, page.js}` (254); `web/events.rs` (the shell's SSE) with each domain's source in its `web/mod.rs` (255) |
| D3 to D7 | `site/pwa.rs` (the manifest by channel); `ops/diagnostics/{mod.rs, bundle.rs, redact.rs, tests.rs}`; the `diag` verbs in `crates/devagotchi` |
| 258, 259 | `notify/channels/`; `identity/preferences/`; `notify/approvals/` (the durable notifier on `signal_cursors`, one-user decision 4) |
| 263, 265, 266, 268, 269 | `apps/desktop`; `crates/hub/src/mcp/views/`; `crates/kernel/src/cues/`; `apps/vscode/`; `plugins/` and the `.mcpb` package |

## 8. Every PR, done properly

1. **A plan.** Each PR has a plan under `.ai/dev/plans/`, or a row here for a W1 move.
   Delivery states are recorded honestly.
2. **A move PR (W1)** uses `git mv`, so the history follows the file. It changes no behaviour,
   and proves that with:
   - **the golden route table:** `web/tests/routes.rs` lists every method and path, and the
     file it compares against is written once in W1.2, before the first move;
   - **the tool list:** the `tools/list` dump, as the B4 SDK check makes it, byte-identical;
   - **the test count:** the same number of tests pass before and after;
   - **the layout lists:** `oversize.txt` and `beside.txt` only shrink.
3. **A change PR** first adds the test that fails on `main` for the claim, then the code.
   - One owner decides (G1). Each surface makes one call.
   - Every input is closed (G2), every refusal is coded (G4), and every write leaves one
     audit line (G7).
4. **Front doors.** A new domain or folder gets its front door with the four sections (N1).
   Nothing outside the front door is `pub`.
5. **Before pushing:**
   - `cargo fmt --check`, then the workspace suite with
     `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1`;
   - `tests/e2e.py` on a fresh database;
   - for a page PR, the browser suites at 390 and 412 px;
   - for a tool-list change, the MCP SDK check.

   Read the results before pushing. Never chain the push after the tests.
6. **Docs:**
   - a CHANGELOG line;
   - the [product](../../../docs/product.md) status row and the
     [parity](../../docs/desktop-commander-parity.md) row the change moves;
   - this page's §10.
7. **Before merging:** check N12 (no private host names, no secrets), and that migrations
   are LF. Merge on green and delete the branch.

## 9. Decisions for the operator

Each wave proceeds on the default shown, except D6, which W1 waits for.

| # | Decision | Default |
|---|---|---|
| D6 | Where a domain's pages live (the S8 ruling) | **Each domain serves its own pages from `<domain>/web/`, and `crate::web` keeps the shell (§5).** The operator's 2026-11-20 request for nested folders and clear separations reads as this; confirm before W1.3 |
| D9 | One crate for document formats | `crates/documents`, with `pdf/`, `office/` and `fonts/`, in place of P10's `crates/pdf`, so that PDF, DOCX and XLSX share one home, its limits and one worker child. It changes P10 §13 only |
| D10 | W4 (plugins) before W6 (Desktop Commander) | yes, as the operator ordered on 2026-11-18 |
| D11 | A separate `devagotchi-local` plugin for "no hub" (path 5.4) | yes. One plugin per way of connecting, so neither plugin carries a mode switch |
| D12 | Settings cards owned by their domain (`notify/web/admin/mail.rs`, `identity/web/providers/cards.rs`), with `settings/web` as the `/admin` frame | yes |
| — | Still open from [one-user](one-user-edition.md): the update channel, the Shippin client, the notifier's home | as recorded there |
| — | Still open from [write-pdf](write-pdf.md) §14: the app's text font, and CJK and emoji fonts | as recorded there |

## 10. Status

| Wave | State | PR |
|---|---|---|
| W0 | 210 and 211 pending; Phase 0 waits for the operator's server and phone, and for the test deployment on `:main` | not_created |
| W1 | W1.1 in review; W1.2 and W1.11 may start; W1.3 to W1.9 wait on D6 | not_created |
| W2 to W10 | pending | not_created |

Phase 0 findings: none yet.

## Verification receipts

This is a plan. It was read against `main` at `726af4e` on 2026-11-20:
- `.ai/patterns/layout.md`, and `layout/oversize.txt` and `layout/beside.txt` as they are;
- every file under `crates/hub/src/web/`, with its line count and module doc;
- the outline of `broker.rs`;
- the one-user edition's increments 208 to 270 and its lanes;
- write-pdf §13, and dev-build-and-diagnostics §2 to §6;
- the release run for `726af4e` (success);
- the test deployment's `machines` tool description, which still says `broker`.

## Delivery

Implementation: done (plan only). Validation: pending. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
