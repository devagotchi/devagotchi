# The cloud edition: a private repository with this one as a submodule

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: move towards starting the official cloud commercial
  version as its own submodule; only what is truly cloud goes there.
- PR: https://github.com/devagotchi/devagotchi/pull/189
- Branch / baseline: C1 and C2 in this repository; C3 onwards in the private repository
- Objective: the hosted, paid service is built in a private repository that uses this core
  unchanged, pinned to a release, and moves forward by bumping the pin.
- Non-goals: moving any self-hosting feature out of the core ([editions](../../docs/editions.md),
  rule 1); a fork (rule 2).
- Canonical architecture / related plan: [editions](../../docs/editions.md), [roadmap](roadmap.md),
  [factory board plan](../../../docs/agenthub-factory-board-plan.md) ("After this plan").

## Shape of the private repository

```
devagotchi-cloud/                 private; organisation: shippinAI (D1, decided 2026-11-14)
  core/                           git submodule: devagotchi/devagotchi at a v* tag
  Cargo.toml                      workspace: crates/*, depending on core/crates/* by path
  crates/cloud/                   the hosted binary: the core hub plus the extensions below
  crates/billing/                 plans and payments (D2), webhooks, the plan policy
  crates/provisioning/            tenant lifecycle at scale, quotas, abuse limits
  crates/chat/                    enterprise chat
  deploy/                         hosted deployment; references to secrets, never secrets
  .ai/                            the same contracts, with the cloud's own project.json
  .github/dependabot.yml          ecosystem gitsubmodule: proposes each new core tag as a PR
```

The submodule pins an exact tag, so every cloud build names the core it runs. The core is
public, so the private repository's CI checks it out without a token.

## How it is hosted (ADR 0003, operator 2026-11-15)

[ADR 0003](../../../docs/adr/0003-hosting-pooled-free-per-instance-paid.md) records the decision.

| Tier | Runs as | Isolation | Deployed by |
|---|---|---|---|
| Free and community | a **pooled cell**: one hub process and one database, the `tenancy` crate on, many tenants under row-level security; the edge routes each tenant's host name to its cell | row-level security; cells split as they fill | the panel, on Shippin nodes |
| Paid, managed, consulting | an **instance**: the exact release a solo developer installs from GitHub, its own database, host name and upgrade pace, no tenancy code | a process and a database per customer | the panel, to a Shippin node, AWS, Google Cloud, Oracle or a host the customer names, with its provenance recorded |

One release runs both. The panel moves a release to a staging cell and a staging instance
before any customer. A free tenant becoming paid is exported from its cell into a new
instance, and its machines reconnect to the new host name.

## Devspaces and scale (C10)

The hub stays the trusted part: pooled cells for free tenants, an instance per paying customer
([ADR 0003](../../../docs/adr/0003-hosting-pooled-free-per-instance-paid.md)). What runs
customers' own code is the devspace, and that is where isolation and density matter most:

- **Firecracker microVMs**, the isolation AWS Lambda and Fly.io run on: a guest kernel per
  devspace, boot in a fraction of a second, a small memory overhead per VM.
- **Snapshot and resume** is what makes the free tier affordable: an idle devspace is a file on
  disk, and the next AI call brings it back in well under a second, with its memory loaded as it
  is touched. This is "cheap to keep dormant and fast to wake" in the cloud's vision.
- **Hosts need KVM**: bare-metal instances (AWS `.metal`, Oracle bare metal, Shippin's own
  nodes) or nested virtualisation where a provider offers it. The panel's deploy targets (C9)
  record which hosts can run devspaces.
- Lighter isolation (gVisor, containers) stays for the hub's own processes, which run our code,
  not the customer's.

## Staff, support and the panel

The panel is the private repository's web app. It is also where staff work.

- **Staff roles** (in the panel, signed in through Nomen with a passkey, every action audited
  there):
  - *Support*: answers customers, reads deployment health and versions, and proposes
    configuration changes the customer approves.
  - *Operations*: deploys, upgrades, rolls back and restores instances and cells, and handles
    incidents.
  - *Admin*: staff accounts, billing and plans.
- **Inside a customer's hub, staff have no standing access.** The customer's owner grants it
  on `/admin` → Support access: to a named person, with a role (read, or settings), for a set
  time (24 hours unless changed), revocable at once. Every call it makes goes into that hub's
  own audit chain, and the owner sees it. The grant is core (C2b): a self-hoster gives it to
  their consultant the same way.
- **Questions and tickets**: support runs on an AgentHub board in the panel (Shippin uses its
  own product). A customer's "Help" opens a ticket there with the hub's version, health and
  provenance attached, and nothing else unless the customer adds it.
- **Infrastructure rights** over a managed instance (deploy, upgrade, roll back, restore a
  backup) belong to Operations, under the customer's contract, in the panel. They never
  include signing in to the customer's hub.

## What the core must offer first (C2)

The cloud adds to the hub through extension points the core defines and uses itself, each with
the self-hosted behaviour as its default. Nothing cloud-specific is in the core.

