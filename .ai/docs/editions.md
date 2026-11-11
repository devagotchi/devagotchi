# Editions: what is open and what is cloud

This is the boundary. Check it before adding anything that only a hosted service would need.

| Lives in | What belongs there |
|---|---|
| **Open core** (`devagotchi/devagotchi`, this repository) | The hub, AgentHub boards, notes and approvals, the phone app, self-hosting, connectors for third-party AI clients, API keys, sign-in providers, organisations and roles, signals, statistics, and every setting a person running their own hub needs |
| **Cloud** (`devagotchi/devagotchi-enterprise`, private) | Hosted-service operations: billing, plan enforcement for a paid service, tenant provisioning at scale, hosted deployment configuration and secrets, and enterprise chat |

Rules:

1. A self-hoster gets every feature that works on one hub. Cloud adds running it for many.
2. Cloud reuses this core unchanged, through a private repository synced from upstream.
3. Enterprise code, secrets and hosted-service deployment configuration never go in this repository.
4. Every setting a person changes has a guided page, grouped by what it is for. A setting that only
   exists as a file or an environment variable is unfinished.

Already in this repository and marked for review against rule 2 and 3: `tenancy.mode multi`,
`/operator` (waiting list and plans) and plan limits. `devagotchi-enterprise` does not exist yet.
