# Editions: what is core and what is cloud

This is the boundary. Check it before adding anything that only a hosted service would need.

| Lives in | What belongs there |
|---|---|
| **Core** (`devagotchi/devagotchi`, this repository, source-available under [BSL 1.1](../../LICENSE)) | The hub, AgentHub boards, notes and approvals, the phone app, self-hosting, connectors for third-party AI clients, API keys, sign-in providers, organisations and roles, signals, statistics, and every setting a person running their own hub needs |
| **Cloud** (a private repository, not created yet) | Hosted-service operations: billing, plan enforcement for a paid service, tenant provisioning at scale, hosted deployment configuration and secrets, and enterprise chat |

Rules:

1. A self-hoster gets every feature that works on one hub. Cloud adds running it for many.
2. Cloud reuses this core unchanged: its private repository holds this one as a git submodule
   pinned to a release tag, and moves by bumping that pin ([cloud edition plan](../dev/plans/cloud-edition.md)). It is never a fork, since a
   GitHub fork of a public repository stays public and drifts from upstream.
3. Enterprise code, secrets and hosted-service deployment configuration never go in this repository.
4. The license allows production self-hosting and forbids offering the core to others as a
   hosted or managed service; that is what the cloud sells. Each version becomes AGPL-3.0 four
   years after it is published.
5. Every setting a person changes has a guided page, grouped by what it is for. A setting that only
   exists as a file or an environment variable is unfinished.

Decided by the operator on 2026-11-14, and recorded so this repository does not invent them:

- The free edition is one developer's hub on their own VM. Multi-tenancy (`tenancy.mode multi`,
  `/operator`, plan limits) stays in this repository but as its own crate, outside the default
  build ([free edition plan](../dev/plans/free-edition.md), R4).
- The commercial cloud is a marketing site and a hosted service at devagotchi.com, with a free
  tier. Its repository is `shippinAI/devagotchi-cloud` (private, made 2026-11-14); none of its
  deployment belongs in this repository.
- Sign in with Shippin (Nomen) is on from a hub's first start and the owner turns it off with one
  switch on `/admin` -> Sign-in, so Google sign-in always works without the owner's own Google
  client ([free edition plan](../dev/plans/free-edition.md), S0 and D1). This replaces the
  earlier same-day line "This core does not default to it": until S0 lands, an operator types
  the issuer on `/admin`, or passes it to `install.sh --nomen`.
- Billing, when the cloud repository has code, is Stripe. Plugin submission comes later.
- A self-hosted install names its own hub. No hostname or address is a default in code,
  scripts, or examples.
- What the cloud replaces is `crates/hub/src/extensions.rs`: extra routes, plan limits, tenant
  lifecycle, and an in-process event hook. Each default is what a self-hosted hub already does.
