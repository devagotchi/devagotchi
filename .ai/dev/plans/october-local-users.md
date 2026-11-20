# October principals: local human users

Branch `increment-282/restore-local-users`, baseline `e01c7b2` (PR275 merged after all
18 checks passed; feature branches deleted). Continues oldest source `46419c6`, specifically
`a703761` server/internal/store/admin_people.go and principals.go: immutable principal IDs,
editable display names, enabled state, versioned updates and human-only administration.

The current user explicitly requests users within one self-hosted environment. This replaces
the older November 18 interpretation that single mode means only one user. Single mode still
does not expose organization/invite/tenant management. Extend the existing PostgreSQL
accounts, passwords and session/token guards; no parallel human identity or storage system.

Add an owner-only Local users page linked from Hub settings. Create a named ordinary user
with an email/password atomically, edit its display name and enabled state with optimistic
version checks, and reset its existing local password. Protect the owner's identity and
authority; no self-demotion or owner disable. Disabling ends active sessions and OAuth
connections and revokes API keys, so enabling later does not revive old sessions or revoked tokens.
Password resets end browser sessions and OAuth connections; existing API keys retain their
independent lifecycle. Passwords/hashes never appear in the page, log or audit detail.
All changes use existing account authority and audit ownership. No email is sent.

Acceptance: real PostgreSQL atomic duplicate-email/concurrent-edit tests; one environment
for all added users; owner-only HTTP access, strict-origin refusal, escaping, invalid input
without partial users, stale version refusal, old sessions/tokens refused after disable,
re-enable with fresh login, owner protections and actual password login/reset. Native Edge
owner/user workflows at phone/desktop widths. Full applicable suite before push, all PR CI
before merge/delete, and official Docker acceptance afterward.

Agent principals, credential bindings, individual WIP, editable roles/policy and the rest
of the oldest source are still open. The current account/seat contract must be reconciled
with original credential-bound principal identity before an agent roster is called restored.

Implementation: complete for this scoped user-management flow. Validation: all applicable local checks passed; PR CI pending.
Push: not pushed. PR: not created.
Main: not merged. DELIVERY INCOMPLETE.

Focused PostgreSQL domain and HTTP tests passed. The first HTTP run exposed missing origin
validation; all mutations now require this hub's explicit Origin and refuse a conflicting
Sec-Fetch-Site. The regression exercises creation, edit and password reset with missing,
foreign and contradictory origins. Pagination traverses 103 users; database failure remains
503. Duplicate creation, stale edits, owner protection, revoked sessions, OAuth tokens,
pending codes/device grants and recovery links are tested. Password reset preserves
independently managed API keys. The first browser run exposed the owner-reset redirect;
a reset of the current owner now redirects to sign-in. Phone field styling was corrected
after screenshot review. Final browser/workspace/CI results follow below.

The final full workspace run passed 892 tests (505 hub, 96 AgentHub, 291 others), zero
failures or ignored tests, with required PostgreSQL and four test threads. Earlier failed
runs remain recorded: an owner-only link was incorrectly assigned to an ordinary-user
fixture; a device poll asserts OAuth invalid_grant rather than a Rust error; changing copy
while an older binary ran caused a schema mismatch. Those fixtures were corrected and the
entire final suite ran against unchanged source. Owner-reset redirect and recovery-link
revocation are covered over HTTP. Browser/process validation and PR CI remain pending.

Final binary build and Edge acceptance passed: owner creates/renames/disables/re-enables
an ordinary user; real password login succeeds; old sessions end; reset requires the new
password; ordinary users cannot reach administration; organization routes remain absent;
owner self-reset returns to sign-in and authenticates again. 390/1280 layouts have no
horizontal overflow and the phone screenshot was inspected. The browser fixture now uses
a unique display name per run, so prior acceptance users do not make its count ambiguous.
MCP protocol, dependency audit, generated contracts, AI checks and formatting passed.
Windows process E2E and PR CI are the remaining validation gates.

Windows process E2E passed on a fresh isolated PostgreSQL database, including board,
organization, signals, machine link/reconnect/revoke, CLI/browser sign-in and no SQLite in
hub state. Linux boundary/MCP smoke, ClickHouse and deployment checks remain CI gates.
Implementation and local validation are complete; publication and merge follow only after
the operator policy gates and green PR checks. No release-readiness claim.

Delivered: PR276 merged as `9c6dc6e43374e3bf35cdb0d2b073f67fb98500d4` after all18
checks completed successfully. Local and remote increment282 branches were deleted.
Official Release44823993998 succeeded; its immutable image was deployed to the development
Docker stack. Edge user creation, rename, disable/enable, password reset, session revocation,
owner boundary and390/1280 layouts passed. Existing accounts also passed HTTP access checks
and restart persistence; owner credentials were preserved. This closes local-user delivery,
not agent-principal/policy parity or the oldest source as a whole.
