# Documentation map

Read at `main` 2b3feec on 2026-11-19. Scope: every file `git ls-files '*.md'` lists, except
`crates/agenthub/` (its own repository) and `reference/` (the Desktop Commander snapshot).
That is 112 files.

The single product description will be `docs/product.md`. It is not in the tree yet. Every
document below that defines the product, its order or its platforms points there after the
rewrite.

## How to read this map

- **KEEP**: canonical for the subject named.
- **FIX**: keep it. Each false statement is listed with its line and the correction, checked
  against the tree at 2b3feec.
- **FOLD**: it duplicates another document. Move the unique content named, then delete it.
- **RETIRE**: stale or superseded. Delete it; git keeps the history. When an evidence file
  under `audit/` or `verification/` cites it, move it to `docs/history/` instead.

"Dead path" means the file is not in the tree at 2b3feec. "Lx" is a line at 2b3feec.

Method. Short and mid-size documents were read in full. The largest (CHANGELOG.md,
crates/hub/README.md, docs/desktop-commander-teardown.md, desktop-bridge-audit-and-foundation.md,
docs/agenthub-issue-lifecycle-and-wardenmesh-plan.md, .ai/dev/plans/structure-cleanup-map.md,
.ai/docs/cross-project-training-standard.md, docs/performance-and-experience-spec.md,
docs/implementation-guide.md, docs/task-graph-and-mesh-contract.md,
docs/desktop-commander-user-evidence.md, .ai/dev/plans/kernel.md, landing.md,
ai-development-capture.md) were read by heading structure and opening, with every repository
path in them checked for existence and targeted searches for SQLite, macOS, ClickHouse,
release tags and links to retiring documents. A FIX list for those files may be incomplete.

## Documents that define the product, its order or its platforms

| Document | What it defines | State at 2b3feec |
|---|---|---|
| `README.md` L5, L7 | "Universal workspace bridge"; Windows, macOS and Linux "required targets" | False on platforms (X1) and storage (X2) |
| `docs/product-and-platform-plan.md` | Calls itself the owner of product scope and phase order (L23); D01 to D25; G0 to G5 | October 2026 plan; superseded on platforms, distribution, storage and order (X1 to X5) |
| `docs/implementation-and-release-plan.md` | "Canonical release plan" (AGENTS L17, plans/README L23); R1 to R3, P00 to P12 | Superseded (X2 to X6) |
| `docs/agenthub-issue-lifecycle-and-wardenmesh-plan.md` | Initial product strategy, WardenMesh fork | Superseded by the release plan, which is itself superseded |
| `.ai/docs/editions.md` | Core, multi-tenant and cloud; three builds; licence; "what Devagotchi is for" (L74-80) | Current; one stale line (L64-66) |
| `.ai/dev/plans/path.md` | Order of phases 0 to 9; what "done" means; standalone edition scope (L28-58) | Current on order; status column read at ea50352 |
| `.ai/dev/plans/one-user-edition.md` | Increments 208 to 270 in the operator's 2026-11-18 order; platform default (L902) | Current |
| `.ai/dev/plans/standalone-build.md` | The order across path, one-user, rewrite B5/B6, the S8 moves, P10 and D1 to D7 (waves W0 to W10); the hub's and the other crates' target trees; naming rules N1 to N12 | Added 2026-11-20 at 726af4e; current |
| `.ai/dev/plans/roadmap.md` | Four tracks; core lane and cloud lane order (L32-37) | Stale; its cloud lane contradicts path.md (X4) |
| `.ai/dev/plans/free-edition.md` | "Order, as of 2026-11-15" (L188-201) | Order superseded by path.md and one-user-edition.md |
| `.ai/dev/plans/cloud-edition.md` | The commercial cloud and its milestones | Current, one internal contradiction (X6) |
| `docs/adr/0003-hosting-pooled-free-per-instance-paid.md` | How the hosted product is run | Conflicts with editions 2026-11-18 (X7) |
| `desktop-bridge-audit-and-foundation.md`, `docs/foundation-archive-readme.md`, `docs/implementation-guide.md` | October 2026 product scope, Windows/macOS/Linux | Historical |
| `apps/desktop/README.md` L5, `apps/website/README.md` L41 | Platform status of the inspector and the website | False (X1) |
| `crates/hub/site/default/content/docs.md` L42, `llms.md` L39-40 | Public platform lines: Linux x86_64/ARM64, Windows, Docker | Match the builds |
| `docs/devagotchi/README.md` L41-49 | Says product-and-platform-plan wins over the Go design | Points at a retiring document (X18) |

## Repository root

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `README.md` | 327 | Front page: install, status, components, tools, build, licence | FIX | L5: "Windows, macOS, and Linux are required targets" -> the release builds Linux and Windows on x86_64 and aarch64 (`dist-workspace.toml:13`); the hub runs on Linux (X1). L7: "The macOS provider is still required before a cross-platform release" -> remove. L5, L98-99, L327: the Sites URL, "Site version 3" and "devagotchi.net ... pending DNS" are October receipts not checked here; move them to `verification/README.md`. L106 (the "architecture and growth plan") and L120 (the "master plan") link `docs/product-and-platform-plan.md` -> `docs/product.md`. L133: store is "Postgres ... or SQLite" -> Postgres only (`crates/store/Cargo.toml` has no SQLite dependency; ADR 0005). L152: "The account (D22)" -> D22 is superseded by D25; drop the tag. L17-22: "the operator re-cuts v0.1.0" conflicts with one-user-edition decision 1 (X10). L170 ("bundled SQLite") is true: `crates/machine-engine` and `crates/kernel` link `rusqlite`. So the test increment 211 plans (`readme_truth.rs`: "README.md has no `SQLite`") would fail on L170 as written. |
| `AGENTS.md` | 74 | Canonical agent instructions | FIX | L17: "implementation plan" links `docs/implementation-and-release-plan.md` -> `docs/product.md` and `.ai/dev/plans/path.md`. L36: "private WardenMesh deployment code" -> the private repository is `shippinAI/devagotchi-cloud` (`.ai/docs/editions.md:9`). L39: "Contracts belong in `crates/contracts/src/lib.rs`" -> family modules (`crates/contracts/src/files.rs`, `search.rs`, `system.rs`, `web.rs`, `history.rs`, `interactive.rs`, `ai.rs`) re-exported from lib.rs (patterns C11, B0 text). L65: product decisions link -> `docs/product.md`; its performance budgets link -> `docs/agenthub-benchmark-plan.md` (after the fold). |
| `CLAUDE.md` | 9 | Claude discovery adapter | KEEP | Subject: Claude discovery. |
| `SECURITY.md` | 30 | Reporting and scope | FIX | L14-15: "only explicitly granted executables run; the workspace is the jail" holds only under `exec_policy` `granted`. Under `account`, which `devagotchi start` writes on first run (`crates/devagotchi/src/start.rs`, `"exec_policy": "account"`; README L58-63), the broker runs what the OS account can. State both. |
| `CHANGELOG.md` | 1592 | Release notes | FIX | Subject: release history. L230 heading "prepared for v0.1.0-alpha.1" and L239 "pin `--tag v0.1.0-alpha.1`": that tag was never cut (`.ai/dev/plans/v0.1-release-readiness.md:32`; CHANGELOG L12 says so too). |
| `desktop-bridge-audit-and-foundation.md` | 597 | October 16-17 audit and build foundation | RETIRE | Superseded by `docs/desktop-commander-teardown.md` and `.ai/docs/desktop-commander-parity.md`. L7, L13, L23 name Windows/macOS/Linux targets. Cited by `audit/planning-revision.json` and `audit/website-and-graph-revision.json`, so move to `docs/history/`. |

