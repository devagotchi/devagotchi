# Connections: the hub as the one place agents and apps reach outside services

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-14: "start to incorporate connectors so this can be the hub
  for them to use with lovable to connect to resend or secrets for vaultix and nomen and the big
  known out there".
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`
- Objective: a person connects a service once (Resend, Stripe, GitHub, Slack, Vercel, Supabase,
  Linear, OpenAI, Anthropic …) with its secret held by the hub or by their Vaultix. Their agents
  (MCP) and their apps (a Lovable app, over HTTP with a Devagotchi API key) call that service
  through the hub. Neither ever holds the secret; every call is audited per agent.
- Non-goals: a marketplace of our own; reading secret values back out (write-only, as Lovable,
  Vaultix and agent-hub's secret box all do); acting as an end user's OAuth app for every SaaS in
  the first milestones (API keys first, OAuth providers after).
- Canonical architecture / related plan: [roadmap](roadmap.md), [factory](agenthub-factory.md),
  [hub settings](hub-settings.md) (sealed secrets, `crate::sealed`), [API keys](api-keys.md)
  (`KeyLimits`: a key that may call only what it was made for). Code to come: the gateway in
  `crates/agenthub` (`connections`), its pages in `crates/hub/src/web/app/`.

## Why this shape (research of 2026-11-14, primary sources in the factory plan)

- Lovable's own connectors are a gateway: the app holds one `LOVABLE_API_KEY` and an opaque
  connection key; provider tokens stay in an encrypted store nobody can read back; the gateway
  refreshes them (docs.lovable.dev/integrations/security). Lovable's custom MCP servers are chat
  context only, not the app's runtime, so a Lovable app reaches the hub over HTTP.
- Rechecked 2026-11-15 (docs.lovable.dev/integrations/mcp-servers, /any-api, /security): custom MCP
  servers in chat on every plan (a static IP is Enterprise); shared connectors are used by the
  deployed app at runtime; keys go in project secrets, never readable back; apps made from
  2026-05-13 on run on TanStack Start with server functions, older ones on Edge Functions. Lovable
  documents no general REST API, so a project comes home through the GitHub repo Lovable syncs
  it to, not through Lovable.
- Credential brokering at egress is where Infisical (Agent Proxy, Agent Vault), Composio, Arcade
  and Cloudflare MCP portals converge: the agent holds a handle, the broker attaches the real
  credential per host, and transcripts never carry it.
- The operator's own Vaultix (an Infisical fork, `D:\localdev\vaultix`) already has "proxied
  services" with 36 templates (resend, stripe, slack, github, vercel, supabase, linear, openai,
  anthropic …), placeholders swapped per host, and an MCP that returns references, never values.
  Machine sign-in: `POST /api/v1/auth/universal-auth/login`; a secret:
  `GET /api/v4/secrets/:name?projectId&environment&secretPath`, `Authorization: Bearer`.
- Nomen (`D:\localdev\nomen-react-go`) issues `client_credentials` and RFC 8693 token exchange
  (`act` delegation): the hub can call a Nomen-protected service as itself, or on a person's
  behalf with a token scoped to that call.
- OWASP Top 10 for Agentic Applications (ASI01–10): least privilege per tool, no secrets in the
  model's context, audit of every tool call.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| X1 | Catalog and connections: a template per service (host, auth placement, a test call), stored connections per tenant with the secret sealed (`settings.key`), a Connections page in the app (desktop and phone) to add, check and remove one | none | store tests both backends; page tests | done (increment 140): 11 services (Resend, Stripe, GitHub, Slack, Vercel, Supabase, Linear, OpenAI, Anthropic, Postmark, Notion), API keys first |
| X2 | Agents call through the hub: MCP `connection_call {connection, method, path, body}` restricted to the template's host and allowed paths; the hub attaches the credential, strips it from anything echoed back, audits per agent seat; `connections` lists what an agent may use (names only) | X1 | MCP tests against stand-ins; a secret never appears in a result | done (increment 140) |
| X3 | Apps call through the hub: `POST /api/v1/connections/{name}/…` with a Devagotchi API key limited to named connections (`KeyLimits`), for a Lovable or any other app's server code; CORS closed (server-to-server) | X2, API keys | HTTP tests; a key for one connection cannot use another | not started |
| X4 | Resend first, end to end: template, `email_send` as a typed tool over X2, a guide on the page for a Lovable app (its edge function calls the hub with its key) | X3 | stand-in test; operator receipt with a real Resend key | not started |
| X5 | Vaultix as the secret source: a connection's secret may be a Vaultix reference (`vaultix://org/project/env/path#KEY`); the hub signs in to Vaultix with a machine identity (universal auth) set on `/admin`, reads the value at call time, never stores it | X2 | stand-in Vaultix test; operator receipt against their Vaultix | not started |
| X6 | Nomen: the hub as a Nomen client (`client_credentials`) for services that accept Nomen tokens, and token exchange so a call can carry the person's delegated identity (`act`) | X2 | stand-in token endpoint test | not started |
| X7 | The big ones: Stripe, GitHub, Slack, Vercel, Supabase, Linear, OpenAI, Anthropic, Postmark, Notion as templates (the same 36 Vaultix ships, where they apply); OAuth connections for those that need a person's account (GitHub, Slack, Linear) | X2 | template tests | not started |
| X8 | **Lovable, both ways**: (a) the hub as a custom MCP server in Lovable's chat, so a person's Lovable builder reaches their boards, machines and connections; (b) a Lovable app calls through the hub with its own limited key (X3), from a server function or an Edge Function, with the snippet for each; (c) **import from Lovable**: a project's GitHub repo cloned onto the person's machine or devspace, a board started from its README and TODOs (factory F5), and its secrets' *names* listed for the person to fill into Vaultix or the hub, never their values | X3, X5, factory F5 | a real Lovable project on the operator's account: chat reaches the hub, the app sends through Resend via the hub, the import makes the board and the secret list | not started |
| X9 | **Shippin.ai**: Sign in with Shippin (free-edition S0, D1) and a person's Shippin devspace as one of their machines, joined without a line (the devspace is provisioned already) | free-edition S0, D2; cloud C10 | a devspace appears in `machines` after sign-in and runs `echo ok` | not started |

## Risks and approved decisions

- A connection is a tenant's: never shared across tenants; its secret is sealed with the hub's
  key and never shown again (write-only), as sign-in secrets are.
- An agent can only reach a template's host: no open proxy (SSRF), private addresses refused
  unless the hub is single-tenant and the person allows it, as webhooks are (`wake.rs`).
- Every call is audited with the agent's seat, the connection and the path, never the body.

## Verification receipts

X1 and X2, 2026-11-14, Windows 11, Postgres 18 (docker), branch `increment-140/connections`:
- `connections_tests::a_connection_is_called_through_the_hub_and_its_key_never_leaves_it` on
  SQLite and Postgres (a database made, so the Postgres half and its row-level security ran),
  against a stand-in service that echoes the key back: a refused key is not kept; another
  site's form, an unknown service and a bad name are refused; a good key is checked, sealed and
  shown as its last four; `connections` lists it by name; `connection_call` reaches the service
  with the key and returns the answer with the key redacted; five calls that try to leave the
  service or use another method reach nothing; another account neither sees nor uses it; removal
  ends it.
- `devagotchi-agenthub` `connections::tests`: the path and origin rules, each service's key
  placement, redaction, the catalog's own consistency.
- Not run: any real service (a Resend key is needed for X4's receipt).

## Delivery

Implementation: X1 and X2 done. Push: not_verified. PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: X3 TO X7 NOT STARTED**
