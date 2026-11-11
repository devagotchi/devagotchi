# API keys: granular control after the OAuth handshake

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-11: OAuth is only the handshake; real UI and backend for API
  key management, granular control and durations.
- PR: not_created
- Branch / baseline: `increment-125/api-keys`, stacked on `increment-124/connectors`
- Objective: a person can issue an agent a credential limited to exact capabilities and a time.
- Non-goals: keys on the REST API; organisation-owned keys; the network layer, planned separately.
- Canonical architecture / related plan: [connectors](connectors.md)

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Store, hash, scopes, durations, chain events | none | `accounts/api_keys/tests.rs` | done |
| M2 | Enforcement in `Hub::call_tool`, closed by default | M1 | `keys_tests.rs` | done |
| M3 | `/app/keys`: make, show once, last use, revoke, replace | M1 | `keys_tests.rs` | done |
| M4 | Each client's configuration with a key | M1, connectors | `crates/connectors` tests | done |
| M5 | Review and merge to main | M1-M4, operator approval | merged PR | pending |

## Risks and approved decisions

A key is a bearer credential, so it is limited, expiring, hashed at rest and revocable, reaches only
the MCP endpoint, and can never act as the person. A key limited to some machines can still list the
names of the others.

## Verification receipts

`cargo test -p devagotchi-connectors -p devagotchi-mcp -p devagotchi-hub` on local Postgres.

## Delivery

Implementation: done. Validation: local. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
