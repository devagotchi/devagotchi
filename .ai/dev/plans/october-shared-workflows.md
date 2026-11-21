# Restore shared-workflow browser management

Continue oldest source46419c6, specifically admin_config.go PutWorkflow/PutProject,
admin_misc.go DeleteConfig and Workflows.tsx's selection and used-by list. Branch:
increment-285/shared-workflow-browser. Backend AgentHub PR14 passed PostgreSQL CI and
merged dd4b723; its reviewed tree matches main and its feature branch is deleted.
Parent PR277 merged c52e773, passed CI and official Release44833139414, and is deployed
with no-JS Edge configuration, existing-user, health and restart-persistence acceptance.

Objective: let a person publish a board workflow, use it on another owned board, see all
boards using it, edit that shared definition, detach a current independent copy and delete
an unused definition. Extend the existing configuration domain/editor and PostgreSQL
owner; no separate service, web SQL or duplicate editor. Existing private configurations
remain private until explicitly shared. Self-hosted remains one environment with users.

Authority: catalog reads and writes require a human account-owned board resolved by
the existing live board_access mechanism (Kind::Account, Role::Owner). A grant to
administer one shared project does not confer authority over its owner's other boards.
Every mutation retains same-origin, live-session and waiting-account checks. Settings
permissions stay unchanged. No organization UI is introduced.

Forms show versioned selections and affected boards. Shared editing reuses labelled
state/lane/manual-move fields and draft-only row controls. Stale or invalid saves retain
the submitted draft and require explicit reload; no silently refreshed expected version.
A bound board directs people to shared editing or detachment. Delete is refused while
in use, including crafted requests. Storage failures remain503; unknown/account-isolated
resources remain404. All output is escaped and inputs bounded.

Browser acceptance exposed raw state keys on task and board pages. Those pages now read
the effective workflow's display labels; filters and submissions still use immutable
state keys. Route/browser tests cover custom labels and HTML escaping.

Milestones: adopt backend and parent MCP refusal/scanner fixes; shared forms and labelled
navigation; PostgreSQL route tests and no-JS browser acceptance; full applicable local
gates; green CI, merge and branch deletion; official immutable Docker deployment and
existing-user/shared-workflow acceptance after restart. The E2E process fixture fix uses
the already-required Python interpreter instead of Unix sleep, retaining list/kill proof.

Acceptance: two owned boards observe an edited definition and actual task behavior;
one incompatible used state refuses every board's update; stale editor and binding
refuse without losing drafts; detachment freezes the current definition; in-use deletion
refuses, unused deletion preserves audit. Cover cross-account names, project-admin
boundary, CSRF, signed-out/disabled sessions, malformed/duplicate/unknown fields, real
storage failure, escaping, no-op events and 390/1280 widths with JavaScript disabled.

Validation: focused PostgreSQL route tests, full required-PostgreSQL default workspace
tests, binary build, cargo deny, Windows E2E/protocol, contracts, AI/fmt/layout/diff.
Linux IPC/MCP smoke and deployment checks run in CI. Backend preparation already passed
911 workspace tests, 108 AgentHub/510 hub, build/E2E/protocol/deny/contracts/AI/fmt; parent
UI needs its own final evidence. Failed disk/scanner/Windows lock/E2E receipts are retained.

Risks: permission expansion across boards, stale binding/editor writes, misleading private
editor on a bound board, and lost drafts. Use backend atomic/versioned methods; the host
proves namespace authority and never manufactures success. Reconcile old parity claims
against code: credential-bound agents, global principal WIP, editable policies, generic
mutation replay, retention/archives, catalogs/guides/subscriptions remain open. This PR
does not close or delete the historical source branch.

Implementation: complete for the scoped browser change. Nine configuration tests and
then all20 AgentHub page tests passed with required PostgreSQL. First two-board fixture
hit its hosted Free-plan limit; that test now explicitly allows2 projects. First browser
run found raw state keys on task pages; display-name wiring and escaped-label assertions
fixed the behavior. Complete native Edge acceptance passed with JavaScript disabled:
publication, two-board binding/edits, task creation using changed rules, incompatible edits,
stale drafts/reload, detach, in-use/unused deletion, audit/account boundaries and390/1280
layouts. Phone and desktop screenshots were inspected. Final build and all913 default
workspace tests passed with required PostgreSQL (108 AgentHub,514 hub), including the
database cleanup test after its checkpoint completed. Windows E2E, MCP protocol with
absolute paths, generated contracts, AI checks, formatting and diff checks passed.
Dependency audit passed during backend preparation; dependency manifests/lockfile are
unchanged. Two protocol invocations with relative arguments were refused by the harness;
only the subsequent successful invocation is passing evidence. Linux IPC/MCP smoke and
ClickHouse process proof remain CI gates. Push, PR, main and Docker: pending.
DELIVERY INCOMPLETE.
