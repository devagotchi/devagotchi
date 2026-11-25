# Devagotchi: what is being built

> ALPHA = PRE-RELEASE. Read [.ai/README.md](../.ai/README.md) first. The stage is `alpha` in
> [.ai/project.json](../.ai/project.json). Nothing is in production. The hub at
> app.devagotchi.com is a test deployment.

This page is the one description of the product. Every other document agrees with it. Where
another document says something different, this page holds and the other document is fixed.
The product statement below is the operator's, given on 2026-11-18. The status table was
checked against the code and tests at `main` 2b3feec.

## The product

Devagotchi is the central point for a solo developer. It has four parts, joined by a hub
that the developer runs for themselves.

1. **A self-hosted Desktop Commander.** Claude, ChatGPT and Codex can run commands, read and
   write files, manage processes and use every other capability on the developer's own
   machines. The calls go through the developer's own hub. A broker on each machine does the
   work.
2. **AgentHub.** A ticket board that AI agents share. Agents claim tickets, keep notes and
   hand work off. A person approves the steps that need a person.
3. **An Android phone app.** The developer sees their machines, boards and approvals and
   answers approvals from the phone.
4. **Plugins** for Claude Code, Codex and ChatGPT, VS Code and Claude Desktop. Each installs
   once. After that it raises notifications from the hub and tells the developer when an
   update is available.

## Where it runs

| Part | Platforms |
|---|---|
| The hub | Linux only, x86_64 and arm64 |
| Machines (the broker and its link to the hub) | Windows, Linux and macOS, x86_64 and arm64 |
| The phone | Android |

The hub's image is built for `linux/amd64` and `linux/arm64` (`.github/workflows/image.yml`).
CI's `e2e` job also starts a hub on Windows to drive its probes. That is a test setup, not a
supported platform.

macOS is a target, but it does not work today. See the table under "What works today".

## Editions

1. **The free self-hosted / indie edition**, released on GitHub. Each installation is one
   environment with its own owner and local users. It does not host multiple customer
   tenants. Local user permissions belong to this edition.
2. **Managed deployments.** The same release, run for a customer.
3. **The commercial cloud**, powered by Shippin.AI and built by wiredFRONT LLC and AngryVibes
   LLC. Later it becomes the public doorway: Cloudflare, Tailscale, Vaultix secrets, and
   integrations with Lovable, Replit and any AI project. The commercial module is separate
   and comes later, with a free tier and paid tiers. It reuses the accepted self-hosted
   release and adds public hosting and commercial optimizations.

The line between core, multi-tenant and cloud code is kept in
[.ai/docs/editions.md](../.ai/docs/editions.md).

