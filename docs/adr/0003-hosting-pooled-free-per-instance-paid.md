# ADR 0003: hosted Devagotchi is pooled for free, one instance per paying customer

Status: accepted. Date: 2026-11-15.

## Authority and scope

The operator chose this on 2026-11-15 ("Hybrid: pooled free, per-instance paid") when asked
how the official hosted Devagotchi is run. It shapes the private `shippinAI/devagotchi-cloud`
repository and the parts of this core it relies on. It changes no self-hosted behaviour.

## Context

- A hub runs commands on its customers' own machines. A bug that crosses customers in a
  shared hub is the worst failure this product can have.
- A hub keeps each machine's live connection (`machines/link`) in one process's memory. A
  shared hub spread over several processes needs routing by tenant, or a shared bus.
- Machines hold an always-on connection to their hub, so a hub with connected machines cannot
  sleep. A process per free user costs money for every idle sign-up.
- Multi-tenancy with row-level security exists (`tenancy.mode multi`); app.devagotchi.com runs
  it. The free edition moves it into its own crate outside the default build (free-edition R4).
- The cloud's vision (`devagotchi-cloud/docs/vision.md`) already names three ways to get
  Devagotchi: self-hosted, managed and consulting deployments, and the commercial cloud.

## Decision

1. **Free and community sign-ups: pooled cells.** A cell is one hub process and its database,
   built with the `tenancy` crate, holding many tenants under row-level security. The edge
   routes each tenant's host name to its cell, so a tenant's MCP calls and its machines'
   connections always reach the same process. No cross-process bus is needed while a cell is
   one process. Cells are added as sign-ups grow. A cell with more than one process uses the
   optional cache ([ADR 0004](0004-optional-cache-dragonfly.md)).
2. **Paid, managed and consulting customers: one instance each.** An instance is the exact
   release a solo developer deploys from GitHub (the same image tag, the same install
   scripts), with its own database, host name, sign-in settings and upgrade pace, and no
   tenancy code. The private panel deploys it to a Shippin node, AWS, Google Cloud, Oracle
   or a host the customer names, and records where each runs (its provenance).
3. **One product.** Pooled cells and instances run the same core release, and the panel
   tests a release on a staging cell and a staging instance before moving anything to it.
   Nothing hosted gets a fork or a patch the self-hoster lacks
   ([editions](../../.ai/docs/editions.md), rules 1 and 2).
4. **A free tenant upgrades by moving.** Going from a cell to an instance is an export of
   that tenant's rows into a new instance's database, done by the panel. Machines reconnect
   to the new host name.
5. **Staff never hold standing access inside a customer's hub.** Support works through
   access the customer's owner grants: for a named person, for a limited time, audited in the
   customer's own audit chain, and revocable. A self-hoster uses the same grant for their
   consultant. Infrastructure rights over a managed instance (deploy, upgrade, roll back,
   restore) are in the panel, under the customer's contract, and are audited there.

## Consequences

- The core must provide:
  - releases a host can pin and roll back (tags, images, `--version`);
  - the tenancy crate (R4);
  - an export and import of one tenant;
  - support access grants ([cloud edition](../../.ai/dev/plans/cloud-edition.md), C2b);
  - a read API over machines with provenance (self-hosted-and-provenance P4).
- The panel, in the private repository, keeps customers, cells, instances, deploy targets,
  staff roles and support tickets.
- Machine connections are why cells stay one process for as long as possible. Splitting
  cells is cheaper than building cross-process routing.
