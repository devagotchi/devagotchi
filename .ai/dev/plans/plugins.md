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
