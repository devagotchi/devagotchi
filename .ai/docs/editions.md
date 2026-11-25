# Editions: what is core and what is cloud

This is the boundary. Check it before adding anything that only a hosted service would need.

| Lives in | What belongs there |
|---|---|
| **Core** (`devagotchi/devagotchi`, this repository, source-available under [FSL-1.1-ALv2](../../LICENSE)) | One environment with its owner and local users and permissions: the hub, AgentHub boards, notes and approvals, the phone app, self-hosting, connectors for third-party AI clients, API keys, sign-in providers, signals, statistics, and environment settings |
| **Multi-tenant** (this repository, the `tenancy` domain; its own crate outside the default build once [free edition](../dev/plans/free-edition.md) R4 lands) | Multiple customer environments on one service: customer provisioning, tenant organisations, public sign-up and its limits, `tenancy.mode multi`, `/operator`, tenant plans. The commercial build can turn it on; the standalone release does not carry it. Local accounts and permissions remain in core |
| **Cloud** (the private repository `shippinAI/devagotchi-cloud`) | Hosted-service operations: billing, plan enforcement for a paid service, tenant provisioning at scale, hosted deployment configuration and secrets, and enterprise chat |

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

## Current clarification: 2026-11-20

The first GitHub release is self-hosted / indie: one installation, one environment, an owner
and users of that environment. It has no multi-tenant hosting. Account creation must not
provision another customer tenant. Local users, their access grants and revocation stay in
core when R4 separates commercial tenancy. Existing internal scope keys and database access
controls are not removed merely because their names contain `tenant`.

Development runs this same self-hosted configuration in Docker. The requested owner login
is applied only to that deployment, through the existing account/password flow; no known
credential ships in the image, Compose defaults, repository or release assets. Login after
restart and an ordinary user's denied admin actions must be verified on the deployed build.

The commercial public offering follows the accepted GitHub release and includes a free
tier. Hosting, billing and public-service optimizations remain separate from local-user
management. See [release readiness](../dev/plans/v0.1-release-readiness.md#self-hosted-baseline-2026-11-20).

This clarification supersedes the one-account/no-local-user restrictions in the historical
2026-11-18 decision below. The historical decisions are retained with that qualification.

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
- What the cloud replaces is `engine/hub/src/extensions.rs`: extra routes, plan limits, tenant
  lifecycle, and an in-process event hook. Each default is what a self-hosted hub already does.

Decided by the operator on 2026-11-18:

- **Three builds, one licence (FSL-1.1-ALv2 stays):**
  - *The Shippin build*: commercial and multi-tenant. Today's multi-tenant `main` is its basis.
  - *The standalone GitHub release* (historical; local-user restriction superseded above): free, and tailored to **one** user. It does not offer
    managing other users or customers. The owner manages their own projects and their
    startup's systems. The Core row above was corrected to match: organisations and roles were
    listed there and now sit on the multi-tenant side. The standalone release gets there through
    [free edition](../dev/plans/free-edition.md) R4 (tenancy as its own crate, out of the
    default build). Until R4 lands, `/admin` shows a People section (sign-up limits and
    organisations, `engine/hub/src/web/hub_settings.rs:224`, `:231`, `:251-255`) even on a
    single-owner hub; R4 removes it from the single-user build.
  - *Managed deployments*: offer multi-tenancy. [ADR 0003](../../docs/adr/0003-hosting-pooled-free-per-instance-paid.md)
    describes a paid instance as running "no tenancy code"; that line needs the operator's
    reconciliation with this decision (proposed: a managed instance runs the multi-tenant build
    when its customer wants accounts of its own).
- **AgentHub is provided either by Devagotchi** (the library inside the hub, as today) **or by
  a separate service hosted by Shippin.** Both are the same AgentHub on Postgres
  ([ADR 0005](../../docs/adr/0005-agenthub-postgres-everywhere.md)).
- **What Devagotchi is for:** the central point for a solo builder working across tools and
  customers (Replit, VS Code, Base44, Codex CLI, Figma) and services (Infisical, Qdrant,
  SurrealDB Cloud, HydraDB). Integrations are set up once, not per project
  ([connections](../dev/plans/connections-gateway.md)). It follows behaviour, state, intent and
  history, and when the user starts a new project it asks whether that should be a Devagotchi
  project (with AgentHub), which they can undo later from the project's settings
  ([factory](../dev/plans/agenthub-factory.md) F23).
- **Plugins** for VS Code, Codex, Claude Code, the terminal/CLI, Claude Desktop and ChatGPT
  Desktop, with their UI on the user's machine, deterministic first: [plugins](../dev/plans/plugins.md)
  G11 to G15.

Open, not a commitment (operator, 2026-11-18): **LiftOffDB.** A planned fork of Turso (Turso
joined Supabase, announced 2026-10-02, as the operator states it) built for concurrent writes.
Tiers starter, then managed, then advanced, with no capability lost from one tier to the next;
the starter tier accepts single points of failure in exchange for quick recovery, backups and
virtualization. The plugin's dynamic components (plugins G12) are its first consumer. Postgres
stays for the multi-tenant builds (the Shippin build and managed deployments). Whether the
standalone release uses an embedded engine is undecided until a test port proves it; until
then the standalone release stays on Postgres like every other build.