## .github

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `.github/copilot-instructions.md` | 9 | Copilot discovery adapter | KEEP | Subject: Copilot discovery. |
| `.github/pull_request_template.md` | 33 | PR template | KEEP | Subject: PR form. |

## .ai

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `.ai/README.md` | 48 | Alpha banner and AI contract index | KEEP | Subject: the AI contracts. After B0: one row per pattern (L36-38); add `docs/product.md`. |
| `.ai/dev/workflow.md` | 73 | DG-DELIVERY-1 | KEEP | Subject: plan, validate, deliver. |
| `.ai/docs/adapters.md` | 77 | DG-ADAPTER-1 harness matrix | KEEP | Subject: harness adapters. |
| `.ai/docs/capture.md` | 131 | DG-CAPTURE-1 | FIX | L5: "a native command in the existing hub library" -> `crates/kernel` (`crates/kernel/src/capture/mod.rs`). L8: event type "in `crates/contracts/src/lib.rs`" -> `crates/contracts/src/ai.rs`. |
| `.ai/docs/cross-project-training-standard.md` | 232 | Research and roadmap for training data | KEEP | Subject: training-data standard. It says itself it is not implemented. |
| `.ai/docs/desktop-commander-parity.md` | 122 | Tool-by-tool gap list | KEEP | Subject: Desktop Commander parity. L12 links the plan `.ai/dev/plans/desktop-commander-parity.md`, which folds into it. |
| `.ai/docs/editions.md` | 92 | Core, multi-tenant, cloud; licence; the 2026-11-18 decisions | FIX | Subject: editions and licence. L64-66: "Until R4 lands, `/admin` shows a People section even on a single-owner hub" -> increment 225 hides it on a one-user hub (`crates/hub/src/web/hub_settings.rs:216-217`, CHANGELOG L99-109); R4 (tenancy out of the binary) is still open. L74-80 ("What Devagotchi is for") moves to `docs/product.md` and is linked from here. |
| `.ai/docs/lifecycle.md` | 45 | DG-LIFECYCLE-1 | FIX | L38: "release gates" links `docs/implementation-and-release-plan.md` -> `.ai/dev/plans/path.md` ("What done means"). |
| `.ai/docs/retry-lifecycle.md` | 93 | DG-RETRY-1 | KEEP | Subject: broker retry identity. |
| `.ai/patterns/README.md` | 18 | Provider-neutral patterns | FIX | Replaced in B0 by the README in the patterns index. L17 links `docs/product-and-platform-plan.md` -> `docs/product.md`; the new README's G14.6 has the same link and needs the same change. |
| `.ai/patterns/connectors.md` | 23 | A connector is data | KEEP | Subject: AI client catalog. |
| `.ai/patterns/layout.md` | 69 | Where Rust code lives; size ratchet | FIX | L40: the prefix test is in `crates/hub/src/store/tests.rs`, not `crates/store/src/tests.rs`. Rule 3 and rule 5 amendments per B0 (G10, C11). |

