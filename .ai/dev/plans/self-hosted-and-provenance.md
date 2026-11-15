# Self-hosting follows the release; machines say where they run; the cloud console reads it

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-14: "the version I am deploying should be the typical
  opensource ... a customized version of this to integrate into the cloud panel that will show
  the commercial cloud users and then the tab for the managed deployments and it should clearly
  show their provenance whether its on one of my nodes/servers or on aws google oracle etc
  custom"; and "I can truly get my push notifications?"
- PR: S1 done in #189; P1 onwards one PR each, after free-edition R3
- Branch / baseline: `increment-141/self-host-truth` from `5c28f8c` (v0.1.0)
- Objective: a self-hosted hub runs the release, not every merge; every machine reports where
  it runs (which cloud, or a server of the operator's own), shown on its page and to AI
  clients; the private cloud repository's console lists customers and managed deployments
  with that provenance.
- Non-goals: any commercial code in this repository ([editions](../../docs/editions.md));
  changing app.devagotchi.com, the test hub, which keeps following `:main` (its `.env` pins
  `DEVAGOTCHI_TRACK` explicitly).
- Canonical architecture / related plan: [cloud edition](cloud-edition.md) (C2, C3),
  [roadmap](roadmap.md).

## What is already true (checked 2026-11-14)

- Push: Web Push with the hub's own VAPID key (`crates/hub/src/notify/push/`), no third-party
  account. It fires for an approval waiting, a machine going offline or coming back, and a
  machine joining. **Notify this phone** and **Send me a test approval** are on `/approvals`
  and the You page; the test pushes to the person's own phones only. On iPhone the hub must be
  added to the Home Screen first. Not yet pushed: a ticket assigned to a person (S2).
- Images: GHCR has `:main`, `:sha-<commit>`, `:0.1.0` and `:latest` (anonymous pulls answer
  200). Until S1 every installer defaulted to `:main`.
- Machines report only their build (`LinkFrame::Hello.build`); nothing about the host.
- The link protocol is strict both ways (`deny_unknown_fields` on `HubFrame` and
  `LinkFrame`): a new field or frame either side does not know closes the connection. P1 has to
  negotiate, not just add a field.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| S1 | Self-hosting follows the release: `install.sh`, `hub-up.sh`, `compose.yaml` and `.env.example` default to `:latest`; the deploy files come from the same release (`track_ref`: `:latest` the newest release's tag, `:X.Y.Z` `vX.Y.Z`, `:main` main); `:latest` moves only for a tag without a pre-release suffix (cloud-edition C1) | none | merged in #189 (release channel by default) | done |
| S2 | A ticket assigned to a person pushes to their phones, like an approval | none | hub test with the push recorder | not started |
| P1 | The machine learns where it runs, on the machine: Linux DMI (`/sys/class/dmi/id/sys_vendor`, `product_name`, `chassis_asset_tag`, `bios_vendor`), Windows `HKLM\HARDWARE\DESCRIPTION\System\BIOS`, WSL and container markers; mapped to aws, gcp, azure, oracle, hetzner, digitalocean, vultr, linode, ovh, scaleway, proxmox/kvm, vmware, hyper-v, wsl, container, physical, unknown. No network call (no metadata endpoints): read-only local files | none | unit tests over recorded DMI samples per provider | not started |
| P2 | Negotiated report: the hub serves `GET /link/accepts` (`{"host":1}`); a link sends `LinkFrame::Host` after `Welcome` only when that answers, so a new machine never breaks an old hub and an old machine never meets a frame it refuses | P1 | link test against a hub with and without `/link/accepts` | not started |
| P3 | The hub keeps it per machine (the machines crate's `machine_` tables, after free-edition R3), the owner can name it ("my node in Frankfurt", provider "own"), and it shows on the machine page, in Machines, in `machines` (MCP) and the machines API | P2, free-edition R3 | hub tests on Postgres | not started |
| P4 | Extension point for the cloud (cloud-edition C2): a read API over machines with provenance and accounts, used by the core's `/operator` page first | P3 | operator test | not started |
| K1 | Private repo: console crate starting the core hub through C2's entry, with `/console` tabs **Customers** (accounts on hosted hubs, plan, machines, last seen) and **Managed deployments** (each hub or workspace the business runs, its provenance: one of the operator's own nodes, or AWS / Google Cloud / Oracle / Azure / other) | P4, cloud-edition C2 and C3 | private CI; console test over a seeded store | not started |

## Risks and approved decisions

- S1 changes what a new self-hosted install runs: the newest release instead of the newest
  merge. Existing installs keep their `DEVAGOTCHI_TRACK`; `update.sh` is unchanged. Approved by
  the request above ("the typical opensource").
- `deploy/shippin/` is an installer for a per-user broker, written for Shippin Gold
  workspaces. It carries no secrets and no hosted configuration, and the hosted workspaces
  fetch it from this repository, so it stays; its Shippin-specific README moves to the cloud
  repository when the hosted deployment does (cloud-edition C6).
- Provenance is what the machine's firmware says, read locally; a VM can claim any vendor. It
  is shown as reported, with the owner's own label beside it, never used for access.

## Verification receipts

S1: `bash -n deploy/compose/install.sh deploy/proxmox/hub-up.sh`: ok. `track_ref` with
`:latest` gave `v0.1.0` (the GitHub API's newest release), `:main` and
`localhost:5055/devagotchi:main` gave `main`, `:0.1.0` gave `v0.1.0`, `--ref abc` gave `abc`.
CI: recorded on the PR.

## Delivery

Implementation: S1 in this branch. Validation: pending CI. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
