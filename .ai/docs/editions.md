# Editions: what is core and what is cloud

This is the boundary. Check it before adding anything that only a hosted service would need.

| Lives in | What belongs there |
|---|---|
| **Core** (`devagotchi/devagotchi`, this repository, source-available under [FSL-1.1-ALv2](../../LICENSE)) | The hub, AgentHub boards, notes and approvals, the phone app, self-hosting, connectors for third-party AI clients, API keys, sign-in providers, organisations and roles, signals, statistics, and every setting a person running their own hub needs |
| **Cloud** (a private repository, not created yet) | Hosted-service operations: billing, plan enforcement for a paid service, tenant provisioning at scale, hosted deployment configuration and secrets, and enterprise chat |

Rules:

1. A self-hoster gets every feature that works on one hub. Cloud adds running it for many.
2. Cloud reuses this core unchanged: its private repository holds this one as a git submodule
   pinned to a release tag, and moves by bumping that pin ([cloud edition plan](../dev/plans/cloud-edition.md)). It is never a fork, since a
   GitHub fork of a public repository stays public and drifts from upstream.
3. Enterprise code, secrets and hosted-service deployment configuration never go in this repository.
4. The license (FSL-1.1-ALv2) allows self-hosting for one's own and one's organization's use
   and forbids offering the core to others in a commercial service that competes with it; that
   is what the cloud sells. Each version becomes Apache-2.0 two years after it is published.
5. Every setting a person changes has a guided page, grouped by what it is for. A setting that only
   exists as a file or an environment variable is unfinished.

Decided by the operator on 2026-11-14, and recorded so this repository does not invent them:

- The free edition is one developer's hub on their own VM. Multi-tenancy (`tenancy.mode multi`,
  `/operator`, plan limits) stays in this repository but as its own crate, outside the default
  build ([free edition plan](../dev/plans/free-edition.md), R4).
- The commercial cloud is a marketing site and a hosted service at devagotchi.com, with a free
  tier. Its repository is `shippinAI/devagotchi-cloud` (private, made 2026-11-14); none of its
  deployment belongs in this repository.
- Sign in with Shippin (Nomen) has a card on `/admin` -> Sign-in from a hub's first start. The
  switch on that card offers it, on once the issuer is saved, and turns it off without deleting
  the issuer. The issuer, client id and secret are settings: type them, or pass
  `install.sh --nomen ISSUER` with `DEVAGOTCHI_NOMEN_CLIENT_ID`. Nothing in this repository
  fills in an issuer. A hub with no issuer shows no Shippin button, so Google still needs the
  owner's own client until an issuer is set. Signing in with no client registered at all is
  S0 (Nomen accepts Client ID Metadata Documents). That change is in Nomen, not this
  repository, and has not landed ([free edition plan](../dev/plans/free-edition.md)).
- Billing, when the cloud repository has code, is Stripe. Plugin submission comes later.
- A self-hosted install names its own hub. No hostname or address is a default in code,
  scripts, or examples.
- Decided 2026-11-15 ([ADR 0003](../../docs/adr/0003-hosting-pooled-free-per-instance-paid.md)):
  free sign-ups run in pooled cells (the tenancy crate); paid, managed and consulting customers
  each get their own instance of the same release a solo developer installs from GitHub,
  deployed by the private panel to Shippin, AWS, Google Cloud, Oracle or a customer's host.
  Staff reach inside a customer's hub only through access its owner grants.
- Decided 2026-11-15: Dragonfly is an optional cache, off unless configured
  ([ADR 0004](../../docs/adr/0004-optional-cache-dragonfly.md)); AgentHub is Postgres in every
  form, a library in Devagotchi and Shippin and a binary or container on its own
  ([ADR 0005](../../docs/adr/0005-agenthub-postgres-everywhere.md)).
- What the cloud replaces is `crates/hub/src/extensions.rs`: extra routes, plan limits, tenant
  lifecycle, and an in-process event hook. Each default is what a self-hosted hub already does.
