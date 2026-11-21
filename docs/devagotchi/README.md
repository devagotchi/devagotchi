# The Devagotchi product design, absorbed from shippinAI/agent-hub

The public home is `devagotchi/devagotchi`; hosted-service work belongs in the private
`shippinAI/devagotchi-cloud` repository ([editions](../../.ai/docs/editions.md)). Enterprise
code, secrets, and hosted-service deployment configurations must never go in the public
repository.

The 2026-10-21 ruling put the agents' board in this hub; this directory puts the design beside
the code. The source is `shippinAI/agent-hub` at `97172d9`, the head of its local branch
`feat/devspace-bridge-contract` (2026-10-22, never pushed; its last push, `ca626db` of
2026-10-20, lacks that branch's changes to `08-threat-model.md`, `26-connectivity.md`,
`27-gap-tracking.md` and `39c-architecture-on-gold.md`): `docs/devagotchi/01`–`39g` and `adr/`,
its `docs/devagotchi-spec.md` (the index; keep it at `../devagotchi-spec.md` so the links
inside the files still resolve), `docs/spec.md` (keep as `go-hub-spec.md`) and
`docs/connect.md` (`go-hub-connect.md`). Nothing has been copied yet: this README is the only
file here.

**The copy itself is one command, run by the operator** (a session may not move one
repository's files into another on its own):

```bash
# both clones side by side, ../agent-hub checked out at 97172d9
cp -r ../agent-hub/docs/devagotchi/. docs/devagotchi/
cp ../agent-hub/docs/devagotchi-spec.md docs/devagotchi-spec.md
cp ../agent-hub/docs/spec.md docs/devagotchi/go-hub-spec.md
cp ../agent-hub/docs/connect.md docs/devagotchi/go-hub-connect.md
git add docs && git commit -m "docs: import the Devagotchi design from shippinAI/agent-hub@97172d9"
```

What that branch and the later `preserve/packaging-2026-10-25` (`adca8b7`) changed is absorbed
into the plans, capability by capability, on 2026-11-18
([factory](../../.ai/dev/plans/agenthub-factory.md), "The Go hub's last branches"). The
packaging branch's two notes (`docs/agenthub-packaging-slice.md`,
`docs/windows-private-storage-next-packet.md`) are not part of the copy; their content is in
those plan rows. `39c` at `97172d9` names one domain as "the central hub": that was the Go
hub's hardcoded origin and is not carried over; here a hub's origin is its own setting and no
hostname is a default ([cloud edition](../../.ai/dev/plans/cloud-edition.md) C11).

## What governs what

- **This repository is the one home.** `shippinAI/agent-hub` is frozen for reference; nothing
  new lands there.
- **Where these documents and the plan disagree, the plan wins:**
  [product-and-platform-plan.md](../product-and-platform-plan.md) (D01–D19, G0–G5, the boundary
  test), [agent-board.md](../agent-board.md) (B01–B09) and
  [task-graph-and-mesh-contract.md](../task-graph-and-mesh-contract.md). Everything these
  documents cover that the plan does not — subscriptions, providers, guides, costs, the
  approvals PWA, GUI control, the chat page, the public product — they govern, until a plan
  packet or an ADR through Jesse's component says otherwise.
- **Decisions.** `10-decisions.md` D1–D21 and `adr/0001`, `adr/0002` were Jesse's answers in
  October 2026 and stand. Two of them collide with the plan and need a ruling (ledger rows
  marked **conflict**; plan row D19). Resolved rows become `docs/adr/NNNN-*.md` through the
  ADR component.
- **Code paths** inside these files (`server/internal/…`, `bridge/…`, Postgres tables) describe
  the Go implementation they were written against. The ledger says what each became.

## Absorption ledger — Go `agent-hub` → this repository

