# Restore board configuration from the oldest October source

Continue source `46419c6`, specifically `a703761` configuration versions and the
`dcca3d3` workflow/settings screens. AgentHub backend PR13 merged green as
`49387c3d32c1ef811ff339ce434e2e24c58c1bc1`; local and remote feature branches are deleted.
Parent branch: `increment-283/restore-board-configuration`.

Expose the existing board's settings and workflow through labelled browser forms. Lease
durations are shown in seconds, with explanations for work limits and expiry escalation.
Workflow forms cover state names/types, initial/attention states, work stages and manual
transitions. Add/remove rows retain the unsaved form; no JavaScript or raw JSON is required.
Saving changes uses the version the person opened. Conflicts retain the submitted draft
and require an explicit reload before another save; they cannot overwrite newer work.

Use the existing authenticated board resolver and require its configuration role on every
read/write. A POST also requires the hub's exact origin and the existing waiting gate.
Render escaped site copy in the application frame, and call the AgentHub owner for every
read/write. No web SQL, new configuration store, organization UI or hardcoded credentials.
Standing approvals remain independently managed. Changes and refusals follow the existing
PostgreSQL transaction/audit rules. Board links appear only for people allowed to configure.
The shared from_this_hub helper now requires an exact Origin and rejects conflicting fetch
metadata; the HTML frame uses same-origin referrers so browser POSTs supply that Origin.
Both changes follow the existing hub-pages pattern and require broader authentication tests.
The form parser rejects duplicate/unknown fields, more than 512 rows per section, and bodies
over 1 MiB. It never silently truncates a configuration; lease units round-trip to milliseconds.

Milestones: adopt the merged backend; add handlers/parser/rendering and localized copy;
prove real HTTP edits, conflicts, role/account isolation, origin refusal, draft retention,
storage failures and unchanged audit on refusals; accept the forms in Edge at 390/1280;
run applicable validation, publish, wait for green CI, merge and delete the feature branch;
deploy the official immutable image and accept the same flows against Docker.

Backend validation already passed all102 AgentHub tests and all898 default workspace tests
with required PostgreSQL, binary build, Windows E2E, protocol, dependency audit, generated
contracts, AI checks and formatting. Parent changes require their own applicable final run.
Linux boundary/MCP smoke and deployment checks remain CI gates, not local Windows claims.

Risks: stale configuration must never erase approvals or newer settings; workflow edits
must not strand active work; parser limits/errors must not silently truncate a submitted
workflow; template output must escape data; unavailable storage must return503.

Non-goals: shared workflow registry/project rebinding, agent identity and credential
bindings, global principal WIP, editable policies, and remaining later source features.
These remain explicit parity gaps. This increment does not close the historical source.

PR276 local-user administration is merged as9c6dc6e and deployed from successful official
Release44823993998. Edge create/rename/disable/enable/reset, revoked sessions, owner access,
390/1280 and existing users/restart persistence passed; owner credentials are unchanged.

Forms implemented; five focused tests passed against required PostgreSQL. They cover
exact duration conversion, no-op/stale/invalid edits, draft-only row controls, real storage
refusals, account isolation, origin/session checks and immediate role demotion/reinstatement.
Browser acceptance first stopped at the manual-move dropdown: the row existed, but wrapping
the select in its label included option text in the accessible label. Explicit for/id labels
correct that markup. The complete Edge flow then passed with JavaScript disabled, including
settings/workflow saves, add/remove controls, validation, conflict recovery, audit and account
isolation at 390/1280. Phone screenshots were inspected. Failed receipts are retained.

Final local validation: all 903 default workspace tests passed with required PostgreSQL,
including 510 hub and 102 AgentHub tests. Binary build, Windows E2E, protocol, dependency
audit, generated contracts, AI contracts, formatting and diff checks passed. An earlier
parallel run hit Windows' executable lock while the E2E server held the binary; it is not
counted as passed. The complete sequential rerun passed after the server exited normally.
Parent PR277 passed all18 checks and merged as c52e773e74c39dda6df16331400f0c3cfc2ee89b.
Its reviewed tree matches main; the feature branch is deleted. Main CI44833139317,
security44833139256 and official Release44833139414 succeeded, including Linux gates.
The immutable official image was deployed to the development Docker hub. Existing-owner
and user login, owner-only boundaries, absent organization routes, PostgreSQL/ClickHouse/
audit health, no-JS Edge at390/1280 and configuration persistence after restart passed.
Implementation, validation, push, PR, main and development Docker: complete for this
scoped change. Historical source absorption remains incomplete; shared workflows are
tracked by the dependent [shared-workflow plan](october-shared-workflows.md).
