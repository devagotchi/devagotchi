# Devagotchi plugins

Two plugins, one marketplace (this repository):

| Plugin | What it gives the client | Needs on this computer |
|---|---|---|
| `devagotchi` | The Devagotchi connector (your hub's MCP endpoint, signed in with OAuth) and two skills: **agenthub** (take, keep, note, finish or hand off board tickets) and **machines** (run commands and work with files on your machines, ask for approval first) | nothing |
| `devagotchi-agenthub` | Hooks: the board's state at session start, leases kept alive while you work, a Stop held while you still hold a ticket. Silent in projects without a board | the `devagotchi` command on PATH |

Your boards, notes and approvals live on your hub, so every client you install them in works
the same board.

## Claude Code

```
/plugin marketplace add devagotchi/devagotchi
/plugin install devagotchi@devagotchi
/plugin install devagotchi-agenthub@devagotchi
```

Then `/mcp` and sign in to Devagotchi. Running your own hub? Set its address with
`/plugin configure devagotchi@devagotchi` (Hub address, without `/mcp`). Updates: turn on
auto-update for the `devagotchi` marketplace in `/plugin`, or run `claude plugin update`.
The same plugin installs in claude.ai, Claude Desktop and Cowork (Customize, Plugins); the
hooks plugin is Claude Code's only.

If you already added Devagotchi as a connector by hand, remove that one so the tools are not
listed twice.

## Codex

```
codex plugin marketplace add devagotchi/devagotchi
codex plugin add devagotchi@devagotchi
codex mcp login devagotchi
```

Updates: `codex plugin marketplace upgrade`. The plugin uses the hosted hub; for your own hub,
`codex mcp add devagotchi --url https://<your hub>/mcp` instead. Codex works the board as your
account for now (no seat of its own yet), so the hooks plugin is not offered there.

## Grok Build

Grok Build reads Claude Code marketplaces: `grok plugin marketplace add devagotchi/devagotchi`,
then install `devagotchi`. Not yet checked in a real Grok session.

## For maintainers

`crates/hub/src/plugin_tests.rs` holds these files to the code: the hooks equal
`agenthub::cli::plugin_hooks()`, every tool a skill names is served by the hub, the connector
points at the hosted hub, and Codex's version is the release's. Check a change with
`claude plugin validate plugins/devagotchi` and an install into a throwaway `CODEX_HOME`. The
plan and receipts: `.ai/dev/plans/plugins.md`.