The operator clarified this boundary on 2026-11-20: **one environment is not one account**.
This supersedes the 2026-11-18 restriction against managing local users. Development must
exercise the self-hosted Docker deployment, with a deployment-local test owner credential;
that credential is never a default in the distributed image or source. Branch integration
and the deployment acceptance are tracked in
[release readiness](../.ai/dev/plans/v0.1-release-readiness.md#self-hosted-baseline-2026-11-20).
These are requirements, not a claim that local-user management or deployment is complete.

## Repositories

| Repository | What it holds |
|---|---|
| `devagotchi/devagotchi` (this one) | The hub, the broker, the link, the local MCP adapter, the phone app's pages, the plugins |
| `devagotchi/agenthub` | AgentHub. It is a git submodule at `agenthub` and keeps its own migration history |
| `shippinAI/devagotchi-cloud` (private) | The commercial cloud. Later. No work happens there until every phase of [the path](../.ai/dev/plans/path.md) is proven (its non-goals) |

## Licence and stage

- **Licence:** FSL-1.1-ALv2 ([LICENSE](../LICENSE)). Each version becomes Apache-2.0 on the
  second anniversary of its release.
- **Stage:** alpha, which means pre-release ([.ai/README.md](../.ai/README.md)).

## What works today

Status words come from [the path](../.ai/dev/plans/path.md):

- **built**: code and a test exist;
- **partial**: some of it is built, and the gap is named;
- **absent**: no code.

The path counts a feature as **done** only when it works on Texas and on the operator's phone.
At 2b3feec the plans record no such receipt for any row below. "Built" here means a test
exists and CI is set up to run it. It does not mean this check ran the test.

| Capability | Status | Evidence at 2b3feec |
|---|---|---|
| Commands, files and processes through the hub, on Linux machines | built | `tests/e2e.py` drives `tests/adapter_probe.py` (local MCP) and `tests/hub_probe.py` (through the hub). The CI `e2e` job runs it on `ubuntu-24.04` and `ubuntu-24.04-arm` (`.github/workflows/ci.yml`). Gaps: reading PDF, DOCX and XLSX, Office writes, and `write_pdf` ([one-user edition](../.ai/dev/plans/one-user-edition.md) increments 221, 222 and 223; [parity](../.ai/docs/desktop-commander-parity.md)) |
| The same, on Windows machines | built | Provider in `engine/machine-engine/src/execution/platform/windows.rs`. CI runs `tests/e2e.py` with the hub on `windows-2022` and `windows-2025`. `windows-11-arm` runs it with `--no-hub` |
| macOS machines | absent | `spawn` in `engine/machine-engine/src/service/background.rs` and `serve` in `engine/machine-engine/src/service/mod.rs` return `unsupported_platform` on any OS that is not Linux or Windows. `engine/machine-engine/src/execution/platform/` has providers only for Linux (`linux`) and Windows (`windows`). `dist-workspace.toml` `targets` lists no Apple target. CI `core` on `macos-latest` builds the workspace and runs its tests, but no broker runs there: the broker binary reports `unsupported_platform` |
| Docker machine join | built | `engine/hub/src/machines/join/compose.yml`, served by `engine/hub/src/machines/join/compose.rs`. Test `a_container_is_never_asked_to_update_and_is_told_how_to_move` (`engine/hub/src/machines/link/tests/container.rs`). The Docker leg of `tests/compose_join.py` (`--docker`) runs only in `.github/workflows/image.yml` |
| Self-hosted users share one environment | local-user administration implemented; release acceptance tracked separately | `web::people` lets the owner add, edit, disable and reset local users through existing PostgreSQL accounts. `web::one_user` hides organization/invite pages and APIs in single mode. Domain and HTTP tests cover authority, versions and session revocation; hosted build separation remains open in [free edition](../.ai/dev/plans/free-edition.md) R4. |
| AgentHub board: claim, notes, handoff, approvals | built | `agenthub/src/tests/hosted.rs` `native_board_graph_filters_versions_handoff_and_approval_rules`; `agenthub/src/tests/approvals.rs` `an_approval_waits_for_a_person_and_records_the_verdict`. The hub serves the board tools from `engine/hub/src/agenthub/mcp/`. `tests/e2e.py` runs `tests/board_probe.py` |
| Board settings and workflow forms | built; development Docker accepted | Owners/admins edit lease/task defaults, states, work stages and manual moves through versioned PostgreSQL methods. Stale saves retain the draft; standing approvals and used-state meanings are preserved. PR277 passed CI and official-image browser/restart acceptance ([configuration plan](../.ai/dev/plans/october-board-configuration.md)). Shared catalogs and bindings are being integrated separately ([shared-workflow plan](../.ai/dev/plans/october-shared-workflows.md)). |
| AgentHub as a software factory | mostly absent | [agenthub-factory](../.ai/dev/plans/agenthub-factory.md), milestone table: F12 done, F5a built, F1, F2 and F5 partial. Not started: F1b, F3, F4, F4b, F5b, F5c, F6, F8, F8b, F9, F10, F11 and F13 to F23 (completion gated on checks, plan before code, proof of work, review agents, the notification outbox, the project overview, and the rest) |
| Android phone app, installable | built, never on a real phone | `engine/hub/src/site/pwa.rs`. `tests/android_browser.py` claims a one-user hub on an emulated Pixel 7 in the runner's Chrome, driven by Playwright (`--channel chrome`), and checks `Page.getInstallabilityErrors`; CI runs it on `ubuntu-24.04`. No real-device test exists yet (increment 253) |
| Web push to the phone | built, tested only against a fake push service | `engine/hub/src/notify/push/mod.rs`. Test `notify_this_phone_then_an_approval_buzzes_it_encrypted` (`engine/hub/src/web/tests/mod.rs`) sends to a fake push service. Nothing has been sent through a real push service |
| Approval email | absent | The hub's mailer (`engine/hub/src/web/email.rs`, over `engine/hub/src/notify/mail.rs`) sends sign-in links, the test message from `/admin` and the early-access "let in" notice. It sends no approval email. Increment 259 |
| Desktop notifications | absent | There is no `devagotchi watch` (262) and no companion popup (263). The hooks plugin (`plugins/devagotchi-agenthub/hooks/hooks.json`) has `SessionStart`, `PostToolUse`, `Stop` and `SessionEnd` only, with no notification hook (264) |
| Claude Code plugin | partial: installs, points at a placeholder hub | Marketplace `.claude-plugin/marketplace.json`. `plugins/devagotchi/.mcp.json` points at `https://hub.example.com/mcp`, and the `hub_url` default in `plugins/devagotchi/.claude-plugin/plugin.json` is `https://hub.example.com`. Test `the_connector_names_a_placeholder_hub_and_claude_names_its_seat` (`engine/hub/src/mcp/tests/plugins.rs`). [plugins](../.ai/dev/plans/plugins.md) records an install into a throwaway config on 2026-11-13, made when the plugin still pointed at app.devagotchi.com, and no signed-in session (G7) |
| Codex plugin | partial: installs, cannot set the hub | `plugins/devagotchi/.codex-plugin/plugin.json` has no `userConfig`. Increment 211 |
| VS Code extension | absent | No VS Code extension exists in `plugins/` (increment 268). `engine/connectors/catalog/microsoft/vscode-copilot.json` is connect steps for VS Code's MCP config, not an extension |
| ChatGPT Desktop | absent | Increment 269. `engine/connectors/catalog/openai/chatgpt.json` is the web surface (`"surface": "web"`) |
| Claude Desktop | partial | `devagotchi setup` writes the MCP entry into Claude Desktop's config ([README](../README.md), "Install: one line, for you or your AI"). The local package (`.mcpb`, increment 269) does not exist |
| Update notice in the UI | absent | The hub has no release check (increment 240) and no update notice or button (241). The plugins' notice (270) is not built either. Machines already follow the hub's build (`tests/machine_update_probe.py`, run by `tests/e2e.py` on Linux when it is given two builds with `--update-builds`) |
| Sign in with Shippin | partial: the card exists, untested end to end | The card is one of the Sign-in cards on `/admin`, in `engine/hub/src/web/sign_in_cards.rs`. Tests `the_switch_hides_shippin_and_keeps_the_install_secret` and `a_fresh_hub_shows_shippin_off_until_an_issuer_is_saved` (`engine/hub/src/web/shippin_tests.rs`). `tests/nomen_probe.py` signs in against a fake Nomen, but only in the hub's `nomen` auth mode. No test signs in through the card on a one-user hub (226). Nothing has signed in through the real Shippin issuer |

## What the code is

Measured at 2b3feec, with the `agenthub` submodule at its pinned commit 5c1499c.

How it was measured:

- Every file from `git ls-files`, plus the submodule's own `git ls-files`.
- Lines counted as `wc -l` counts them, blank lines and comments included.
- A **test** line is in a `tests/` directory, a `tests.rs`, a `*_tests.rs`, or the repository's
  top-level `tests/`. In any other Rust file, everything from a `#[cfg(test)]` line directly
  followed by a `mod` line, to the end of the file, is test.
- Counted: Rust under `crates/`; TypeScript, JavaScript, CSS and HTML under `apps/`;
  JavaScript and CSS under `engine/hub/src/`; and the Python under the top-level `tests/`,
  reported on its own.
- Not counted: `reference/` (the upstream Desktop Commander copy), generated files (`*.d.ts`,
  `apps/desktop/src/generated/`, `apps/desktop/src-tauri/gen/`), SQL migrations, lockfiles,
  SVG, the scripts under `scripts/` and `audit/`, and JSON, Markdown, YAML, TOML, shell and
  PowerShell files. The hub's site words (`engine/hub/site/`) are JSON, so they are not
  counted.

### Rust, per crate

| Crate | Non-test lines | Test lines |
|---|---:|---:|
| `engine/hub` | 54,214 | 37,002 |
| `agenthub` | 9,275 | 5,080 |
| `engine/machine-engine` | 8,759 | 4,967 |
| `engine/kernel` | 2,746 | 2,693 |
| `interfaces/mcp` | 2,444 | 1,016 |
| `platform/contracts` | 1,784 | 266 |
| `runtime/link` | 1,310 | 399 |
| `platform/ipc` | 1,175 | 158 |
| `runtime/devagotchi` | 843 | 2,212 |
| `platform/store` | 729 | 127 |
| `engine/connectors` | 83 | 445 |
| **Total** | **83,362** | **54,365** |

Other code, non-test: `apps/website` 1,723 lines, `apps/desktop` 1,245 (its Tauri Rust
included), `apps/turnkey-video` 736. The Python tests under `tests/` are 10,208 lines.

### The hub, per module

Non-test lines of Rust, JavaScript and CSS under `engine/hub/src/`. The Rust here is 54,208
lines; the other 6 of the crate's 54,214 are `engine/hub/build.rs`.

| Module | Lines | What it is |
|---|---:|---|
| `web` | 19,133 | Every page and form, split across the areas below |
| `identity` | 7,221 | Accounts, sign-in, sessions, the OAuth server, API keys |
| `site` | 4,804 | The site's words, brand, discovery files and the app manifest |
| `machines` | 3,609 | Machines, their link, join and update |
| `agenthub` | 3,493 | How the hub serves AgentHub: MCP tools, registry, issue lifecycle |
| `tenancy` | 3,232 | Organisations, members, plans, the operator |
| `config` | 2,651 | Hub configuration and live settings |
| `ops` | 1,776 | Audit chain, analytics, ClickHouse statistics |
| `workflows` | 1,746 | One-link workflows such as "connect my server" |
| `login` | 1,608 | `devagotchi login` (device sign-in) |
| `mcp` | 1,546 | The hub's `/mcp` surface and machine tools |
| `kernel` | 1,244 | The lexicon and dictionary tools |
| `notify` | 991 | Web push, email, signals |
| `offers` | 959 | The site's network offers |
| `connections` | 784 | Outside services called through the hub |
| `serve`, `cli`, `extensions`, `store`, `lib`, `test_keys` | 1,232 | The running hub and its command line |
| **Total** | **56,029** | |

### Share by product area

All counted code, non-test: 88,887 lines. That is the Rust crates, the three apps, and the
hub's JavaScript and CSS. The Python tests are not in it.

| Area | Lines | Share | What is counted |
|---|---:|---:|---|
| Desktop Commander core | 27,347 | 30.8% | `machine-engine`, `ipc`, `link`, `mcp`, `contracts`, `connectors`, `runtime/devagotchi`, `apps/desktop`; the hub's `machines`, `mcp` and `workflows`, and the machine, connect and workflow pages |
| Accounts and sign-in | 16,320 | 18.4% | The hub's `identity` and `login`, the sign-in, passkey, OAuth, email-link, claim and device pages, the account and API-key pages, sign-in config, mail |
| AgentHub | 14,425 | 16.2% | `agenthub`, the hub's `agenthub`, the approvals page, the board and ticket pages |
| Shared plumbing | 8,331 | 9.4% | `platform/store`, the hub's `serve`, `cli`, `store`, page shell, site words and style, audit, health, signals, the rest of `config` |
| Multi-tenant and commercial | 6,910 | 7.8% | The hub's `tenancy`, `offers`, `extensions.rs`, the organisation, operator, offers and hosted pages, tenancy config |
| Marketing site | 6,794 | 7.6% | `apps/website`, `apps/turnkey-video`, the hub's landing and public pages, discovery files, and the visit counts behind them |
| Kernel (lexicon, dictionary, AI capture) | 3,990 | 4.5% | `engine/kernel`, the hub's `kernel` |
| Admin (`/admin`) | 2,100 | 2.4% | The `hub_settings*` pages, live settings, the one-user switch |
| Phone | 1,625 | 1.8% | The `/app` shell and home, web push, the service worker, the app manifest |
| Connections gateway | 1,045 | 1.2% | The hub's `connections` and its page |

The phone's own share is small because the phone shows pages that belong to other areas. The
board and ticket pages count as AgentHub, the machine page as Desktop Commander, and the
account pages as accounts.

## Where things are documented

| Subject | Canonical document |
|---|---|
| What is being built (this page) | `docs/product.md` |
| How every agent works in alpha | [.ai/README.md](../.ai/README.md), [AGENTS.md](../AGENTS.md), [lifecycle](../.ai/docs/lifecycle.md) |
| Core, multi-tenant and cloud; the licence | [.ai/docs/editions.md](../.ai/docs/editions.md) |
| The order of work, and what "done" means | [.ai/dev/plans/path.md](../.ai/dev/plans/path.md) |
| The one-user edition's increments (208 to 269, and 270 from the operator's later clarification) | [.ai/dev/plans/one-user-edition.md](../.ai/dev/plans/one-user-edition.md) |
| Desktop Commander tools and gaps | [.ai/docs/desktop-commander-parity.md](../.ai/docs/desktop-commander-parity.md), [plan](../.ai/dev/plans/desktop-commander-parity.md) |
| The AgentHub board contract | [docs/agent-board.md](agent-board.md), [agenthub/README.md](../agenthub/README.md) |
| AgentHub as a software factory | [.ai/dev/plans/agenthub-factory.md](../.ai/dev/plans/agenthub-factory.md) |
| The phone | [.ai/dev/plans/mobile-experience.md](../.ai/dev/plans/mobile-experience.md), [docs/spec-phone-app.md](spec-phone-app.md) |
| Plugins | [.ai/dev/plans/plugins.md](../.ai/dev/plans/plugins.md), [plugins/README.md](../plugins/README.md) |
| Sign-in and identity | [docs/identity.md](identity.md) |
| Security | [docs/security-baseline.md](security-baseline.md) |
| Running a hub | [docs/deploy.md](deploy.md), [deploy/compose/README.md](../deploy/compose/README.md), [engine/hub/README.md](../engine/hub/README.md) |
| The free and cloud editions' plans | [free edition](../.ai/dev/plans/free-edition.md), [cloud edition](../.ai/dev/plans/cloud-edition.md) |
| How code is written | [.ai/patterns/README.md](../.ai/patterns/README.md) |
| Decisions | [docs/adr/](adr/) |
| What changed | [CHANGELOG.md](../CHANGELOG.md) |
