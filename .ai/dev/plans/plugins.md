# Devagotchi as a plugin for Claude, Codex and Grok

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-12: "devagotchi is supposed to be able to easily provide
  updates and extend shared state and integrate the agenthub into claude and codex and grok as
  a plugin; the connector is the MCP transport boundary; the plugin provides local UI to be
  triggered by the API".
- PR: not_created
- Branch / baseline: `increment-136/plugins`, from `main` at `a8223f4`
- Objective: one install per client gives it the Devagotchi connector, the skills that teach the
  AgentHub loop and the machines, and (where the client can) local surfaces the hub's API drives:
  hooks that keep work honest, views and live notices. Updates arrive through each client's own
  plugin update; shared state stays on the hub, the one place every client reads and writes.
- Non-goals: a second transport (the connector, MCP over HTTP with OAuth, is the only way in);
  state kept in the plugin; anything that bypasses a client's own trust prompts for hooks.
- Canonical architecture / related plan: [roadmap](roadmap.md), [factory](agenthub-factory.md)
  (F11 live activity), [AI sessions](ai-sessions.md) (S6 told when it matters),
  [adapters](../../docs/adapters.md), [agent board §6](../../../docs/agent-board.md).
  Code: `plugins/`, `.claude-plugin/marketplace.json`, `.agents/plugins/marketplace.json`,
  `crates/hub/src/agenthub/cli.rs` (`plugin_hooks`, `plugin_host`), `crates/hub/src/plugin_tests.rs`.

## The boundary

| Layer | What it is | Where it lives |
|---|---|---|
| Connector | The hub's MCP endpoint, `https://<hub>/mcp`, OAuth (dynamic client registration at `/oauth/register`). Every tool, every write, every audit entry | the hub |
| Shared state | Boards, tickets, notes between agents, approvals, machines, operations | the hub's store |
| Plugin | What the client installs: the connector's address, skills, hooks, and later views and monitors. No state of its own | `plugins/` in this repository |
| Local UI | What the client shows when the hub's API says so: hook context at session start, a held Stop, MCP Apps views, monitor lines | the client, driven by the hub |

## What each client can take (verified 2026-11-12 on each client's own docs)

| Client | Manifest it reads | Connector | Skills | Hooks | Views (MCP Apps) | Live notices |
|---|---|---|---|---|---|---|
| Claude Code | `.claude-plugin/plugin.json`, marketplace `.claude-plugin/marketplace.json` | `mcpServers` inline: `${user_config.hub_url}/mcp`, seat header `claude` | yes | yes | not rendered by Claude Code | monitors (experimental), channels |
| claude.ai, Desktop, Cowork | the same plugin, synced | yes | yes | no | yes | no |
| Codex | `.codex-plugin/plugin.json`, marketplace `.agents/plugins/marketplace.json`; MCP from `.mcp.json` | fixed hosted hub, no seat header (not documented) | yes | yes, after the person trusts them; plugins with hooks are kept out of the public directory | not documented | not documented |
| Grok Build | reads Claude Code marketplaces and plugins (its docs; paths not documented) | from `.mcp.json` | yes | yes (`GROK_PLUGIN_ROOT`) | not documented | not documented |

