# Next October source feature: board audit history

Source a703761 store/events.go and dcca3d3 web/src/pages/Audit.tsx offer actor/action-prefix
filters, newest-first event history, event detail and chain verification, with live updates.
Rust already owns board events and verification but lacks a browser audit page and filtered,
newest-first queries. Extend that existing owner; preserve all board authorization.

Backend increment-280 will add a bounded EventFilter query with actor, action prefix,
ticket, exclusive after/before sequence cursors and descending order. Existing events()
semantics and full chain verification must remain unchanged. Filter in PostgreSQL before
limiting; escape literal wildcard characters. Tests must include filtering beyond the first
page, order/cursors, board/tenant isolation, no mutations, and chain verification after paging.

Parent increment-280 will add Audit on each board and a browser page in the existing app
frame, with localized labels, filters, event data escaped, task links, older-page cursor,
chain verification status and a refresh control. Live notification must use existing board
changes and recheck session/board access before returning any new result. Surface failures
without fabricated empty history. Shared-board viewers may read but gain no mutation access.
MCP board_events should expose the same filters, keeping its existing defaults.

Acceptance includes HTTP owner/member/viewer/foreign-account checks, actual PostgreSQL
permission failure, forged HTML escaped, invalid cursors refused, chain tampering visibly
reported, new events visible on live refresh, and Edge desktop/phone workflow with no overflow.

This restores board audit behavior only. The original global admin/principal/machine event
view also needs an owner-authorized hub call-audit interface through ops::audit; keep that gap
explicit until delivered. No historical source branch is complete from this one feature.

Delivery: backend PR12 merged as `fce03e2`, all 96 AgentHub tests and its PostgreSQL CI
passed. Parent branch `increment-280/restore-audit-history` starts from merged search
PR273, main `2ce4e91`. Existing event reads and complete-chain verification retain their
semantics; the new filtered query applies scope and filters before limits.

Parent implementation: board Audit history link, localized filters/details/older pages,
explicit integrity check, and live waits on existing board change notifications. The wait
rechecks browser session and board access after wakeup. Older pages do not auto-refresh;
typing a filter shows a new-events link instead of discarding input. Verification is explicit
so each background update does not scan the entire chain.

Three focused PostgreSQL HTTP/MCP tests passed: 103 task events paginate correctly,
filter/cursor inputs work, another account is refused, storage errors are 503, tampering
is visibly reported, and sign-out during a pending wait prevents delivery of new events.
All 498 hub tests passed, including shared-board viewer access and removal in the existing
organization test. The other default workspace crates passed 291 tests (none ignored).
Dependency audit, binary build, Windows E2E, stdio protocol, generated contracts, formatting
and AI checks passed. Edge acceptance passed for owner and ordinary user at desktop/phone
widths: board/task creation, dependencies, ID/comment search, filtered audit, explicit chain
verification, live refresh, and edited filters retained after focus moves. The final actor
display adjustment passed the three focused audit tests and a repeated complete browser run.
Parent PR274 merged as `bf74369ff25a65bfc2820d9e93fa28d2070f1b76` after all 18
pre-merge CI/security checks passed. Local and remote feature branches were deleted.
The official Docker image release is building; runtime acceptance of that image remains.

Delivery caveat found while checking merged PRs 272/273 and backend PR12: their pre-merge
build/test/security checks passed, but the separate post-merge `notify` workflow fails because
its Resend credential and sender configuration are absent. This is not a passing workflow
and has not been hidden or described as green. No email configuration is changed here.
