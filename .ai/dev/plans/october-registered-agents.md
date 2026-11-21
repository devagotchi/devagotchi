# Restore registered API-key agents from the oldest AgentHub source

**Superseded scope:** this initial narrow proposal is folded into
[A02 of the complete absorption plan](absorption/implementation.md#a02--principals-credentials-roster-roles-and-policy).
OAuth binding, kinds, roles, policy and the principal default are now included in that
packet. Reuse the existing local branch for the entire S1 source; do not open a profile-only
PR or run CI before the full source queue is authored. The earlier non-goals and per-slice
delivery sequence below record the initial proposal, not the current execution contract.

Source: preserved 46419c6, principals.go/admin_people.go/lease.go and
web/src/pages/Agents.tsx/AgentEditor.tsx. Continues shared workflows (root PR278),
principal capacity (AgentHub PR15) and workload reads (AgentHub PR16).

Objective: a person can register an immutable agent identity, name it, set its total
live-work limit, pause or enable it, issue scoped API keys for it, and see real claims
and last-seen activity. Different keys for one agent share its identity/capacity;
replacement preserves binding. Changing or omitting a seat header cannot bypass it.
Use the existing PostgreSQL accounts/keys and AgentHub lease authority.

Scope: account-owned, always-nonhuman agents bound to new API keys. Existing unbound
keys keep their established seat behavior. Browser management uses the live person,
same-origin checks, waiting gates, versioned edits and preserved failed drafts.
One environment still has users, without organization UI. Owner credentials unchanged.

Non-goals: OAuth binding, configurable kinds/roles/policy and a global default limit.
Those remain explicit source gaps. This slice uses an explicit per-agent limit, with
zero meaning unlimited. It neither completes nor deletes historical source 46419c6.

Tasks: add identity-owned profile storage with forced RLS and credential binding FK;
fallible stored-key restriction decoding; live verifier lookup and last-seen update;
canonical actor/fence/inbox identity and capacity wiring; no-JavaScript roster/editor;
existing key issuance/replacement UI integration with one-time secrets; source parity
and earlier delivery records reconciled; focused real-PG/HTTP tests and native browser
acceptance; full applicable suite; green PR/main; verified feature branch deletion;
official image deployment and Docker/restart acceptance.

Acceptance: two keys/one agent, cross-board capacity and changed/omitted headers;
same profile name in another account does not collide; disabled profile and revoked
key are refused on an existing MCP session; reduced limits stop new claims while
existing leases remain renewable; stale edits retain drafts and cannot overwrite;
agent is never human; only the owning browser manages profiles/keys; tenant/account
boundaries and read/write failures are explicit; malformed stored restrictions never
become unrestricted; real last-seen and visible claims agree with board leases;
one-time keys appear after issuance/replacement and never reappear on GET or in
storage/audit; mobile390/desktop1280 with JavaScript disabled and restart persistence.

Risks: aliases confused with identity, missing/disabled profiles treated as unbound,
independent per-key limits, stale session authority, private task disclosure, and lost
one-time secret on a post-commit list read failure. Verification must cover each. No
second token scheme, task store, scheduler, or SQLite hosted fallback.

Implementation/validation/push/PR/main/Docker: pending. DELIVERY INCOMPLETE.
