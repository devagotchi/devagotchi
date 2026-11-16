# Texas baseline: Devagotchi installed the way a public customer would

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-16 (chat): no new feature work until Devagotchi is
  deployed on the Texas server and fully confirmed there, set up as a real public customer
  would, with the owner account made by the person in one step from a phone; Texas then
  becomes the dev and pre-release environment. Pre-release: no legacy, no compatibility.
- PR: not_created
- Branch / baseline: `increment-175/texas-baseline` from `main` 5104f35
- Objective: one confirmed install on Texas from the published hub image, claimed from a
  phone, with a machine linked and an AI client connected, and every step it took written
  down as a step a customer can follow.
- Non-goals: Cloudflare, Tailscale and Wardenclyffe sign-in or routes; AgentHub standalone
  server; licensing; marketplace. Each waits for the baseline (milestones below).
- Canonical architecture / related plan: [deploy/proxmox](../../../deploy/proxmox/README.md),
  [deploy/compose](../../../deploy/compose/README.md), ADR 0005, D24.

## What was checked (2026-11-16, main 5104f35)

- The hub from main, started on an empty Postgres 16, comes up healthy and makes 56 tables,
  all in schema `devagotchi`, each with its domain's prefix (the layout rule 8 test).
  ClickHouse holds one table, `devagotchi.board_events`, made on the first export.
- A machine linked to that hub with `hub enroll` / `link enroll` / `link run` and showed as
  connected in `/healthz`, once the hub config named it.
- `hub enroll` printed a code for a device the config did not name; the hub then refused the
  link (`enroll_refused`). Fixed in this PR (M1).
- `hub-up.sh` and `install.sh` follow `:latest` unless told otherwise. `:latest` is the
  v0.1.0 image, which the operator is deleting, so until a true release the Texas install
  must pass `--channel main`.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | `hub enroll` refuses a device its config does not name, before printing a code | none | unit test `a_code_is_issued_only_for_a_device_the_config_names_as_a_link` | done (this PR) |
| M2 | Pre-release installs follow `:main` by default; no step names `:latest` until a stable tag exists | M1 | install-check CI; README steps | pending |
| M3 | Texas: fresh container via `hub-up.sh --channel main`, claimed from a phone, one machine linked, Claude Code connected | M2, operator's yes for Texas and its domain | `/healthz`, claim screenshot, `whoami` from the client | pending |
| M4 | The customer steps, each a click, a value to author, or a save, written from what M3 took | M3 | a person follows them on a second container | pending |
| M5 | No-legacy pass: one baseline migration with final table names (drops 0029/0030 renames), migration history tables in `devagotchi` with prefixes, stale SQLite comments gone, AgentHub's SQLite board removed (ADR 0005) | M3 | prefix test, fresh install on Texas | pending |
| M6 | After the baseline: sign-in through Cloudflare Access and Tailscale; Cloudflare Tunnel and Tailscale as link routes (D24); Wardenclyffe | M4 | their own plans | pending |
| M7 | AgentHub standalone server and its release workflow (ADR 0005), then its first release, protection and the Devagotchi pin | M5 | their own plans | pending |

## Risks and approved decisions

- No branch protection on AgentHub and no AgentHub pin until a true release (operator,
  2026-11-16).
- Touching Texas waits for the operator's yes.

## Verification receipts

- `cargo test -p devagotchi-hub --lib machines::cli`, rustc 1.99.1, Linux x86_64: 1 passed.
- Hub start and table list: debug build of 5104f35 against Postgres 16.15, healthy.

## Delivery

Implementation: M1 done. Validation: focused test only. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
