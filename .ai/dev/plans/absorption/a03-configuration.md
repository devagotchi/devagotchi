# Workflow and board configuration adapter review

Pinned S1 `46419c697964016eb547c60416a8f2f5e854a95f`: `httpapi/admin.go`
registers POST/PUT/DELETE workflows; `store/admin_config.go::PutWorkflow` creates a
validated independent definition or updates its current version. `admin_misc.go` refuses
deletion when projects use it. `web/src/pages/Workflows.tsx` edits the selected definition
and displays the projects using it. The current web editor already belongs to
`web/app/agenthub/configuration/shared/`; do not replace it with a parallel UI.

## Authoring

- Added `HostedBoard::create_shared_workflow` and its synchronous `Board` face. Unlike
  publishing a board's current workflow, creation does not bind or change a board. It
  takes the existing namespace/board locks, validates the definition, authorizes current
  policy, reserves the existing receipt, inserts once and appends the publishing event
  on the source board. Concurrent same-key requests replay one result.
- New `agenthub/http/configuration/` routes delegate to that owner and the existing
  edit/delete/bind/detach and versioned settings/private-workflow operations. No second
  configuration database, scheduler or interpretation layer.
- Catalog scope is the complete account-owner namespace, selected through a reachable
  account board. People granted one configured shared board or an organization board
  cannot use that grant to disclose/change a whole owner catalog. Existing configuration
  pages keep their own scope checks. Agent credentials are not people.
- Settings accept the existing `SettingsEdit`, excluding standing approvals. Workflow
  binding checks both the board version and selected catalog version. Path/body and
  If-Match/body mismatches refuse. The canonical receipt input includes a deleted ID;
  an old deletion receipt cannot accidentally apply to a different definition.

Routes and payloads are recorded in `docs/agenthub-http.md`. These are authored local
routes. Project lifecycle was subsequently authored and checked in
[a03-projects.md](a03-projects.md), including the retained catalog audit-anchor guard.
Source aggregate `/config` remains open. Independent catalog creation makes the audit
reference explicit; deleting its anchor cannot break later catalog edit/delete events.

## Validation

Authored real HTTP/PostgreSQL checks in `http/tests/configuration.rs`: concurrent create,
same-key replay without binding or extra event, changed-input conflict, catalog visibility
from both account boards, binding two boards, atomic shared edits, in-use deletion,
private-edit refusal while bound, detaching without losing the effective definition,
deletion replay, chain verification, versioned settings, invalid workflow, path mismatch,
agent and cross-account refusal.

Focused command passed on the first run:

`cargo test --locked -p devagotchi-hub -p devagotchi-agenthub --lib -- agenthub::http::tests::configuration tests::shared_workflows tests::configuration --test-threads=2`

Fourteen AgentHub library checks and six hub checks passed, zero failed/ignored. This
includes the three new HTTP scenarios and existing configuration, policy, receipt,
sharing, isolation and browser authority/origin regressions. Build took 2m56s; test
execution took 10.16s and 8.01s. Log: `target/absorption-workflow-administration.log`.
No CI/push/main delivery is claimed.
