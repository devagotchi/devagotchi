# The cloud edition: a private repository with this one as a submodule

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: move towards starting the official cloud commercial
  version as its own submodule; only what is truly cloud goes there.
- PR: not_created
- Branch / baseline: C1 and C2 in this repository; C3 onwards in the private repository
- Objective: the hosted, paid service is built in a private repository that uses this core
  unchanged, pinned to a release, and moves forward by bumping the pin.
- Non-goals: moving any self-hosting feature out of the core ([editions](../../docs/editions.md),
  rule 1); a fork (rule 2).
- Canonical architecture / related plan: [editions](../../docs/editions.md), [roadmap](roadmap.md),
  [factory board plan](../../../docs/agenthub-factory-board-plan.md) ("After this plan").

## Shape of the private repository

```
devagotchi-cloud/                 private; organisation: decision D1
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
| C1 | First pre-release tag `v0.1.0-alpha.1`: release archives and image `:0.1.0-alpha.1`; `:latest` moves only for tags without a pre-release suffix (today `image.yml` moves it for every `v*` tag) | D3 | Release run on the tag; anonymous pull of the tag | not started |
| C2 | The extension points above, each with its default and tests; no change in behaviour | C1 | hub tests unchanged and new trait tests | not started |
| C3 | Create the private repository (D1), core as a submodule at the C1 tag, an empty `crates/cloud` that starts the core hub through C2's entry, CI building and testing both | C2, D1 | private CI green; the binary starts and answers `/healthz` | not started |
| C4 | Billing (D2) and the plan policy from it | C3, the gate in the roadmap | billing tests against the provider's test mode | not started |
| C5 | Provisioning at scale | C3, the gate | tests on Postgres | not started |
| C6 | Hosted staging deployment from `deploy/` | C4, C5 | the staging hub answers; a test tenant signs up and connects an AI | not started |
| C7 | Enterprise chat | C6 | its own plan | not started |

## Risks and approved decisions

Already in the core and kept there (ruled 2026-11-11): `tenancy.mode multi`, `/operator` and
plan limits; C2 lets the cloud replace their policy without moving them. Secrets and hosted
deployment configuration never enter this repository (editions rule 3). The license (BSL 1.1)
permits the cloud: AngryVibes LLC is the Licensor.

## Verification receipts

Each milestone records its own.

## Delivery

Implementation: not started. Push: not_verified. PR: per milestone.
**DELIVERY INCOMPLETE: NOT STARTED**
