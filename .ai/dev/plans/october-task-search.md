# October baseline: restore task search across the app, MCP and CLI

Source: Go AgentHub baseline `46419c6`, specifically `a703761`'s
`server/internal/store/tasks.go` and `dcca3d3`'s board search. This remains the oldest
source branch under review; restoring search does not complete that branch.

Objective: people and agents can find a task by its ID, a comment, a title/description
substring, or PostgreSQL web-search terms across title and description. Browser, MCP
and bound CLI must read the same board through the existing AgentHub owner.

Dependencies: board-creation PR272; AgentHub search PR10. Parent branch
`increment-279/restore-task-search` starts after PR272 merges. No new database or
search service, no unrelated agent-profile/policy changes in this increment.

Tasks and acceptance:
- Merge backend PR10 after its PostgreSQL CI passes; pin the actual merged commit.
- Update MCP input description and browser search label to describe supported search.
- Verify browser comments are searchable, exact IDs and nonadjacent words work, labels
  still constrain results, and another account cannot find private tasks or notes.
- Verify a bound CLI creates a task, adds a note and finds it by ID and note through MCP.
- Exercise the search box in Edge at desktop/phone widths using isolated PostgreSQL.
- Run affected tests, applicable pre-push checks, review the diff and merge the parent PR
  after its checks pass. Deploy the resulting official image and test the Docker stack.

Risk: a note subquery missing scope correlation could reveal another board's note by
whether a task appears. Backend tests deliberately reuse the same task ID across boards
and tenants. Wildcard metacharacters retain literal behavior; other text uses the same
`simple` text-search dictionary as the original source. Queries remain parameterized.

Implementation: backend PR10 merged as `8fed602`; bound-CLI acceptance then exposed
that `--text` always became an output switch instead of a query. Follow-up PR11 fixes
that conflict and adds explicit `--human` output; it merged as `6912c3e`, the parent pin.
The browser label, MCP input documentation and user instructions now describe search.

Validation: all 94 backend tests and all 495 hub tests passed with isolated PostgreSQL
required, none ignored. This includes actual CLI parsing and browser/MCP/bound-CLI
search and account isolation. Backend GitHub PostgreSQL checks passed on both merges;
dependency audit and generated-contract/AI checks passed locally.
The remaining default workspace crates passed 291 tests (26 test/doc groups, none ignored),
including the actual executable's note/ID search. `cargo build --locked --bins`, Windows
E2E and stdio protocol passed. Edge exercised owner and ordinary-user board/task/dependency
creation, comments and ID/comment searches at 1280/390 widths on a single-environment hub
with isolated Docker PostgreSQL; both passed, without horizontal overflow.
Local Windows E2E does not run the Linux deployment or ClickHouse probe; those remain CI
gates. Parent push/PR/main and official Docker image acceptance remain pending.