Doc sources: code.claude.com/docs/en/plugins-reference, /plugin-marketplaces, /mcp;
developers.openai.com/codex/plugins/build; docs.x.ai/build/features/skills-plugins-marketplaces,
/mcp-servers; modelcontextprotocol ext-apps spec 2026-01-26.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| G1 | Two plugins in one marketplace: `devagotchi` (connector, skills `agenthub` and `machines`) for every client, and `devagotchi-agenthub` (the four board hooks) for Claude Code. `board hook --as auto` names the agent after the client whose plugin runs it; `--if-board` keeps the hooks silent in projects without a board | none | `plugin_tests` (hooks equal the handler's, skills name only real tools, connector URL, Codex version), `one_binary` hook test, `claude plugin validate`, installs into throwaway Claude Code and Codex homes | done (this PR) |
| G2 | Guided install on the hub: each client's connect page (the `ai_client` workflow step) shows that client's plugin commands with this hub's address filled in, and `devagotchi setup` offers the plugin where the client supports one | G1 | workflow tests; browser test | not started |
| G3 | Views the API opens (MCP Apps, `io.modelcontextprotocol/ui`): the hub negotiates the extension, serves `ui://devagotchi/board`, `ui://devagotchi/ticket` and `ui://devagotchi/approval` as `text/html;profile=mcp-app`, and `board_prime`, `board_get` and `approval_request` name them in `_meta.ui.resourceUri`. The view calls the same tools back, so a click in claude.ai, Desktop, ChatGPT or VS Code is a board write every other client sees. Text answers stay complete for clients that render nothing | G1 | MCP tests on `resources/list`, `resources/read`, `_meta`; one host checked by hand | not started |
| G4 | Live in Claude Code: a plugin monitor (`devagotchi board watch --as auto`, a new verb on `board_wait` and the inbox) prints one line per note, approval answer or ticket change for this seat, so a phone approval or another agent's note reaches the running session without polling. Then a channel through `devagotchi mcp --hub`, so the hub can push into the session | G1 | CLI tests; real session receipt | not started |
| G5 | Seats for Codex and Grok: the hub names the seat after the OAuth client (Codex, Grok) when no `X-Devagotchi-Seat` comes, behind an account setting in the UI, so hooks and connector agree; then the hooks plugin for Codex (`.codex-plugin` with `hooks/hooks.json`) and Grok | G1 | hub tests on seat resolution; Codex install receipt | not started |
| G6 | Updates and listings: Claude Code follows each commit to `main` (no `version` in its manifest); Codex follows the release version (held equal by a test), so each release reaches it. Submit `devagotchi` to Anthropic's directory and OpenAI's (no hooks there) | G1, roadmap D3 (first tag) | listing receipts (operator accounts) | not started |
| G7 | Real sessions on the operator's machine: Claude Code (`/mcp` sign-in, `board_prime`, hooks at start, beat and Stop), Codex (`codex mcp login devagotchi`, a board call), Grok Build (install from this marketplace) | G1 | operator receipts in this plan | not started |
| G8 | **Private beta, before any public listing** (operator, 2026-11-15: "test before publicly offering it on the marketplace"): a `beta` channel of both marketplaces that installs the plugins against a hub the operator names (app.devagotchi.com today), in the operator's own Claude Code, Codex, claude.ai and Grok; a page on the hub listing what the beta covers and how to report | G2, G7, free-edition D1 and D2 | the operator installs from `beta` in each client and runs the gate below by hand; each run recorded here | not started |
| G9 | **The marketplace gate**: every row of the gate below passes, in a script where it can be (`tests/marketplace_gate.py` against a real hub, a real machine on Linux and Windows) and by the operator where it cannot; the public listing is the operator's act after it | G8, free-edition D3 to D5, connections-gateway X3 to X5 | the script green in CI on both OSes; the operator's run recorded | not started |
| G10 | **What this hub can do, machine-readable** (agent-hub `3ca781f` `bridge/agentguide`: the `connection_contract` tool, the `devagotchi://agent-guide` resource and the `devagotchi_connectivity` prompt; absorbed 2026-11-18): `whoami` (`mcp/tools/machines.rs:17-60`, which already returns access, plan, brokers, boards, orgs, `waiting_since` and `access_note`) gains `capabilities`: multi-tenant on or off, the sign-in ways configured, machines online, the connections the seat may use, and the features this build lacks, so an AI never infers a capability from an installed CLI or a guide. The operating rules (discover first; ask for the smallest access with a reason; a pending or timed-out approval is not approval; stop on revoke; never another person's token; provider credentials stay in their client) go where guidance already ships, the MCP server instructions from the site (`mcp/handler.rs:111-128`; the hub's MCP enables tools only), and in the `machines` skill and `/llms.txt`. No hostname in any of them: Go's hardcoded central origin is not carried over | G1 | `mcp/tests/schema.rs`: `whoami` has `capabilities`; a hub with tenancy off says so; `mcp/tests/plugins.rs`: the skill names only real tools | not started |

### The plugins on the person's own machine (operator, 2026-11-18)

The operator's decision of 2026-11-18, recorded as given: plugins for **VS Code, Codex, Claude
Code, the terminal/CLI, Claude Desktop and ChatGPT Desktop**. The UI lives on the user's
machine as optimised visual components that raise prompts and cues from local signals (for
example, a test run triggers at least a factoid capturing lessons learned working with AI).
Deterministic machine-code experiences come first, so AI tokens are spent only where they pay.
On connect, the plugin shows the user's own instance: self-hosted on their domain, a VM with a
provided subdomain, or Shippin's free or paid tiers. G11 to G15 carry it; G1 to G10 stay as
they are (Grok remains in G1, G5 and G7 until the operator says otherwise; it is not in the
2026-11-18 list).

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| G11 | **One plugin per client in the 2026-11-18 list**: VS Code (an extension), Codex, Claude Code, the terminal/CLI (`devagotchi` itself), Claude Desktop and ChatGPT Desktop. Each client's manifest, connector, skills, hooks and views are checked on that client's own docs before its work starts and added to the table above; what a client cannot take is written there, not worked around | G1 | the table above has a row per client, each with its doc source and date; an install receipt per client in a throwaway home | not started |
| G12 | **The local UI: optimised visual components on the user's machine**, not pages served by the hub: drawn by the plugin from local state and the hub's API (path row 5.6), small enough to stay open beside the work; the same components in each client that can show them, a terminal rendering for the CLI. Where their dynamic state is kept is an open decision (LiftOffDB, [editions](../../docs/editions.md)); until it is made they keep none of their own and read the hub | G11 | a component test per view; start-up and memory measured on Linux and Windows; the operator's review | not started |
| G13 | **Cues from local signals**: the plugin watches signals on the machine (a test run, a build, a commit, a hook firing) and raises a prompt or cue from them; a test run raises at least a factoid that captures a lesson learned working with AI. Nothing is read or kept before the person opts in ([DG-CAPTURE-1](../../docs/capture.md)), and a factoid stays on the machine unless the person sends it to their hub | G12 | a recorded test run raises its factoid; with capture off, nothing is raised or kept | not started |
| G14 | **Deterministic first**: every cue is machine code (rules over the signal, no model call) unless its rule names an AI step, and each AI step says why it pays; the dev build's tool-call record (path 5.8) counts, per cue, whether a model was called and its tokens | G13, path 5.8 | a test per cue proves no model call where the rule names none; the per-cue count on the page | not started |
| G15 | **On connect, the user's own instance**: the plugin asks where the person's hub is and shows it: self-hosted on their own domain, a VM with a subdomain provided for them, or Shippin's free or paid tiers. The person enters or picks it; no hostname is a default in the plugin, its manifests or its examples | G11, free-edition V1; cloud C9 for the provided subdomain and the Shippin tiers | install tests: a fresh plugin names no hub until the person gives one; each of the three kinds reaches its hub | not started |

### The marketplace gate (G9)

Nothing is listed publicly until each row holds, on a hub installed from a tagged release:

| # | What must hold | How it is shown |
|---|---|---|
| 1 | Desktop Commander parity: every tool in the teardown (`docs/desktop-commander-teardown.md` §1.3) answers on a real machine, with the machine's grants deciding | free-edition D3's per-tool tests |
| 2 | First connect as smooth as Desktop Commander's: sign in (Continue with Shippin), one line on the machine, no sudo, the AI says it is online | free-edition D1, D2 e2e |
| 3 | AgentHub works from each client: `board_prime`, claim, heartbeat, note, complete, handoff; a note reaches the other client's agent | `tests/marketplace_gate.py`, two clients on one board |
| 4 | Phone: an approval buzzes the phone and the tap decides it; a finished ticket shows on the phone | the gate script with a test push subscription; the operator on a real phone (mobile P2g) |
| 5 | Secrets never reach a transcript: a connection is called through the hub, Vaultix references resolve, a Lovable app calls through with its own key | connections-gateway X2 to X5 tests |
| 6 | Updates: a new release reaches each client through its own plugin update | G6 |


## Risks and approved decisions

- **One `.mcp.json`, three clients.** Codex 0.167.1 reads the Claude-format `.mcp.json` and
  ignores a portable `mcp.json`; it also refuses the plugin when a portable root `plugin.json`
  is present (checked, see receipts). So the shared `.mcp.json` points at the hosted hub with no
  seat, and Claude Code's manifest replaces that server by name with the person's hub and the
  `claude` seat (a later declaration of the same name wins, per Claude's docs).
- **Seats.** Codex and Grok connect as the account until G5: right for the connector, but their
  hooks would name a seat the connector does not use, so the hooks plugin is listed for Claude
  Code only (Codex's marketplace omits it; Grok is unverified).
- **Hooks need the binary.** Only the `devagotchi-agenthub` plugin runs local commands, and it
  says so. Without `devagotchi` on PATH its hooks fail as non-blocking errors; the connector
  plugin needs nothing local, so claude.ai and Cowork install it.
- **`--as auto` reads the plugin loaders' documented variables** (`GROK_PLUGIN_ROOT`, Codex's
  `PLUGIN_ROOT`, `CLAUDE_PLUGIN_ROOT`, checked in that order); `DEVAGOTCHI_ACTOR` still wins.
  Grok's and Codex's variables are from their docs, not yet seen in a real session (G7).
- **Updates.** Claude Code pins a plugin to its `version`; with none, each commit is a version,
  which is what an alpha wants. Codex shows the manifest's version, so it moves with releases.

## Verification receipts

2026-11-13, Windows 11, branch `increment-136/plugins`:
- `claude plugin validate` (Claude Code 2.1.322) on `plugins/devagotchi`,
  `plugins/devagotchi-agenthub` and the repository's marketplace: passed; directory checks found
  nothing to report; the only warning is the deliberate missing `version`.
- Throwaway `CLAUDE_CONFIG_DIR`: `claude plugin marketplace add <checkout>`, both plugins
  installed and enabled at the commit's version; `claude mcp get plugin:devagotchi:devagotchi`
  shows `https://app.devagotchi.com/mcp`, header `X-Devagotchi-Seat: claude`, "Needs
  authentication" from the live hub.
- Throwaway `CODEX_HOME` (codex-cli 0.167.1): marketplace added from
  `.agents/plugins/marketplace.json`, `codex plugin add devagotchi@devagotchi` installed and
  enabled at 0.1.0, `codex mcp list` shows `devagotchi https://app.devagotchi.com/mcp`, "Not
  logged in".
- `cargo test -p devagotchi-hub --lib plugin_tests` 5 passed; `cargo test -p devagotchi --test
  one_binary` (plugin hooks, native hooks, help) 3 passed; `agenthub::` 91 passed.
- Not run: a signed-in Claude Code or Codex session, Grok Build (not installed here) (G7).

## Delivery

Implementation: G1 done. Validation: above. Push: not_verified. PR: not_created.
Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
