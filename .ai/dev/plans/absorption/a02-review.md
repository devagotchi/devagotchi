# A02 source comparison and authored changes

Source S1 `46419c697964016eb547c60416a8f2f5e854a95f`. Work remains open; this is not
an absorption or delivery receipt. CI, pushes, PRs and source deletion remain held.

## Credential prerequisites

- `store/admin_people.go` issues secrets once and deliberately excludes issuance from
  receipt replay. Current hub issuance already does that, but its browser response lost
  the secret if the subsequent key-list read failed. The page now retains the committed
  response's secret and shows the list failure, without persisting/replaying the secret.
- Current API-key readers silently decoded malformed restrictions as empty lists; empty
  project/machine lists mean unrestricted. Verification, listing and replacement now use
  fallible typed decoding with bounds and sanitized errors. Refused replacement leaves
  the original key, key count, last-use timestamp and audit events unchanged.
- Local required-PostgreSQL checks passed: all four API-key tests and the real HTTP
  create/replace/list-failure regression. GET never replays any issued secret; responses
  retain `Cache-Control: no-store`. Initial compilation caught an incorrect SQL wrapper
  type in the new fixture; corrected before the passing run. Logs retained in `target/`.

## Source authority being restored

`store/admin_people.go`, `admin_config.go`, `admin_misc.go`, `conf.go`,
`policy/policy.go`, `policy/policy_test.go` and the default preset were read directly
from the pinned source. Source semantics include human-only configuration, versioned
profiles/roles/rules, checked kinds/roles, no self-escalation, default deny, highest
matching priority, deny at a tie, all-label selectors and literal-or-star action matching.
Source deletion checks referenced roles. Invalid stored JSON must fail closed rather
than reproduce the source's ignored JSON errors.

## Further local authoring

- AgentHub now owns a typed policy evaluator and additive PostgreSQL catalog for roles,
  principal settings and rules. Configuration uses optimistic versions; deletion refuses
  role references and policy changes cannot remove the acting administrator's ability to
  repair policy. Seed defaults install once, never recreate deleted rules, and contain no
  original operator identities. Five focused evaluator/catalog checks passed locally.
- Identity owns account-bound profiles and OAuth bindings. Profiles are never human.
  The active environment owner changes authority; a user may rename their own assigned
  agent without changing kind, roles, limit or enabled state. Types and roles validate
  inside the profile transaction; removal checks even disabled profiles. Audit commits
  with each write. Last-seen uses PostgreSQL time with a 15-second write interval.
- Keys bind optionally to an owned enabled profile and retain that binding when rotated.
  OAuth binds by account and stable client id across refresh families. A disabled binding
  remains a rejecting row, rather than falling back to an unbound identity. Three focused
  profile/key/OAuth PostgreSQL checks passed, including two same-name OAuth clients.
- Registered credentials use `@agent:<account>:<id>` for board actors, fence memory and
  inbox delivery. They cannot change identity by changing a valid seat header. The host
  supplies current kind/roles and effective cross-board WIP. Unbound credentials retain
  their existing host contract, as planned; they do not acquire a registered profile.
- Board writes evaluate policy in their mutation transaction against current labels:
  create/edit/transition, claim/claim-next, complete, handoff, notes and dependencies.
  Existing lease, reviewer and human-only guards still apply. The focused enforcement
  test passed with no writes on refusal and label changes unable to evade an edit denial.
- Browser roster, editor, real visible-work read, key selection, OAuth binding controls
  and owner role/kind/policy forms are authored. New copy is site-owned; stale forms retain
  their submitted version. Account pages and navigation link to AgentHub agents.

The first real MCP identity/fence/capacity/session-revocation check and both initial
browser HTTP journeys passed (`absorption-agent-mcp-verified.log`,
`absorption-agent-browser.log`). Initial MCP fixture used a hosted free plan (one project); corrected to
single-environment mode. Its manual SSE reader then incorrectly selected an empty transport
event; corrected to select JSON events as the existing fixture does. These failed runs
are retained, not counted as passes. No CI, push or main delivery has occurred.

## Further source checks and transaction wiring

- Re-read pinned `principals.go`, `auth/auth.go`, `hubtest/policy_test.go` and the
  Agents, AgentEditor, Tokens, Policies, Roles, Login and Connect components. The original
  roster's first-1000-task approximation is replaced by the authorized aggregate plus
  paged visible claims; the browser test proves hidden project names and task titles are
  absent while the authorized total remains accurate.
- OAuth access, authorization-code exchange, refresh and device exchange now consult the
  live binding/profile. A revoked binding remains a rejecting row. Profile re-enable does
  not revive already-rotated access tokens. A test initially asserted against an old token
  after rotation; corrected to check the new token. The 13 existing OAuth tests passed.
- Handoff and human delegation validate canonical recipients within the board mutation
  transaction through a host-provided identity directory. Identity SQL stays in identity;
  AgentHub never reads auth tables. Disabled/missing/foreign/lapsed recipients refuse.
  Legacy seat names keep their existing unbound contract; registered actors require an
  enabled principal. Recipient validation does not update recipient last-seen.
- Browser delegation accepts registered identities and offers enabled owned profiles with
  display-name labels. Key metadata identifies its bound profile. OAuth forms show the
  current binding, and persisted bindings remain manageable after their tokens expire.
- Board settings and private/shared workflow changes apply human administration policy
  inside their transaction. The host's existing project administrator check still applies;
  its authorized human receives the policy admin role only for that resource. A shared
  definition checks every affected project's selector and rolls back the entire change
  if any project refuses. Registered agents cannot turn an admin role into human status.
- The owner profile editor now uses configured kind choices and role checkboxes. Disabled,
  invalid and stale submissions preserve their submitted values/version. User renaming
  still cannot change authority. Human accounts remain in the existing users administration;
  an agent editor never offers a human checkbox or an external-login override.

Local required-PostgreSQL results (no CI or push):

| Check | Result / retained log |
|---|---|
| Policy evaluator, catalog, resource enforcement, shared configuration policy | 7 passed; `target/absorption-policy-composition-verified.log` |
| Shared workflow mutation, concurrent edits, isolation, rollback | 5 passed; `target/absorption-policy-shared-workflows.log` |
| Agent identity, concurrent role removal, MCP shared-board capacity/handoff/revocation, browser administration/binding/delegation/hidden claims | 9 passed; `target/absorption-agent-acceptance-verified.log` |
| Existing OAuth exchange/refresh/device/hash-only storage regressions | 13 passed; `target/absorption-oauth-regressions.log` |

The MCP test initially addressed `board_get.version`; that response owns the value under
`ticket.version`. The corrected test passes and confirms refused handoffs leave it unchanged.
The configuration test initially used a nonexistent `SettingsEdit::default`; corrected to
exercise the real `put_settings` API. Failed logs remain; they are not acceptance receipts.

Still open: complete source operation accounting and A03 replay/surface integration;
source tailnet/proxy identity adapters and explicit disposition of source human-token login;
source role-member and full environment roster presentation; current-state policy/label
concurrency coverage; full credential-bound RLS/account-disable acceptance; 390/1280 visual
acceptance and final integrated deployment. The four browser checks passed again with configured kind choices and role checkboxes
(`absorption-agent-form-controls.log`); ten hub store/migration checks and five standalone
AgentHub migration checks passed (`absorption-agent-migrations.log`,
`absorption-policy-migrations.log`). All eight layout checks passed
(`absorption-agent-layout.log`). A02 remains partial, and all five source branches remain open.
