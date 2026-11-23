# A01 source comparison and authored changes

## Restore the source board experience (increment-290)

The operator rejected the deployed interface on 2026-11-22 and requested immediate
implementation from the original design. Source `46419c6`, `web/src/pages/Board.tsx`
and `TaskCard.tsx`, has workflow columns, state counts, completed tasks, a held-by-me
filter, and cards exposing holder/lease, blockers, assignment, labels and priority.
Main `18d3ee2` instead renders a flat open-only list. Restore these behaviors through
the existing scoped PostgreSQL board reader; preserve task links, search, configuration,
creation and audit actions. No new task store, agent scheduler or workflow authority.

Author the board render/copy/styles and HTTP/browser coverage together. Wide screens
show workflow columns; phone screens retain readable cards without page overflow.
State labels/order come from stored workflow configuration. Filters must preserve
each other; a completed task remains reachable; foreign accounts remain isolated;
untrusted labels and titles stay escaped. Show bounded-result counts honestly.
Validate focused PostgreSQL/HTTP tests and desktop/phone browser behavior, then publish
the completed screen change on green CI and replace the existing development image.
This delivery is a source-derived board restoration, not closure of S1-S5 or the
software-factory requirements. Source refs remain preserved until full closure.

Local verification: 24 authenticated AgentHub PostgreSQL regressions and 17 site/copy
tests passed, along with the actual-binary browser journey (desktop columns, phone state selection, closed
task visibility, comment/close/undo and existing machine/account flows). Final review
also preserves the other states' filtered counts when a state is selected; the final
PostgreSQL rerun includes that regression. CI, merge
and replacement-image acceptance remain delivery gates, not completed assertions.

Initial PR281 CI found the new board used a viewport breakpoint, violating the existing
page-layout contract. Use the shared `query` container primitive and a container query
instead. All four styling-contract tests and the complete desktop/phone browser journey
pass on the corrected binary. No contract test was weakened.

## Earlier S1 authoring ledger

Source: S1 `46419c697964016eb547c60416a8f2f5e854a95f`. This is a work ledger, not a
completion receipt. Remaining policy, replay and identity dependencies keep these files
open even where an individual operation already exists. The original whole-batch
CI/push hold below records the earlier S1 plan; the scoped increment-290 delivery
above follows the operator's subsequent instruction to implement and deliver now.

## Authored on the local S1 branch

| Source behavior | Current target and correction | Acceptance |
|---|---|---|
| `store/edit.go`: update checks the live holder's fence as well as version; accepts progress | `crates/agenthub/src/hosted/edits.rs`, `tickets.rs`, `ticket.rs`; MCP schema/handler and CLI local/remote adapters forward fence/progress. Human metadata edits retain current board authority; other agents cannot overwrite a live holder. Configurable policy comes in A02. | `tests/held_updates.rs`, CLI `tests/held_updates.rs`, hub MCP `tests/held_updates.rs`: stale/omitted fence, reclaimed lease, correct fence, version conflict, no audit writes on refusal, progress without renewal, omission versus explicit null |
| `store/notes.go`: any supplied stale fence refuses, including a previous holder | `hosted/notes.rs` uses the shared supplied-fence guard before writing | stale previous-holder note leaves notes/events unchanged; independent unfenced observation remains allowed |
| `store/hint.go`, default preset: configured heartbeat hint capped by actual TTL | `settings.rs`, `hosted/leases.rs`, `board/prime.rs`; versioned browser settings expose the hint | `tests/runtime_settings.rs`: custom hint used by claim/heartbeat/prime; shorter actual TTL caps it; invalid values cause no writes |
| `store/notes.go`, default preset: configured note kinds, empty permits any | `settings.rs`, `hosted/notes.rs`; browser settings expose allowed kinds | `tests/runtime_settings.rs`: allowed/unknown/empty list, duplicate/invalid configuration, lifecycle summary still written, legacy settings decode |
| `presets/default.json`, `TaskCard.tsx`: configurable priority names | `settings.rs`; browser settings save names and board/ticket pages display them | priority-label bounds/fallback test and real browser save/read/escaped label test passed; priority numbers and sorting are unchanged |

The default note list contains the source's note/handoff/blocker/decision/summary plus
the existing Rust review/question types. Existing stored settings without this field
retain their prior unrestricted note behavior; newly created boards get the configured
default. This preserves existing data while restoring the editable source constraint.

Initial local run: four backend/CLI fence tests passed. The MCP test failed because its
fixture read `ClaimOut` as a bare `Lease`; corrected to `structuredContent.lease`. The failed
log is retained privately. Final focused results must be recorded after the whole current
behavior batch passes; no source row is closed by the initial results.

Subsequent focused checks passed: five backend/CLI fence-and-note tests, one real MCP
test, two runtime-setting tests and the browser settings POST/conflict test (9 total).
Next local library run: 123 passed; the refusal-help coverage test found the newly restored
`not_completable` code had no CLI next-step mapping. Added explicit CLI/MCP repair guidance
and a conflict exit classification. The affected refusal-help test passed, followed by
all 57 hub AgentHub tests (real PostgreSQL, including MCP, bound CLI and browser HTTP).
The progress panel is exercised through an authenticated page request and escapes script
content. Full workspace, native visual, Docker and CI acceptance remain held for R01.

## Further source-backed work found during comparison

- `ops.go`: completion without an active lane now refuses; the new local regression passed
  with no ticket/note/event changes. Public workflow validation already rejects active
  states without a lane. The corrupted-storage case no longer reports success.
- `ops.go`: handoff now records `last_worker`; the new local regression passed attribution
  and distinct-reviewer checks after a review handoff. Force-release also retains the
  human's reason as a ticket note, and agents cannot use the human delegation API to
  bypass lease fencing; its new local regression passed.
- `ops.go`: handoff target must be an enabled principal. A02 must supply credential-bound
  principal lookup; aliases alone do not close this requirement.
- `admin_config.go`, `config_read.go`, `admin_misc.go`, `conf.go`, `sort.go`: roles and
  policy configuration/read/matching belong to A02. Existing shared workflow CRUD covers
  only the workflow part; project removal and in-use checks still need per-surface review.
- `admin_misc.go`, presets: priority labels are now authored and locally checked; remaining
  general versioned settings still need operation-level mapping.
  Dedicated grant/schema settings must use their A04/A05/A07 owners, not generic unsafe CRUD.
- `reaper.go`: current event-driven expiry is the replacement for fixed-interval polling;
  verify equivalent expiry timing/failure behavior. Original timed receipt deletion must
  be reconciled with A03 replay lifetime, never silently re-execute retired requests.
- `tasks.go`: current search/list/dependency reads need source-filter and ordering evidence;
  existing tests are candidates, not a reason to mark the entire file equivalent.
- `seed.go`: preserve non-overwrite semantics across all seeded catalogs. Source-specific
  human names and tokens are not public installation defaults.
- Remaining A01 source tests and web components still require operation-level mapping.

## Ref inventory read during this work

Canonical GitHub repos each exposed only `main`: root `4b01ac9`, AgentHub `8a6db52`.
Preserved Go refs point at the four recorded source heads. Preserved Rust additionally
has `increment-96/live-machine-capabilities` and local/main at `6282071`, the S5 base;
their provenance and earlier ports must be verified in E01 before deleting any source ref.
This does not mean the source code is already merged: the preserved source queue remains
open. Original dirty checkouts have not been changed.