## .ai/dev/plans

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `README.md` | 23 | Plan registry | FIX | L23: "Broader product delivery remains in the canonical release plan" -> remove; point to `docs/product.md`. L16-22 "Active" lists finished plans (alpha-stage-obvious, ai-development-capture, kernel K1-K5 done) and v0.1-release-readiness, which folds -> list open plans only. |
| `TEMPLATE.md` | 33 | Plan template | KEEP | Subject: plan form. |
| `agenthub-crate.md` | 85 | AgentHub as its own crate | FIX | Subject: A3, A5, A6, A8-A11. L37 ("Postgres or SQLite") and L40 ("refused on SQLite") describe the store before free-edition R3; mark them as the state at the time. L20 links `agenthub-module.md`, which retires. |
| `agenthub-factory.md` | 251 | AgentHub factory milestones | FIX | Subject: F1 to F23. L15: code is the engine in `crates/agenthub` plus the hub surface `crates/hub/src/agenthub/`. L200: F4 acceptance "on Postgres and SQLite" -> Postgres. Receives the fold of `docs/agenthub-factory-board-plan.md`. |
| `agenthub-module.md` | 41 | AgentHub in one folder | RETIRE | Done (M1-M3); superseded by agenthub-crate.md. |
| `ai-development-capture.md` | 168 | Receipt of the `.ai` packet (PR #164) | RETIRE | The contracts live in `.ai/docs/`. L16 names the retiring release plan as "broader product authority". |
| `ai-sessions.md` | 68 | AI sessions from the phone | FIX | L18: `crates/broker` (dead) -> `crates/machine-engine` (admission in its `broker` module; the running service in `service`). |
| `alpha-stage-obvious.md` | 33 | Alpha banner; installs follow `:main` | RETIRE | Done. Its M2 (`:main` default) was reversed by self-hosted S1 (X8). |
| `api-keys.md` | 36 | API keys | RETIRE | Done (CHANGELOG, increment 125). |
| `browser-experience.md` | 48 | Wide-screen layout | KEEP | Subject: B1 to B8. |
| `cloud-edition.md` | 137 | Private cloud repository and milestones | FIX | Subject: the commercial cloud. L135: "C3 onwards wait on the private repository, which does not exist" -> it exists (L120; editions L29-31). L40 "no tenancy code" conflicts with editions 2026-11-18 (X7). Receives roadmap.md's gate (L39-45). |
| `connections-gateway.md` | 92 | Outside services through the hub | FIX | L18-19: "the gateway in `crates/agenthub` (`connections`)" -> `crates/hub/src/connections/`. L51: "store tests both backends" -> Postgres. |
| `connectors.md` | 67 | AI client catalog plan | FOLD | Into `.ai/patterns/connectors.md` and `crates/connectors/README.md`. M1-M5 done; M6 (promote entries to `ci`) is the pattern's rule 5. |
| `desktop-commander-parity.md` | 221 | Parity working notes P1-P11 | FOLD | Into `.ai/docs/desktop-commander-parity.md` (open P10 `write_pdf`, the readiness row) and one-user increments 212-224. Two rows are numbered P11 (L94, L96). |
| `dormancy-and-reauth.md` | 109 | Dormant machines, re-auth by link | KEEP | Subject: M1 to M6. |
| `free-edition.md` | 263 | One developer's hub on their VM | FIX | L26-40 "What is true today" is a 2026-11-14 snapshot that is false now (`crates/core`, SQLite in the hub, `list_processes` and `get_config` missing): label it as that date's state or delete it. L169: the People section is hidden at runtime by 225. L261 "Implementation: not started" contradicts R0 and R1 done and R2 and R3 in progress (L165-168). L188-201 order is superseded by path.md and one-user-edition.md. Keep rows D1b, D2, D2b, D6, D7, E1, H1, K1, R4, S0, U0, V1: one-user-edition cites them. |
| `group-settlement.md` | 34 | Process-group settlement fix | RETIRE | Done. Names `devagotchi-core`, now `devagotchi-machine-engine`. |
| `hub-settings.md` | 39 | Every setting a guided page | FIX | L37: "Implementation: M1 and M2" -> M1-M6 and M9 done per its own table; M7 and M8 open. |
| `kernel.md` | 172 | The kernel crate, lexicon and dictionary | KEEP | Subject: kernel decisions K2-K5 (K1-K5 are done). L19 links `ai-development-capture.md`, which retires. |
| `landing.md` | 137 | Landing page plan | KEEP | Subject: landing scenes. |
| `license.md` | 49 | Licence and public image | RETIRE | M1-M3 done. M4 (each publish pulls the image with no login) is still pending: move it to the release check before deleting. The licence is `LICENSE` and editions L7, L18-20. AgentHub's licence is path.md decision 3. |
| `mirror-fallback.md` | 33 | Caddy install mirror fallback | RETIRE | Done. |
| `mobile-experience.md` | 53 | The phone track | FIX | Add rows 245-255 (one-user-edition L684 says so). |
| `mobile-navigation.md` | 42 | Tabs, swipe, Go to sheet | RETIRE | Merged (one-user-edition L684). |
| `notes-that-reach-them.md` | 37 | Notes delivered to agents | RETIRE | Done. Its M5 (real phone) is a path.md Phase 0 proof. |
| `one-user-edition.md` | 933 | Increments 208 onward, in order | FIX | Subject: the one-user edition's increments. L1 "208 to 269": increment 270 exists (L63). 211 is still open at 2b3feec: README L5 and L133 are unchanged and `crates/devagotchi/tests/readme_truth.rs` does not exist. |
| `path.md` | 228 | Every feature in order, proven on Texas and the phone | FIX | Subject: phase order and "done". Fix the status column (read at ea50352): 0.3 still says partial, but the Docker join (209) is built; 2.6 "17 clients" is fixed (225). 8.1 "apps/website says four tools" is still true (`apps/website/public/llms.txt:23`). |
| `phone-fit.md` | 34 | Landing and sign-in on a phone | RETIRE | Merged (one-user-edition L684). |
| `plugins.md` | 134 | Plugins for Claude, Codex, Grok and the 2026-11-18 list | FIX | L20: `crates/hub/src/agenthub/cli.rs` and `crates/hub/src/plugin_tests.rs` are dead -> `crates/agenthub/src/cli/` (`plugin_hooks` in `cli/hooks/install.rs`, `plugin_host` in `cli/mod.rs`) and `crates/hub/src/mcp/tests/plugins.rs`. L37 and L95-99: Codex "fixed hosted hub" conflicts with G15 (X15). |
| `roadmap.md` | 61 | Four tracks and their order | FOLD | Tracks table read at ed818cf. "Cloud lane: C1 and C2 can start now" (L36-37) contradicts path.md L52-55 (X4). Move "Gate before cloud features" (L39-45) and decisions D1-D5 into cloud-edition.md; the plan list is already in plans/README. Then retire. |
| `self-hosted-and-provenance.md` | 71 | Release channel, provenance | FIX | L69: "S1 in this branch. Validation: pending CI" -> S1 is done (L39, #189). |
| `structure-cleanup.md` | 189 | Moves and renames S0-S12 | FIX | Subject: structure steps. L184-189: S0 PR #185 "open" and S5 "draft" are stale; path.md 9.5 says S5-S7 and S9 merged. |
| `structure-cleanup-map.md` | 344 | Per-file map at 5104f35 | RETIRE | Stale: hub `lib.rs` listed at 1942 lines (L116) is 92 lines today. Regenerate from the tree when a step needs it. |
| `texas-baseline.md` | 58 | Texas installed as a customer would | FIX | M2 (L36, "follow `:main` by default") is contested (X8); mark it per the operator's decision. |
| `v0.1-release-readiness.md` | 68 | v0.1.0 release readiness | FOLD | M1-M4 done; M5 is factory F22. The tag decision moves to path.md (X10). Then retire. |
| `vibe-coder.md` | 52 | Vibe-coder features V1-V12 | KEEP | Subject: V1-V5 approved, V6-V12 proposed. |

## docs/adr

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `0001-hosted-board-postgres-and-clickhouse.md` | 67 | Hosted boards in Postgres, ClickHouse required | FIX | Add amendment notes. L29-31 and L63-66 (standalone SQLite boards) are superseded by ADR 0005 and increment 197. L8 and L32 ("ClickHouse required in every hub deployment"): the compose stack ships it, and the hub serves boards while it is down (`deploy/compose/compose.yaml:82`; agenthub-crate L19) (X3). |
| `0002-bound-board-offline-cache-and-queued-drafts.md` | 66 | Offline cache and pending queue | KEEP | Already notes ADR 0005 and increment 197. |
| `0003-hosting-pooled-free-per-instance-paid.md` | 61 | Pooled free, instance per paying customer | FIX | L32-35 "no tenancy code" in a paid instance conflicts with editions 2026-11-18 (X7). Operator decision. |
| `0004-optional-cache-dragonfly.md` | 35 | Optional Dragonfly cache | KEEP | Subject: cache. |
| `0005-agenthub-postgres-everywhere.md` | 69 | AgentHub on Postgres in every form | KEEP | Subject: AgentHub storage. |

## docs

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `agent-board.md` | 418 | The board contract | FIX | Subject: board contract. L4: owner `crates/hub/src/agenthub/` -> engine `crates/agenthub`, hub surface `crates/hub/src/agenthub/`. L5: product decisions owner -> `docs/product.md`. L42: `crates/core/journal` -> `crates/machine-engine/src/journal`. L47: `crates/hub/src/agenthub/tests.rs` is dead (the tests are in `crates/hub/src/agenthub/tests/`). L167 "Nineteen tools" and L362 "17 tools" -> the hub registers 24 `board_*` tools (`name = "board_` in `crates/hub/src`). L340 B01 "ClickHouse analytics is required" (X3). L106-110 error codes per B3. |
| `agenthub-benchmark-plan.md` | 100 | AgentHub benchmark spec | FIX | Subject: AgentHub benchmark. L4: "Delivery is P07 in the implementation and release plan" -> path.md 5.8 and 5.9. L70 links the performance spec, which folds into this file. Receives the fold of the performance spec. |
| `agenthub-factory-board-plan.md` | 139 | Factory board plan, rulings of 2026-11-09 | FOLD | Into `.ai/dev/plans/agenthub-factory.md`: rulings, decisions (L120-127) and "After this plan" (L133-139, cited by roadmap and cloud-edition). L5 cites `agenthub-research.md`, which is not in the repository. |
| `agenthub-issue-lifecycle-and-wardenmesh-plan.md` | 355 | Initial lifecycle and WardenMesh strategy | RETIRE | L3-4 says the release plan replaces it, and that plan retires too. WardenMesh is no longer the commercial name. |
| `auth.md` | 65 | Accounts and sign-in | KEEP | Subject: accounts, sign-in, tokens. Receives product-and-platform-plan §14 design detail. |
| `deploy.md` | 93 | Promoting a release-tarball hub | KEEP | Subject: `deploy/linux/promote-hub.sh`. |
| `desktop-commander-teardown.md` | 617 | Evidence-backed teardown of Desktop Commander | KEEP | Subject: parity reference. Its paths are upstream paths, not ours. |
| `desktop-commander-user-evidence.md` | 195 | Desktop Commander user reports | KEEP | Subject: user evidence. |
| `devagotchi/README.md` | 96 | Absorption ledger of the Go agent-hub | FIX | Subject: Go agent-hub ledger. L41-49: "the plan wins: product-and-platform-plan.md" -> `docs/product.md`. L15-16 ("Nothing has been copied yet") is still true. |
| `foundation-archive-readme.md` | 27 | README of the October source archive | RETIRE | Every link (`devagotchi/docs/...`) is dead. L3 "the current execution provider ... Linux only" is false. The audit JSON names it as the archive's `devagotchi-foundation/README.md` (`audit/planning-revision.json`, `audit/website-and-graph-revision.json`): move to `docs/history/`. |
| `identity.md` | 225 | Identity design (banner: superseded) | FOLD | The banner (L3-6) says nothing in it describes the code, yet the Machines section (L150-166) was updated for increments 209 and 210 and one-user-edition L82, L167, L180 cite it. Move that section into `crates/hub/README.md` ("Machines on the link", L320), then retire. Dead paths L8, L14, L24, L52, L64. |
| `implementation-and-release-plan.md` | 636 | "Canonical" implementation and release plan | RETIRE | L12: the commercial repository "does not exist" -> `shippinAI/devagotchi-cloud` exists. L171-173, L221-225: standalone and offline SQLite boards (ADR 0005). L170, L222: ClickHouse required (X3). L249-251: Dragonfly not selected -> ADR 0004. L213-215: no licence identified -> FSL-1.1-ALv2. L330: "Only the hub speaks to the network (CLAUDE.md)": CLAUDE.md does not say that; L331 `crates/hub/src/mail.rs` is dead (mail is `crates/hub/src/notify/mail.rs` and `web/email.rs`). Move: the R1 walkthrough (§5) into path.md Phase 0 proofs; the client matrix (§3) into `.ai/docs/adapters.md`; the mail section into `crates/hub/README.md`. |
| `implementation-guide.md` | 211 | October 17 execution handoff (P0, G0) | RETIRE | Dead paths L56-59 (`src/bin/bridge-broker.rs`, `src/execution.rs`, `tests/performance`). Cited by audit JSON and `verification/source-manifest.json`: move to `docs/history/`. |
| `performance-and-experience-spec.md` | 213 | Proposed workloads and budgets | FOLD | Workload matrix and budget rules (§4, §5, §8) into `agenthub-benchmark-plan.md` and path.md 5.8-5.9. Dead paths L18, L191. L5 defers to product-and-platform-plan. Cited by `verification/source-manifest.json` and the audit JSON, so after the fold move it to `docs/history/` instead of deleting it. |
| `plan-phone-and-signals.md` | 112 | Phone app and signals plan, 2026-11-06 | RETIRE | P1 built (mobile-experience L28), P3 built (`docs/signals.md`). Dead paths L20, L23, L50, L79. |
| `plan/hub-auth-and-identity.md` | 296 | Hub auth plan (banner: superseded) | RETIRE | Superseded (L3-6); twelve dead paths. |
| `product-and-platform-plan.md` | 548 | Architecture master: scope, decisions, phases | RETIRE | Replaced by `docs/product.md`. False today: L7 and D01 (L261) Windows/macOS/Linux required; §3 (L37-46) cites `src/execution.rs` and other dead paths; D21 (L281) "hosted by default"; D23 (L283) and L330 "live boards still use SQLite"; §15 (L532-538) `crates/broker`, `crates/core`, `hub/src/board/`, `hub/src/audit.rs` are dead; L544 says Platinum "is the Texas server", which the operator's own host map contradicts. Move: the live rulings D03 (one broker), D20 (one binary), D24 (link routes), D25 (built-in sign-in) and §14 decisions 1-8 into `docs/product.md` (a line each) or ADRs; §14 design detail into `docs/auth.md`. Cited by audit JSON: move to `docs/history/`. |
| `security-baseline.md` | 111 | Checklist every increment clears | FIX | Subject: security baseline. L4-7: "shippin.cloud on Gold first, DevPulse" and "§11a of the planning document" -> the editions in `.ai/docs/editions.md`; plan link -> `docs/product.md`. L17: `crates/hub/src/auth.rs` -> `crates/hub/src/identity/auth.rs`. L33: "org boards under `org-<id>/` (0700)" -> boards are tenant-scoped Postgres rows. L50: `migrations/postgres/0007_tenant_rls.sql` -> `crates/hub/migrations/0007_tenant_rls.sql`. |
| `signals.md` | 127 | Signal contract v1 | KEEP | Subject: signals. |
| `spec-first-connect.md` | 213 | First-connect spec, approved 2026-11-04 | RETIRE | Executed. Dead path L34. free-edition L20 "keeps" it: point that line to `docs/auth.md` and the CHANGELOG instead. `tests/first_connect_browser.py:2` also names it. |
| `spec-machine-update.md` | 112 | Machine update spec, approved 2026-11-05 | RETIRE | Executed (`machine_update`, path.md 2.3). Dead paths L15, L80, L85. Code and scripts name it in their headers: `crates/hub/src/config/machines.rs:2`, `crates/hub/src/machines/update.rs:2`, `crates/link/src/update/handoff.rs`, `crates/link/src/update/mod.rs`, `crates/link/src/update/update-windows.ps1`, `deny.toml:75`, `deploy/linux/ci-install-check.sh:365`, `deploy/linux/install-devagotchi.sh:203`, `:699`, `deploy/windows/ci-install-check.ps1:240`, `tests/e2e.py:298`, `tests/machine_update_probe.py:2`. Repoint them (G11) before deleting, or keep it as `docs/history/`. |
| `spec-phone-app.md` | 82 | Phone app spec P1 | RETIRE | P1a-P1d done (mobile-experience L28). Update mobile-experience L12 and mobile-navigation L10, and the headers that name it: `crates/hub/README.md:1148`, `crates/hub/src/web/app/mod.rs:1`, `tests/app_browser.py:2`. |
| `spec-ticket-anchors.md` | 68 | Ticket anchors spec P2 | FOLD | `crates/agenthub/src/anchors.rs` exists, so some of P2 landed. Check P2a-P2d against the code, move any open item into the factory plan, then retire. Not checked item by item here. |
| `task-graph-and-mesh-contract.md` | 191 | Proposed task graph and mesh contract | FIX | Subject: proposed graph and mesh records (unimplemented; factory F8b cites it). L15-19 and L172-174: `src/broker.rs`, `src/execution.rs`, `src/journal.rs`, `src/contracts.rs`, `src/ipc.rs` are dead -> `crates/machine-engine/src/`, `crates/contracts/src/`, `crates/ipc/src/`. L7 links product-and-platform-plan and the performance spec (X18). |

## apps

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `apps/desktop/README.md` | 135 | Tauri inspector | FIX | L5: "Current native execution provider: Linux"; "Windows ... providers ... pending" -> the broker has Linux and Windows providers (`crates/machine-engine/src/execution/platform/linux.rs`, `windows.rs`). L91: `src/contracts.rs` -> `crates/contracts`. The `[[bin]] name = "devagotchi"` collision (`apps/desktop/src-tauri/Cargo.toml:18-20`) is increment 260. |
| `apps/website/README.md` | 51 | Website and public reference service | FIX | Subject: the Sites deployment. L41: "The project license is not yet selected" -> FSL-1.1-ALv2 (`LICENSE`). L41: "No ... macOS/Windows execution providers ... are claimed" -> the Windows provider exists. |
| `apps/website/public/reference/task-graph-and-mesh-contract.md` | 191 | Served copy of the graph contract | FOLD | Byte-identical to `docs/task-graph-and-mesh-contract.md`. Generate it from docs at build time, or hold the two equal with a test. |

## crates

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `crates/connectors/README.md` | 16 | How to add an AI client | KEEP | Subject: connector catalog files. |
| `crates/hub/README.md` | 1198 | Hub operator reference | FIX | Subject: running and configuring the hub. L3-6: "routes the broker's own four handle-based tools" -> the hub serves the full machine tool set (README tool table). L9-13: the diagram names `mcp.shippin.cloud` and "audit: hash-chained SQLite" -> audit is in Postgres (free-edition R3). L22: `devagotchi-hub` binary -> `devagotchi hub serve` (one binary, `crates/devagotchi`). L79-81: "the older auxiliary hub SQLite stores (audit and devices) have not been migrated" -> migrated in increment 147 (free-edition R3). L503: `crates/store/migrations/postgres/` -> `crates/hub/migrations/`. L659: `analytics/report.rs` -> `ops/analytics/report.rs`. L1192-1198 "Not in H1" lists items built since. Receives the Machines section of `docs/identity.md` and the mail section of the release plan. |
| `crates/hub/site/default/content/docs.md` | 70 | Served Docs page | KEEP | Subject: public docs copy. |
| `crates/hub/site/default/content/llms.md` | 59 | Served llms.txt | KEEP | Subject: agent-facing index. |
| `crates/hub/site/default/content/privacy.md` | 102 | Served privacy page | KEEP | Subject: privacy copy. |
| `crates/hub/site/default/content/support.md` | 14 | Served support page | KEEP | Subject: support copy. |
| `crates/hub/site/default/content/terms.md` | 25 | Served terms page | KEEP | Subject: terms copy. |
| `crates/link/README.md` | 66 | The machine side of the link | KEEP | Subject: the link. Fixed in B2 (increment 271): a placeholder hub in the example, the `devagotchi link` command, the pattern instead of the retiring plan, the installers, Update and Layout. |

## deploy

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `deploy/compose/README.md` | 390 | Self-hosting with Docker Compose | FIX | Subject: the compose stack. L3: "The hub, its own Postgres, and Caddy" -> also ClickHouse (`compose.yaml:1`, `:53`). L126: "pass `--tag v0.1.0-alpha.1`" -> that tag was never cut (X10). L104, L123-127: channel default per X8. |
| `deploy/compose/site/README.md` | 11 | Placeholder for a custom site | KEEP | Subject: site mount. |
| `deploy/devspace/README.md` | 86 | Development devspace | KEEP | Subject: dev environment (AGENTS L67). Not checked against a live devspace. |
| `deploy/linux/README.md` | 92 | Linux server installer | FIX | Subject: `install-devagotchi.sh`. L3-4: "plan §11b, step 1" refers to the retiring plan. |
| `deploy/proxmox/README.md` | 53 | Hub in a Proxmox container | FIX | Subject: `hub-up.sh`. L28-29: channel default per X8. |
| `deploy/shippin/README.md` | 66 | Per-user broker in Shippin workspaces | FIX | L3-4: "Shippin.Cloud on Gold is the first hosted deployment; devagotchi.net is the second (plan §12, N11)" is October planning. L43 names `deploy/image/build-image.sh` and `deploy/image/ai/install.sh` in shippin-console, another repository; not checked here. Moves to the cloud repository at cloud-edition C6 (self-hosted-and-provenance L53-56). |

## plugins

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `plugins/README.md` | 53 | Installing the plugins | FIX | Subject: plugin install. L38: "The plugin uses the hosted hub" -> set your own hub (`devagotchi connect codex --hub URL`, one-user 211). L49: `crates/hub/src/plugin_tests.rs` -> `crates/hub/src/mcp/tests/plugins.rs`. L50-51: "the connector points at the hosted hub" -> `plugins/devagotchi/.mcp.json:5` names `https://hub.example.com/mcp`. |
| `plugins/devagotchi/skills/agenthub/SKILL.md` | 50 | AgentHub skill | KEEP | Subject: the board loop for agents. |
| `plugins/devagotchi/skills/machines/SKILL.md` | 42 | Machines skill | KEEP | Subject: machine tools for agents. |

## tests, audit, verification

| Path | Lines | What it claims to be | Verdict | Detail |
|---|---|---|---|---|
| `tests/fixtures/neutral-site/content/docs.md` | 23 | Test fixture | KEEP | Fixture data, not documentation. |
| `tests/fixtures/neutral-site/content/llms.md` | 21 | Test fixture | KEEP | Fixture data. |
| `tests/fixtures/neutral-site/content/privacy.md` | 12 | Test fixture | KEEP | Fixture data. |
| `tests/fixtures/neutral-site/content/support.md` | 7 | Test fixture | KEEP | Fixture data. |
| `tests/fixtures/neutral-site/content/terms.md` | 8 | Test fixture | KEEP | Fixture data. |
| `audit/README.md` | 22 | Desktop Commander audit evidence | KEEP | Subject: audit probes. |
| `verification/README.md` | 158 | Verification evidence index | FIX | Subject: recorded evidence. L86-90 name October binaries and `src/*.rs` paths: label them as that run's tree. L57: `tests/devspace_broker_probe.py` is dead. Receives the Sites receipts from README L5, L98-99, L327. |

## Contradictions

Each row names the documents, the lines, and what `docs/product.md` should say. "Operator"
means the code and the plans do not settle it.

| # | Subject | Documents that disagree | Resolution |
|---|---|---|---|
| X1 | Platforms | README L5, L7 and product-and-platform-plan L7, D01 (L261), §6 say Windows, macOS and Linux are required. foundation-archive-readme L3 and apps/desktop/README L5 say Linux only. apps/website/README L41 claims no Windows provider. README L37 says macOS "not supported yet". path.md 8.2 (L160): hosted on Linux, likely arm64; Windows, Android, Linux, arm64 supported. one-user-edition L902: hub on Linux x86_64 or arm64; machines Linux or Windows. `dist-workspace.toml:13` builds Linux and Windows on x86_64 and aarch64. | The hub runs on Linux, x86_64 or arm64 (on a Windows computer, in a VM, WSL or the same Docker stack: path.md 2.4, one-user-edition L902). Machines run Linux or Windows, x86_64 or arm64. The phone app is the installed web app, Android first. macOS is not a target today. Android as a machine is open (path.md decision 2). path.md 8.2 asks for a test that holds this statement to the build targets. |
| X2 | Where SQLite remains | README L133 ("Postgres ... or SQLite"); release plan L171-173, L193-194, L221-225; product-and-platform-plan D23 (L283), L330; ADR 0001 L29-31, L63-66; crates/hub/README L79-81; agent-board B11 (L349). Against: ADR 0005, ADR 0002 note (L7-14), path.md decision 1, `crates/store/Cargo.toml` with no SQLite dependency. | The hub, the link and every board are Postgres. SQLite remains only in the machine engine's operation journal and the kernel's capture journal, on each machine (path.md decision 1, still the operator's to confirm). The offline client queue of ADR 0002 is not built. |
| X3 | ClickHouse required or optional | ADR 0001 L8, L32; release plan L170, L222; agent-board B01 (L340); product-and-platform-plan §13 say required. agenthub-crate L19 ("ClickHouse as a requirement" is a non-goal) and path.md 4.8 say optional. The compose stack ships it (`compose.yaml:53`) and the hub serves boards while it is down (`compose.yaml:82`). | ClickHouse is part of the compose stack and optional for the hub: the hub serves without it and reports analytics degraded. Amend ADR 0001. |
| X4 | Product order | release plan (R1, R2, R3; P00 to P12; 2026-11-10); product-and-platform-plan (P0 site, G0 to G5); roadmap.md L32-37 (core lane, and a cloud lane that "can start now"); free-edition L188-201; path.md (phases 0 to 9, cloud after Phase 9, L52-55, L174); one-user-edition (phases 0 to 5 by the operator's 2026-11-18 priority). roadmap L13 already defers to path.md; plans/README L23 and AGENTS L17 still point at the release plan. | path.md decides order and "done"; one-user-edition.md orders the increments inside it; the cloud waits until path.md is proven. docs/product.md states this once and links both. Retire the release plan, product-and-platform-plan and roadmap.md as order documents. |
| X5 | Default distribution | product-and-platform-plan D21 (L281): "hosted by default", a free account required; §13 tier table. editions 2026-11-18 (L57-70) and path.md L39-55: the standalone GitHub release first, free, one user, self-hosted; D25 (L285): built-in sign-in, Nomen optional. | The standalone self-hosted release comes first. The hosted service is the Shippin build, later. |
| X6 | Commercial repository and name | release plan L11-13 ("WardenMesh", the repository "does not exist"); AGENTS L36 ("WardenMesh"); issue-lifecycle plan. editions L9, L29-31 and cloud-edition L120: `shippinAI/devagotchi-cloud`, made 2026-11-14. cloud-edition L135 says it does not exist. one-user-edition L52: the commercial version is powered by Shippin.AI, built by wiredFRONT LLC and AngryVibes LLC. | `shippinAI/devagotchi-cloud` exists. WardenMesh is retired as a name. Fix cloud-edition L135 and AGENTS L36. |
| X7 | Managed deployments and tenancy | ADR 0003 L32-35 and cloud-edition L40: a paid or managed instance runs no tenancy code. editions L67-70 (2026-11-18): managed deployments offer multi-tenancy. path.md decision 9 proposes a reconciliation. | Operator. Proposed in path.md decision 9: a managed instance runs the multi-tenant build when its customer wants accounts of its own. |
| X8 | Self-hosted update channel | self-hosted-and-provenance S1 (L39), deploy/compose/README L104, L123-127 and deploy/proxmox/README L28-29: `:latest`. alpha-stage-obvious M2 and texas-baseline M2 (L36): `:main` by default. path.md 0.1 (L67): pass `--channel main` until a stable tag. one-user-edition decision 1 (L915-918): keep S1 and cut v0.1.1. | Operator (one-user-edition decision 1). Until then the deploy READMEs say what the scripts do today. |
| X9 | Tenancy in the default build | cloud-edition L123-126 (2026-11-11 ruling: tenancy stays in core, superseded); editions L8 and free-edition R4: tenancy becomes its own crate out of the default build (not started). Increment 225 hides the multi-user pages at runtime on a one-user hub. | The one-user hub hides the multi-user surfaces now (225); R4 still has to take tenancy out of the binary. |
| X10 | The first release tag | roadmap D3 and cloud-edition C1: tag `v0.1.0-alpha.1`. CHANGELOG L230, L239 and deploy/compose/README L126 still name it. v0.1-release-readiness L30-35, free-edition L203-208 and README L17-22: the operator re-cuts `v0.1.0`; alpha.1 was never tagged. one-user-edition decision 1 (L917): never re-cut v0.1.0, since machines would answer `already_on`; cut v0.1.1. | Operator. docs/product.md records the decision once; the other documents link it. |
| X11 | AgentHub licence | The main repository is FSL-1.1-ALv2 (LICENSE, editions L7). path.md 4.9 and decision 3: the AgentHub submodule is still BUSL-1.1 (`crates/agenthub/LICENSE`). | Operator (path.md decision 3). |
| X12 | Superseded identity documents still edited | docs/identity.md L3-6 and docs/plan/hub-auth-and-identity.md L3-6 say they describe nothing current. one-user-edition L82, L167, L180 list docs/identity.md as a file to change, and its L150-166 is current. | Move the Machines section to crates/hub/README; docs/auth.md is the sign-in reference; retire both. |
| X13 | Host identity | product-and-platform-plan L544 says Platinum "is the Texas server". The operator's own host map says they are different hosts. | Delete the line with the document. No document should name which host is which. |
| X14 | Desktop Commander tools Devagotchi lacks | free-edition L39 lists `list_processes`, `kill_process`, `get_config`, `set_config_value`, `get_recent_tool_calls` as missing; path.md 0.6 lists tools the hosted hub lacks. `.ai/docs/desktop-commander-parity.md` (L17-46) and one-user-edition L35-48 say they are present. | `.ai/docs/desktop-commander-parity.md` is canonical. |
| X15 | Which hub the plugins point at | plugins/README L38, L50-51 and plugins.md L37: the hosted hub. plugins.md G15 (L77) and one-user 211: no hostname is a default. `plugins/devagotchi/.mcp.json:5` names `https://hub.example.com/mcp`. | No default hub; the person sets theirs. Fix plugins/README and plugins.md L37. |
| X16 | Where contracts live | AGENTS L39 and .ai/docs/capture.md L8: `crates/contracts/src/lib.rs`. The tree has family modules; the patterns index C11 and G10 make that the rule. | Family modules re-exported from lib.rs (B0 text for AGENTS L39). |
| X17 | Number of board tools | agent-board L167 ("Nineteen tools") and L362 ("17 tools"). The hub registers 24 `board_*` tools. | Do not state a count in prose; name the router, or let `tools/list` answer. |
| X18 | Which document owns product decisions | AGENTS L65, agent-board L5, performance spec L5, devagotchi/README L44, .ai/patterns/README L17 (and the new README's G14.6), security-baseline L6, task-graph-and-mesh-contract L7 (and its website copy), the root `llms.txt` L17, README L106 and L120 all name product-and-platform-plan as the master. | `docs/product.md`. Change each link when product-and-platform-plan retires. |
| X19 | Standalone scope versus the one-user order | path.md L39-51: step 1 is Desktop Commander, AgentHub limited, the plugins, installed from the release. one-user-edition L4-13: the operator's order is Desktop Commander tools, Shippin sign-in and a management section, the update path, Android, then the popup, email approvals and plugins; L57-62 brings the plugins forward after M1. | Not a conflict of content, only of order. docs/product.md states the scope (path.md) and the order (one-user-edition.md, including the plugins-forward clarification). |
| X20 | Increment 211 status | one-user-edition 211 plans the README fix and a `readme_truth.rs` test. At 2b3feec README L5, L7, L133 are unchanged and the test does not exist. | 211 is open. This map's README FIX list is that work. |

## Proposed structure

### One product description

- `docs/product.md` (new): what Devagotchi is, for whom, the three builds, platforms (X1),
  storage (X2, X3), what ships first and the order (X4, X5, X19), the open operator decisions
  (X7, X8, X10, X11), and the still-live rulings from product-and-platform-plan (D03, D20,
  D24, D25 and §14 decisions 1-8), one line each with a link.
- README.md keeps install, update, undo, build, the tool table and the licence. Its opening
  paragraph becomes two sentences from docs/product.md and a link.

### Documents that remain, by subject

| Subject | Canonical document |
|---|---|
| Product, platforms, order summary | `docs/product.md` |
| Editions, builds, licence | `.ai/docs/editions.md` |
| Phase order and what "done" means | `.ai/dev/plans/path.md` |
| One-user edition increments | `.ai/dev/plans/one-user-edition.md` |
| Agent instructions and AI contracts | `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md`, `.ai/README.md`, `.ai/dev/workflow.md`, `.ai/docs/lifecycle.md`, `capture.md`, `adapters.md`, `retry-lifecycle.md`, `cross-project-training-standard.md` |
| Code patterns | `.ai/patterns/` as rewritten in B0 |
| Desktop Commander parity | `.ai/docs/desktop-commander-parity.md`; reference: `docs/desktop-commander-teardown.md`, `docs/desktop-commander-user-evidence.md`, `audit/README.md` |
| Board contract | `docs/agent-board.md`; storage: ADRs 0002, 0005 |
| Sign-in and accounts | `docs/auth.md` |
| Signals | `docs/signals.md` |
| Security baseline | `SECURITY.md`, `docs/security-baseline.md` |
| Hosting and cache decisions | ADRs 0001 (amended), 0003, 0004 |
| Proposed graph and mesh | `docs/task-graph-and-mesh-contract.md` (the website copy generated from it) |
| Benchmarks | `docs/agenthub-benchmark-plan.md` (with the performance spec folded in) |
| Operating a hub | `crates/hub/README.md` (later split per domain), `docs/deploy.md`, `deploy/compose/README.md`, `deploy/linux/README.md`, `deploy/proxmox/README.md`, `deploy/devspace/README.md`, `deploy/compose/site/README.md` |
| Machines and link | `crates/link/README.md` |
| Clients and plugins | `plugins/README.md`, the two skills, `crates/connectors/README.md`, `.ai/patterns/connectors.md` |
| Inspector and website | `apps/desktop/README.md`, `apps/website/README.md` |
| Served pages | `crates/hub/site/default/content/*.md` |
| Evidence and history | `CHANGELOG.md`, `verification/README.md`, `audit/README.md`, `docs/devagotchi/README.md` (Go ledger) |
| Plans that remain (open, or holding live decisions: kernel.md's K1-K5 are done) | `.ai/dev/plans/`: agenthub-crate, agenthub-factory, ai-sessions, browser-experience, cloud-edition, connections-gateway, dormancy-and-reauth, free-edition, hub-settings, kernel, landing, mobile-experience, plugins, self-hosted-and-provenance, structure-cleanup, texas-baseline, vibe-coder, plus README and TEMPLATE |

### Fold

| From | Into |
|---|---|
| `.ai/dev/plans/connectors.md` | `.ai/patterns/connectors.md`, `crates/connectors/README.md` |
| `.ai/dev/plans/desktop-commander-parity.md` | `.ai/docs/desktop-commander-parity.md`, one-user increments 212-224 |
| `.ai/dev/plans/roadmap.md` | `cloud-edition.md` (gate, D1-D5); plans/README (track list) |
| `.ai/dev/plans/v0.1-release-readiness.md` | factory F22 (M5); docs/product.md (tag decision) |
| `docs/agenthub-factory-board-plan.md` | `.ai/dev/plans/agenthub-factory.md` |
| `docs/identity.md` (Machines section) | `crates/hub/README.md` |
| `docs/performance-and-experience-spec.md` | `docs/agenthub-benchmark-plan.md`, path.md 5.8-5.9 |
| `docs/spec-ticket-anchors.md` (open items only) | `.ai/dev/plans/agenthub-factory.md` |
| `docs/implementation-and-release-plan.md` (R1 walkthrough, client matrix, mail) | path.md Phase 0 proofs, `.ai/docs/adapters.md`, `crates/hub/README.md` |
| `docs/product-and-platform-plan.md` (live rulings, §14) | `docs/product.md`, `docs/auth.md` |
| `apps/website/public/reference/task-graph-and-mesh-contract.md` | generated from `docs/task-graph-and-mesh-contract.md` |

### Retire

Delete (git keeps them): `.ai/dev/plans/agenthub-module.md`, `ai-development-capture.md`,
`alpha-stage-obvious.md`, `api-keys.md`, `group-settlement.md`, `license.md` (after its M4
moves), `mirror-fallback.md`, `mobile-navigation.md`, `notes-that-reach-them.md`,
`phone-fit.md`, `structure-cleanup-map.md`; `docs/agenthub-issue-lifecycle-and-wardenmesh-plan.md`,
`docs/plan-phone-and-signals.md`, `docs/plan/hub-auth-and-identity.md`,
`docs/spec-first-connect.md`, `docs/spec-machine-update.md` (after the code headers that name
it are repointed), `docs/spec-phone-app.md`, and, after their folds, `docs/identity.md` and
`docs/implementation-and-release-plan.md`.

Move to `docs/history/`, because files under `audit/` or `verification/` cite them:
`desktop-bridge-audit-and-foundation.md`, `docs/foundation-archive-readme.md` (cited as the
archive's `devagotchi-foundation/README.md`), `docs/implementation-guide.md`,
`docs/product-and-platform-plan.md`, and, after its fold,
`docs/performance-and-experience-spec.md`.

### Links to change when the retirements land

AGENTS L17, L65; `.ai/README.md` (patterns rows); `.ai/patterns/README.md` L17 and the new
README's G14.6; `.ai/docs/lifecycle.md` L38; `.ai/docs/desktop-commander-parity.md` L12;
`.ai/dev/plans/README.md` L12, L16-23; `agenthub-crate.md` L20; `agenthub-factory.md` L13-15,
L18; `ai-sessions.md` L16; `browser-experience.md` L13; `cloud-edition.md` L12-13;
`connections-gateway.md` L16-17; `kernel.md` L19; `mobile-experience.md` L12-13;
`mobile-navigation.md` L10; `one-user-edition.md` L25, L82, L167, L180, L311; `path.md` L161,
L171; `plugins.md` L16; `self-hosted-and-provenance.md` L19; `free-edition.md` L18, L20;
`structure-cleanup.md` L15, L18; `docs/agent-board.md` L5; `docs/agenthub-benchmark-plan.md`
L4, L70; `docs/devagotchi/README.md` L44; `docs/security-baseline.md` L6;
`docs/task-graph-and-mesh-contract.md` L7 and its website copy; the root `llms.txt` L17;
`crates/hub/README.md` L1148; `README.md` L106, L120. Code and script headers that name
`docs/spec-machine-update.md`, `docs/spec-phone-app.md` and `docs/spec-first-connect.md` are
listed in those rows above.
