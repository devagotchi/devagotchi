# DG-ADAPTER-1: harness capability matrix

Verified against the linked official docs on 2026-11-10. Features may change.
Harness discovery/trust determines hooks and instructions, **not** frontier/subscription
model choice. Canonical code is `crates/hub/src/ai.rs`; native configs are translation stubs.

| Harness | Instruction discovery | Native stub / field differences | Actual coverage / gap |
|---|---|---|---|
| Codex | root `AGENTS.md` | `.codex/hooks.json`: grouped PascalCase events; `session_id`, optional `transcript_path`; three-second handlers | Local admitted lifecycle/prompt/tool/turn events; hosted tools/specialized paths can be absent; transcript JSONL is opaque, not stable |
| Claude Code | `CLAUDE.md` imports `@AGENTS.md` | `.claude/settings.json`: grouped PascalCase; `session_id`, `transcript_path` | Local admitted events, authorized transcript at turn/session end; no claim that Desktop/web has the same lifecycle |
| Copilot CLI / current Copilot harness | root `AGENTS.md` and `.github/copilot-instructions.md` | `.github/hooks/session-capture.json`: version 1, camelCase; `sessionId`; `agentStop` maps to canonical turn end | Hook event payloads only; startup may not fire for resume/noninteractive; no automatic full assistant transcript |
| Older VS Code Local / PascalCase configuration | root `AGENTS.md` / Copilot instructions | **Not this camelCase adapter**; its snake_case payload is deliberately rejected rather than guessed | Use an explicit future versioned adapter or manual canonical append/import; no advertised automatic coverage |
| Claude Desktop / arbitrary hosted chat / other clients | Host-dependent | No lifecycle stub claimed | MCP tool availability does not grant filesystem chat/history access; explicit authorized export/import only |

`ai hook` validates the requested native event and required session key, checks a supplied
cwd stays within this checkout, then produces `{}`. Unknown formats/events fail explicitly.
Capture is independent of board init. Stop/session end never complete tickets, release
leases or authorize merge. Existing native **board** hooks remain a separate coordination
contract; install them with the board's documented configuration when using a bound board.
These archive hooks do not pretend to inject AgentHub context.

## Operator installation / acceptance

1. Build/install the unified `devagotchi` binary and ensure the actual harness process
   resolves it on PATH (including GUI-launched processes).
2. Inspect committed hook configs before granting project trust/admission. For Codex use
   `/hooks`; changed definitions require re-review. No trust bypass is recommended.
3. Choose metadata or informed content capture with `ai policy set`; inspect `policy show`.
4. Start a **fresh** actual client session. Confirm its instruction references and hook
   admission/debug view. Submit a harmless prompt/tool action; inspect `capture status`,
   `capture export`, and `capture verify`. Test Stop/resume separately.
5. Disable a hook temporarily and confirm the gap is reported operationally; a missing
   callback cannot be detected from silence alone. Native-binary fixture tests verify our
   translation, not a provider installation that was never launched.

Do not ship these configs as a claim of universal installation or cloud durability.
Cloud jobs have ephemeral filesystems, different tool coverage and admission restrictions;
durable export needs an explicit approved destination/transport, not secret-bearing logs
in artifacts by default. Local archive stays local; no automatic cloud upload is implemented.

### Testing this checkout from source

The compiled binary is not automatically installed on PATH. On Windows, build and launch
the client from a PowerShell whose PATH includes that build directory:

```powershell
cargo build --locked -p devagotchi --bin devagotchi
$env:PATH = (Resolve-Path .\target\debug).Path + [IO.Path]::PathSeparator + $env:PATH
devagotchi ai check
devagotchi ai policy show
```

Then deliberately opt in as described in [capture](capture.md) and launch a fresh
client from that terminal. A separately launched VS Code/desktop process does not
inherit a terminal-only PATH change; install the binary normally or relaunch from the
configured environment. On Unix, prepend the absolute `target/debug` directory to PATH.
Build/test commands do not change the operator's capture consent.

## Research and version boundaries

- [VS Code custom instructions](https://code.visualstudio.com/docs/agent-customization/custom-instructions)
  and [hooks](https://code.visualstudio.com/docs/agent-customization/hooks): model/harness
  distinction, discovery and the old Local vs current Copilot format difference.
- [Copilot hook reference](https://docs.github.com/en/copilot/reference/hooks-reference):
  version 1, camelCase/snake_case schemas, session and cloud coverage, command/OS options.
- [Codex hooks](https://developers.openai.com/codex/hooks/): trust hashes, config discovery,
  native payloads, unstable transcript representation and shutdown time limits.
- [Claude hooks](https://code.claude.com/docs/en/hooks): settings/import discovery, session
  event distinctions, working-directory behavior and shutdown budgets.
- [OpenTelemetry GenAI](https://github.com/open-telemetry/semantic-conventions-genai):
  development-stage observability vocabulary, not this durable event contract.
- [SWE-agent trajectories](https://swe-agent.com/latest/usage/trajectories/): observable
  action/observation/state as prior art for evidence, not permission to collect secrets.

Schema generation: run `devagotchi ai schema` and write its UTF-8 stdout to
`.ai/docs/capture-event.schema.json`; `devagotchi ai check` detects drift.
Do not hand-edit schema or translate Rust contracts independently in provider stubs.