| Extension point | Default in the core | The cloud's |
|---|---|---|
| A library entry: start the hub with extra routes and the implementations below | today's `devagotchi hub serve` | `crates/cloud` |
| Plan policy: a tenant's limits (machines, boards, AI apps) | the limits the operator sets on `/operator` | limits from the paid plan |
| Tenant lifecycle: on create, suspend, delete | create on first connect (`tenancy.mode multi`) | provisioning, quotas, deletion with export |
| Event sinks for board and account events | signals and ClickHouse (exist) | billing usage, chat |

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| C1 | First pre-release tag `v0.1.0-alpha.1`: release archives and image `:0.1.0-alpha.1`; `:latest` moves only for tags without a pre-release suffix | D3 | Release run on the tag; anonymous pull of the tag | image rule done in `image.yml`; release notes drafted under CHANGELOG `Unreleased`; the tag is not created here (the operator tags after review) |
| C2 | The extension points above, each with its default and tests; no change in behaviour | none for the interfaces; the cloud pins them at the C1 tag | `extensions` tests; hub behaviour unchanged for the defaults | implemented: `serve_with`, `PlanPolicy`, `TenantHooks`, `EventHook`, `RouteMount` in `crates/hub/src/extensions.rs`. Pending merge |
| C2b | Core: **support access grants** (above): `/admin` → Support access, a named person, role, expiry, revoke; their calls audited in the hub's chain and shown to the owner | free-edition D1 (sign-in) | hub tests: no grant no access, expiry ends it, revoke ends it within a second, every call audited, the owner sees it | not started |
| C2c | Core: **one tenant's export and import** (all its rows, machines' keys included, as one signed archive), for a cell to instance move | free-edition R4 | round-trip test on Postgres between two databases | not started |
| C3 | Create the private repository (D1), core as a submodule at the C1 tag, an empty `crates/cloud` that starts the core hub through C2's entry, CI building and testing both | C2, D1 | private CI green; the binary starts and answers `/healthz` | not started |
| C4 | Billing (D2) and the plan policy from it | C3, the gate in the roadmap | billing tests against the provider's test mode | not started |
| C5 | Provisioning at scale | C3, the gate | tests on Postgres | not started |
| C6 | Hosted staging deployment from `deploy/` | C4, C5 | the staging hub answers; a test tenant signs up and connects an AI | not started |
| C7 | Enterprise chat | C6 | its own plan | not started |
| C8 | The panel: customers, cells, instances (target, version, provenance, health), staff roles, the support board | C3, C2b | panel tests; a staff member without a grant cannot open a customer's hub | not started |
| C9 | Deploy targets for instances: a Shippin node first, then AWS, Google Cloud, Oracle and a custom host over SSH, each from the same release and install scripts | C8, a tagged release | an instance deployed, upgraded and rolled back on each target | not started |
| C10 | **Devspaces on microVMs** (operator, 2026-11-15: "firecracker/microvms ... for scale including free tier"): a person's cloud devspace is a Firecracker microVM with the machine engine inside, joined to their hub like any machine; idle ones are snapshotted to disk and hold no memory; an AI call to a sleeping devspace resumes it from the snapshot (memory loaded lazily on fault) before the hub routes the call; disks are copy-on-write layers over one base image per OS. Free tier: limits on CPU-seconds, disk, egress and no outbound mail ports; paid: always-on if they want it | C8; KVM on the host (bare metal, or nested virtualisation where the provider offers it) | resume to `echo ok` measured on each target; a thousand idle devspaces on one host hold no memory | not started |
| C11 | **Editions and networks** (operator, 2026-11-06, `increment-113/plan-editions-agenthub`, absorbed here 2026-11-15): an *edition* is everything that makes a deployment one brand (name, logo, colours, words, its surface hosts, sign-in options, plans, its network), stored in the database with the site directory as its defaults, versioned, edited on `/operator/edition` with preview, publish and roll back, no restart; nothing in code names a domain. A *network* on an edition names its identity provider (Nomen), its offers and its link endpoint; linking an account is OIDC account linking to that provider; an edition with no network shows no offers. A second edition, `shippin.online`, is a deployment of the same image (deploy files only). Per the 2026-11-06 ruling the white-label editor is a commercial feature, here. Open: who may edit an edition (proposed: the operator only) | C3, A11 | an edition edited, published and rolled back without a restart; shippin.online served from the same image | not started |

## Risks and approved decisions

- 2026-11-15, operator: hosting is pooled for free and one instance per paying customer
  ([ADR 0003](../../../docs/adr/0003-hosting-pooled-free-per-instance-paid.md)). The private
  repository `shippinAI/devagotchi-cloud` exists (made 2026-11-14) with core at `v0.1.0`; C3's
  repository step is done, its crates are not.

Superseded 2026-11-14 by the [free edition plan](free-edition.md): multi-tenancy becomes its own
module outside the default build (R4). The earlier ruling read: already in the core and kept there (ruled 2026-11-11): `tenancy.mode multi`, `/operator` and
plan limits; C2 lets the cloud replace their policy without moving them. Secrets and hosted
deployment configuration never enter this repository (editions rule 3). The license (FSL-1.1-ALv2)
permits the cloud: AngryVibes LLC is the Licensor.

## Verification receipts

Each milestone records its own.

## Delivery

C1's image rule and C2's interfaces are in this repository, not merged. The tag `v0.1.0-alpha.1` is not created. C3 onwards wait on the private repository, which does not exist.
Implementation: in progress. Push: not_verified. PR: per milestone.
**DELIVERY INCOMPLETE: NOT MERGED TO MAIN**