| Go capability (package · tables) | Here | Status | Lands in |
|---|---|---|---|
| Tasks, deps, notes, leases, fences, events chain and mutation replay (`store`, including `idempotency_keys`) | `crates/agenthub`: PostgreSQL board transactions, chain/outbox, dependencies and standalone boards | task/lease/graph behavior implemented; original principal-scoped replay across every mutation remains open; issue/source receipts cover narrower operations | PR #19 and subsequent PostgreSQL/absorption changes; remaining parity must be proven per operation |
| Workflows, settings, presets as data (`workflows settings`, `presets`) | `crates/agenthub/src/hosted/settings.rs`, `hosted/shared_workflows/`; hub configuration forms | versioned per-board forms accepted in PR277; shared catalog/binding backend merged in AgentHub PR14, browser integration in progress; this does not establish all preset parity | [Configuration](../../.ai/dev/plans/october-board-configuration.md) and [shared workflows](../../.ai/dev/plans/october-shared-workflows.md) |
| MCP tools (13: whoami … wait_for_updates, review_ticket) (`mcptools`) | the hub's `/mcp`: board tools in `crates/hub/src/agenthub/mcp/` (`board_prime` …), `whoami` in `crates/hub/src/mcp/tools/` | ported | increment 8 |
| `hub` CLI with claim cache; `GET /api/v1/protocol` AGENTS.md snippet | `devagotchi board …` (`crates/agenthub/src/cli/`), `board prime`, `board onboard` | the CLI ported; no `/api/v1/protocol` route | increment 8 |
| REST `/api/v1` + SSE `/stream` (`httpapi`, `stream`) | hub axum routes | partial: `/api/v1/me`, organisations (`crates/hub/src/tenancy/orgs/api/`), project statistics (`ops/warehouse`); no SSE | increments 8 (API), 10 (SSE) |
| Web UI: Board, TaskDetail, NewTask, Agents, Tokens, Connect, Policies, Roles, Workflows, Settings, Audit, Actions, Retention, Login (React Aria SPA) | server-rendered pages in `crates/hub/src/web/` and the installable phone app (`web/app/`: Overview, AgentHub, Approvals, Machines, You) | server-rendered, the SPA not ported; no Agents, Policies or Retention pages | [factory](../../.ai/dev/plans/agenthub-factory.md) F13 (Agents page) |
| Principals and per-agent bearer tokens (`principals tokens`, `auth`) | `crates/hub/src/identity/`: human accounts, sign-in providers, passkeys and scoped API keys; local-user administration accepted in PR276 | partial: immutable credential-bound agent profiles, enabled/kind/roles/last-seen and global principal WIP are not restored by human accounts or caller-selected seat aliases; Cloudflare Access and Tailscale sign-in also remain open | [Local users](../../.ai/dev/plans/october-local-users.md); original `principals.go` and `lease.go` remain behavioral references |
| Machines, enrollment codes, presence (`machines`, `bridgehub`) | `crates/hub/src/machines/` (`devices/` enroll and revoke, `link/` the hub's side) and `crates/link` (the machine's outbound WebSocket) | ported | PRs #7, #12 |
| Machine operations through the bridge (`machineops`, `proc_leases`) | hub routes `operation_*` to brokers by name over the link | ported (process operations) | — |
| Grants, access requests, presets, sandbox policies, capability risks (`access`, `grants access_requests grant_presets sandbox_policies capabilities capability_risks`) | hub config `grants` (static, `crates/hub/src/machines/link/presence.rs`); AgentHub's standing approvals (`crates/agenthub/src/approval.rs`: expiring, revocable, on the chain, but advisory) | partial: no enforced grant that ends | [free edition](../../.ai/dev/plans/free-edition.md) D7 |
| Policy rules, deny by default (`policy`, `policyrules`, `policies`) and the gold-policy PDP client (`policygate`) | — | not ported | increment 13 (with factory-board F1, the actuation gateway) |
| Change policy: policy docs, change tickets, evidence, reviews, reviewer agents (`changepolicy`, `policy_docs change_tickets ticket_evidence ticket_reviews`) | board review lane (reviewer ≠ author) | partial | increments 10 (Approve with hash) and 13 |
| Action feed and audit retention (`actionfeed`, `auditkeep`, `action_events audit_categories audit_archives`) | `crates/hub/src/ops/audit/` owner activity and `web/app/agenthub/audit/` board history | filtered feeds, verification and refresh accepted in PR274/275; category retention, gzip archives, chained manifests/signatures and archive-before-prune remain open; visit-analytics retention is a separate concern | Original `auditkeep` remains the retention reference |
| Catalog CRUD with JSON-Schema validation (`catalog`, `schemacheck`, `docker_runtime_presets shell_profiles router_endpoints …`) | — | not ported | increment 15 |
| Guides (`guides`, `guidekit`, `guides guide_versions guide_runs guide_updates`) | — | not ported | increment 16 |
| Subscriptions, role bindings, write-only secrets (`subs`, `secretbox`, `subscriptions role_bindings secrets`) | — | not ported; Vaultix (36) is the secret home | increment 17 |
| Provider schema v1 and the 18-row matrix (`providers`, `providers provider_models`, docs 29/34) | — | not ported; the board's B06 ACP driver is its runtime half | increment 17 |
| Tenants, chat apps, resume acks (`tenants chat_apps resume_acks`) | tenants: `crates/hub/src/tenancy/` (`tenancy.mode multi`; its own crate outside the default build at free edition R4); chat apps and resume acks: none | tenants ported for the multi-tenant builds; the rest not ported | with Shippin's Bundled Stack row |
| Bridge: 28 Desktop Commander-parity tools (files, search, processes, PDF/DOCX/XLSX, config) in Go | `crates/machine-engine`: process execution and its journal, files, search, interactive processes | **partial**, tool by tool in [.ai/docs/desktop-commander-parity.md](../../.ai/docs/desktop-commander-parity.md) | free edition D3 |
| Bridge sandbox, grants, doctor, install, GUI control (docs 02/03/11) | Linux cgroup ladder and Windows Job Objects in `crates/machine-engine/src/execution/platform/`; installers in `deploy/` and the join scripts in `crates/hub/src/machines/join/` | partial; GUI control **conflict** (below) | G2; decision |
| Approvals PWA, push, passkey signing, self-update (39a, 39e, 39g) | approvals on the phone app (`crates/hub/src/web/approvals.rs`), Web Push with the hub's own key (`crates/hub/src/notify/push/`), machines updated by the hub (`machine_update`) | partial: no passkey signing of a decision; no notification outbox | [factory](../../.ai/dev/plans/agenthub-factory.md) F18; [path](../../.ai/dev/plans/path.md) 3.2 |
| Chat page, rooms, routines (24, 39d) | chatmaton (separate repo) | out of this repo | chatmaton board |

**Conflicts to rule on (plan row D19):**

1. GUI control — Go D7 "yes, from v0.1" versus plan D18 "DEFERRED: screen/input automation".
2. The bridge binary — Go `bridge/cmd/devagotchi` (28 tools, Go) versus this workspace's
   `machine-engine` + `link` + `mcp` (Rust; the broker is the machine engine's `service`). The 2026-10-21 bridge plan already names
   the Go bridge as "what it replaces"; its tool list is the parity target, not its code.

## Reading order, once imported

Start at `../devagotchi-spec.md` (the index), then `10-decisions.md`, `27-gap-tracking.md` and
`28-public-gaps-and-phases.md` for the registers; `39*` for the Gold build plan. The plan's phases
(G0–G5) and the board's packets (7–11) are the schedule; `09-phases.md` and
`39-chatmaton-gold-build-plan.md` are the product sequence they serve.
