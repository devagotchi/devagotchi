# Copilot discovery adapter

Read and follow [the canonical root instructions](../AGENTS.md), then its linked
[AI contracts](../.ai/README.md). Do not maintain a second workflow here.

Copilot is a harness, not a model provider. A Claude model selected in Copilot still
uses the Copilot adapter. `.github/hooks/session-capture.json` uses Copilot's native
camelCase events; never copy Claude/Codex payload assumptions into it.
See [capabilities and limitations](../.ai/docs/adapters.md).
