# Machine access execution detail

Part of Batch 2 in implementation.md, after the catalog storage/editor unit. This is
an execution refinement of the full S1-S5 plan, not a new backlog or completion claim.

## Source comparison completed so far

Read at S1 `46419c697964016eb547c60416a8f2f5e854a95f`:
`server/internal/access/{service,request,check,authorize,decide,sandbox,resume,resume_ack,revoke,reaper,util,defaults,read,policygate}.go`
and `bridge/scope/{scope,narrow,match,scope_test}.go`. Remaining routes/MCP, migrations, hub tests, platform sandbox/proxy and policy files
must be read before their code is authored. Current seams read: contracts link frames,
machine broker access checks and hub machine link calls. Broker ownership and keyboard
leases are real existing checks; they do not enforce the source selectors/sandbox.

## Corrections necessary to preserve the intended authority

- The source increments grant uses on every Authorize call and Live rejects a grant
  whose count reaches the maximum. A once-admitted process must retain its operation
  authority while a second start is refused. Add a durable admission identity bound to
  the original canonical request; retries recover that admission without consuming again.
  Continuation authority and permission to admit another operation are separate checks.
- Source narrowing treats empty paths/argv/actions as a subset, while Fits treats those
  empty lists as unrestricted. Use one typed meaning in both checks: an explicit empty
  allowed list permits nothing; absence means unrestricted only for the source's
  documented unrestricted fields. Hosts/environment variables absent or empty permit none.
  Keep tests for empty arrays, malformed scalar/list types, removed numeric limits,
  removed deny keys, argv element boundaries and wildcard host containment.
- Source Strs silently accepts scalars as an empty list, numeric reads default invalid
  values to zero, and unknown equal sandbox strings pass LevelWithin. Reject malformed
  typed selectors, levels and durations at parsing. Catalog JSON Schema remains an
  additional capability-specific check, never the only authority validation.
- Lexical path containment cannot prove OS access. Shared scope code may conservatively
  reject a request; the broker must resolve paths under its own OS, workspace and home,
  check link/reparse escapes, and execute inside an actually available sandbox. Do not
  claim shared string comparison confines filesystem access.
- Source authorization checks task lease identity but not every expiry/policy condition
  in the same query. Admission and continuation both check live principal, machine,
  policy, task generation/fence/lease expiry or guide state. Ending events and notification
  intents commit together. Reaper failures surface; lost notifications cannot extend the
  broker's bounded authority expiry.

## Concrete implementation sequence

1. `contracts/src/grants/{mod,selector,narrow,matching,risk,tests}.rs`, re-exported from
   `lib.rs`: closed capability/level vocabulary, parsed duration, typed selector values,
   immutable risk context and per-call effective authority. One shared scope evaluator
   serves hub and broker. Derive wire schemas where exported; add the actual family to
   the contract export root and regenerate once the complete wire shape is authored.
2. Catalog owner exposes composable reads/validation inside the access transaction;
   access does not query catalog tables directly. Add reference checks for actual access
   request/grant consumers before considering A05 covered. Replace the current
   `catalog/references.rs::default_selection` refusal with the live environment-wide
   unsandboxed policy and typed-confirmation check from source `access/defaults.go`;
   apply it on selection and edits that escalate an already selected default. The catalog's generic human
   CRUD never grants execution by itself.
3. `hub/src/machines/access/` owns request groups, decisions, grants, admitted calls,
   unsandboxed policies, resume acknowledgments and ending notification intents in
   PostgreSQL. Allocate identifiers outside retried transactions. Share identity/policy,
   machine directory, board lease and guide state through their owners. Every write has
   a version and canonical receipt; current authority precedes receipt replay.
4. Request expands preset items, validates all selectors/levels/duration/use bounds,
   requires a substantive reason and valid live task/guide binding for task duration,
   snapshots the matching risk explanations, and applies denial cooldown/expiry settings.
   GUI input requires observation authority; broad input/unsandboxed access needs a live
   explicitly acknowledged policy. Independent person approves/narrows or denies.
5. Admission chooses the least privilege covering grant and stores its canonical
   operation identity atomically with use consumption. Broker authority binds that
   identity, machine, principal, grant/version, selector/level, expiry and task fence.
   Keep the link version at 1: capability-negotiate extended frames and fail closed for
   a peer that cannot enforce them. Do not trust tool-supplied authority fields.
6. Integrate platform sandbox/proxy and broker operation admission/continuation/revoke
   before enabling the corresponding level. Existing local trusted IPC remains a
   distinct authority path. Actual platform tests must prove process termination,
   network host restriction and filesystem escape refusal.
7. Add the source HTTP/MCP access operations and owner/approval/machine browser flows
   after the domain contract is complete. Resume alerts are scoped to the person,
   deduplicated while open, and allow continue/revoke/pause; pause must stop applicable
   ongoing authority, not only alter a status label. Live checks and lifecycle endings
   must agree across all adapters.
8. Complete source change-policy documents, evaluation, evidence, independent reviews
   and bounded external decision provider in the separate owner already specified in
   implementation.md. Both internal and configured external decisions must permit.

## Acceptance

Port the source scope fixtures plus the corrected widening cases. Real PostgreSQL:
duplicate/retried once admission, concurrent last-use starts, task renewal/reclaim/end,
principal disable, current owner/policy on replay, independent approval, atomic groups,
decision narrowing, risk snapshot immutability, expiry/revocation, lost notification,
default preset and resume lifecycle. Real broker/native tests: no forged authority,
unsupported peer refusal, path/link escape, denied environment/network/argv, expiry and
revocation of active processes/GUI, continued read/input under admitted authority only.
Real UI/API/MCP checks cover owner/user/agent, exact origin and preserved refused drafts.

Status: source comparison and implementation instructions; A04 is still open. No claim
of enforced selectors/sandboxing, source closure, remote publication or main delivery.
