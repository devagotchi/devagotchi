# October source: owner-visible hub activity

The oldest source branch `46419c6` includes `a703761` server/internal/store/events.go
and `dcca3d3` web/src/pages/Audit.tsx: global attributed activity, filters, newest-first
history, chain verification and live updates. Board history was delivered in PR274,
merged at `bf74369` after all 18 pre-merge checks passed; its feature branch is deleted.
The original global administrative/machine history is still missing from the browser.

Branch: `increment-281/restore-hub-audit`, from `bf74369`. Extend the existing
`ops::audit::Audit` PostgreSQL chain; do not create another event store or writer.
Expose recorded calls and administrative actions through an owner-only page linked from
Hub settings, with actor/action/machine/outcome filters, cursor paging, details, explicit
verification and live updates. Preserve pending filter edits. Share the existing board
audit refresh script. Stream verification in one database query to bound memory while
preserving its snapshot and existing hash semantics.

Acceptance: actual PostgreSQL tests for filters beyond the first page, literal prefixes,
cursors and no writes; HTTP owner/non-owner/signed-out access, escaped data, session/store
failure, logout during a pending wait and visible tampering; Edge phone/desktop owner
workflow and ordinary-user denial. Rerun board audit browser behavior after sharing its
script. Applicable workspace/build/E2E/protocol/dependency/contracts checks precede push;
cross-platform, Docker and deployment CI precede merge and feature-branch deletion.

The user authorizes sequential absorption and merge. Global history is for the existing
hub owner only; account/board permissions do not become permission to inspect other users.
No change to recorded fields, identity assignment, retention or credential handling.
Original principal management, editable roles/policy and other oldest-source gaps remain
open; this page cannot be described as completion of the entire historical branch.

Implementation: complete for this scoped view. Validation: local suite passed; PR CI pending.
Push: not pushed. PR: not created.
Main: not merged. DELIVERY INCOMPLETE.

Local verification: the three focused HTTP/PostgreSQL tests passed after correcting the
failure-injection fixture to target its separate audit database. The first full default
cargo-test run exceeded the disposable PostgreSQL server's connection limit; its failed
receipt is retained. Repeat with four test threads, preserving every concurrency test's
own parallel work. Dependency audit and generated-contract/AI checks passed.

The complete default workspace rerun passed with `cargo test --locked -- --test-threads=4`:
888 tests, zero failures and zero ignored, including all 501 hub tests and 96 AgentHub
tests. PostgreSQL was required. The saturated initial run was stopped and retained;
no test was disabled or weakened. Binary build, Windows E2E, stdio protocol, formatting, generated contracts and AI checks
passed. Edge owner/user workflows passed board creation/search and both audit views,
including filters, verification, live refresh, edited filters retained after blur, and
ordinary-user denial of hub history. Desktop and phone layouts had no horizontal overflow.
Linux boundary, ClickHouse, deployment and all platform jobs remain PR CI gates.
