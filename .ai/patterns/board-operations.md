# Pattern: a board operation

How an AgentHub operation is defined, stored, put on the hash chain, served and tested. Follow
it for every new operation and every change to one: claim, claim-next, heartbeat, note,
handoff, release, complete, force-release, create, update, delegate, undo-close, deps,
approvals, standing grants, settings, wakes and issue context.

This pattern states only what is particular to boards. The shared rules G1 to G14 are in
[README.md](README.md#shared-rules-stated-once-apply-everywhere) and hold here too. Where a
rule below says "G<n>", the shared rule is the base and this file adds the board's specifics.

Related documents:

- Contract: [agent board](../../docs/agent-board.md) §4 (item 8: "Every mutation appends its
  event in the same transaction", and the stable codes), §6 (surfaces) and §10 (ticket
  origins).
- Where files go in general: [layout.md](layout.md).
- The form of every MCP tool (names, titles, hints, closed and documented inputs, object
  outputs): [mcp-tools.md](mcp-tools.md).
- Migrations, RLS and store fixtures: [persistence.md](persistence.md) and
  [tests-and-ci.md](tests-and-ci.md).

References below name a file and a function, not a line: lines move, names are checked by the
compiler. Paths under `agenthub` are in the AgentHub submodule (its own repository,
`devagotchi/agenthub`).

## Purpose

One operation is one Postgres transaction, one event on the hash chain for each action it
performs, and one stable refusal code. A person or agent reaches it through any surface (hub
MCP, the `devagotchi board` CLI on its local or remote face, a page, a REST route, a hook) and
gets the same answer. `HostedBoard` decides (G1). Every other layer forwards.

## The layers

| Layer | Home | Shape |
|---|---|---|
| Store (decides) | `agenthub/src/hosted/<subject>.rs` | `impl HostedBoard { pub async fn op(&self, actor: &Actor<'_>, ...) -> BridgeResult<T> }` |
| Sync face | `agenthub/src/board/<subject>.rs`, `lease.rs` for lease verbs, `approval.rs` and `wake.rs` for theirs | one line: `self.hosted_call(self.hosted.op(...))` |
| CLI face | `agenthub/src/face.rs` and `cli/remote.rs` | a `Face` method; `Board` forwards; `Remote` makes one `board_<tool>` call |
| CLI command | `agenthub/src/cli/commands/<group>.rs`, dispatched in `commands/mod.rs` | parse, `fence_for`, `with_id(id, board.op(...))`, claim cache, `output.value` |
| MCP tool | `engine/hub/src/agenthub/mcp/tools/<group>.rs`; input in `mcp/schema.rs` | `#[tool]`, then `board_call(tool, project, ticket, ctx, move \|board, actor\| board.op(...))` |
| Page or REST | `engine/hub/src/web/app/agenthub/`, `web/approvals/`, `engine/hub/src/agenthub/issue_lifecycle/` | a page: `spawn_blocking` and one `board.op`; an async REST route: one awaited `HostedBoard` method; either way one audit line |
| Hook | `agenthub/src/cli/hooks/`, shipped as `plugins/devagotchi-agenthub/hooks/hooks.json` | `plugin_hooks()` prints it; the plugin ships exactly that |

## Exemplars

1. **`HostedBoard::handoff`** (`agenthub/src/hosted/leases.rs`), end to end. It touches
   every concern: a pure check, the fence guard, a multi-row write, one event, refusal codes,
   the CLI claim cache and the hub's fence memory. `docs/agent-board.md` §4 item 5 makes
   "handoff note + release + assignee, one transaction" its contract.
   - `scope` (`hosted/mod.rs`) takes the tenant scope, locks the board row `FOR UPDATE` and
     returns the database clock.
   - The fence guard is `lease::check_held` (`agenthub/src/lease.rs`).
   - `clear_lease` (`hosted/leases.rs`) writes the one event through `event`
     (`hosted/mod.rs`): `board_events`, `board_outbox` and `pg_notify` in the same
     transaction. `clear_lease` owns `data.from` and `data.to`; the recipient goes under
     `data.assignee`.
   - Sync face: `Board::handoff` (`lease.rs`). CLI faces: `Face::handoff` (`face.rs`),
     `Remote::handoff` (`cli/remote.rs`, one `board_handoff` call). CLI command: `handoff` in
     `cli/commands/leases.rs`. MCP tool: `board_handoff` (`mcp/tools/tickets.rs`, input
     `HandoffIn`), through `board_call` / `board_call_scoped` (`mcp/calls.rs`), whose actor
     comes from `actor_for`; refusals become `BoardError` through `refuse` (`mcp/refusals.rs`).
2. **`HostedBoard::decide_and_allow`** (`hosted/approvals.rs`): two actions in one
   transaction. `decide_in` and `grant_in` are the shared in-transaction steps that `ask`,
   `decide_approval` and `grant_standing` reuse; the standing id is made before the
   transaction (rule 10). A refused grant (`too_many`) leaves the approval undecided.
3. **`HostedBoard::set_wake`** (`hosted/inbox.rs`): a read-modify-write of one settings row
   in one transaction, its secret made before it and kept off the chain, a removal of nothing
   writing nothing.

Idempotency models, for an operation a caller may retry:

- A natural key: `create_in` with a source (`hosted/tickets.rs`, `sourced`).
- A caller-supplied request id with a stored receipt: `write_issue_context`
  (`hosted/issues.rs`).
- In both, the repeat returns what exists and writes nothing, and changed content is a conflict
  code (`source_conflict`, `request_conflict`) refused before any write.

## Canonical shape

```rust
// agenthub/src/hosted/leases.rs
impl HostedBoard {
    /// What it does and the rule it keeps, in one or two lines.
    pub async fn handoff(&self, actor: &Actor<'_>, id: &str, fence: i64, note: &str,
                         to: Option<&str>) -> BridgeResult<Ticket> {
        let note = note.trim();                       // 1. pure checks, before the transaction
        if note.is_empty() { return Err(invalid("a handoff needs a note for whoever picks it up")); }
        transaction!(&self.store, |connection| {      // 2. ONE transaction; the body may run again
            let now = self.scope(connection).await?;  // 3. tenant scope, board lock, database clock
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            crate::lease::check_held(&ticket, actor.id, Some(fence))?;   // 4. the shared guard
            self.insert_note(connection, actor.id, now, id, "handoff", note, to, Some(fence)).await?;
            sqlx::query("UPDATE board_tickets SET assignee=$4 WHERE tenant_id=$1 AND board_id=$2 AND id=$3")
                .bind(&self.tenant).bind(&self.project).bind(id).bind(to)  // 5. tenant and board from self
                .execute(&mut *connection).await?;
            self.clear_lease(connection, actor.id, now, &ticket,
                &crate::lease::release_state(&workflow, &ticket),
                actions::TICKET_HANDED_OFF,                              // 6. one action, one event;
                serde_json::json!({"assignee": to})).await?;             //    clear_lease owns from/to
            self.ticket(connection, id, &workflow, now).await              // 7. re-read and return
        })
    }
}

// agenthub/src/lease.rs
impl Board {
    /// A handoff note, a release, and an assignee, in one transaction.
    pub fn handoff(&self, actor: &Actor<'_>, id: &str, fence: i64, note: &str, to: Option<&str>)
        -> BridgeResult<Ticket> { self.hosted_call(self.hosted.handoff(actor, id, fence, note, to)) }
}

// engine/hub/src/agenthub/mcp/tools/tickets.rs
#[tool(name = "board_handoff", title = "Hand off a ticket", description = "...",
       annotations(title = "Hand off a ticket", read_only_hint = false, destructive_hint = false,
                   idempotent_hint = false, open_world_hint = false))]
async fn board_handoff(&self, Parameters(input): Parameters<HandoffIn>,
                       context: RequestContext<RoleServer>)
                       -> Result<Json<Ticket>, Json<BoardError>> {   // spelled out: rmcp reads it
    let subject = self.subject_id(&context)?;        // the fence memory's key
    // The Result goes into the closure, so a missing fence is refused there and audited (G7).
    let fence = self.boards.fence_for(&subject, &input.project, &input.id, input.fence);
    let id = input.id.clone();
    let result = self.board_call("board_handoff", &input.project, Some(&input.id), &context,
        move |board, actor| board.handoff(actor, &id, fence?, &input.note, input.to.as_deref())).await?;
    self.boards.forget_fence(&subject, &input.project, &input.id); // after the call returns
    Ok(result)
}
```

## Rules

1. **One decider (G1).** Only `HostedBoard` (`agenthub/src/hosted/`) runs SQL on
   `board_*` tables. The hub asks `hosted::registry` (`names`, `boards`, `has_board`,
   `lease_delays`) and `HostedBoard::events_page` for what it needs across boards.
   - `Board`, `Face`, `Remote`, MCP tools, CLI commands, pages and REST routes each make exactly
     one call per user intent, and add no logic beyond forwarding.
   - When a surface needs two writes (a decision plus a standing grant, an assignment plus a
     note, a default plus a create, a read-modify-write of settings or wakes, a revoke for each
     of several grants), that is a new `HostedBoard` operation.
2. **One operation, one `transaction!`.**
   - Every read the decision depends on, and every write, happens inside it.
   - Checks that need no database (lengths, enum parsing, `actor.human`, a name's form) run
     before it.
3. **Scope first, then the database clock only (G9).**
   - The block's first statement is `let now = self.scope(connection).await?;`.
   - Every time the board stores or decides on (expiry, liveness, a standing approval's
     window, the undo window) comes from that `now`. `system_clock` and a page's clock never
     decide. A read that filters by time (`live_standing`, `suggestions`) filters inside its
     transaction.
   - Display-only arithmetic ("12m left", a page's countdown) may use the face's clock:
     `Face::now` exists for that and nothing else.
4. **Tenant, board and actor come from authentication (G8).**
   - Every statement binds `tenant_id=$1 AND board_id=$2` from `self.tenant` and
     `self.project`.
   - Every operation takes `&Actor<'_>`, never an actor string. The actor is the one the
     surface resolved: `actor_for` on MCP and the issue routes, `Actor::human(&subject.id)` on
     pages.
5. **Guards are the shared ones.**
   - A held ticket changes only after `lease::check_held(&ticket, actor.id, Some(fence))`.
   - An unheld edit compares `version` and refuses an approval (`unheld_edit`,
     `hosted/tickets.rs`: `version_conflict`, `approval_decide_only`).
   - A person-only act checks `actor.human` (`humans_only`).
   - An approval changes only through `decide_in`.
   - The WIP limit is `check_wip` (`hosted/leases.rs`), from claim and claim-next alike.
6. **One event per action, none for a no-op.**
   - Each action in `chain::actions` is one event. An operation appends one event for each
     action it performs, inside its transaction, through `self.event(...)` directly or through
     `clear_lease` / `expire`. That writes `board_events`, `board_outbox` and `pg_notify`
     together.
   - Handoff is one action (`ticket.handed_off`, covering its note, assignee and release).
     An ask that a standing grant covers is two (`approval.requested`, `approval.decided`).
     Approve-and-allow is two (`approval.decided`, `approval.standing_granted`). Delegate is
     one (`ticket.updated` with `assignee` and `note_seq`).
   - An unchanged request returns the current state, writes nothing and announces nothing.
     That covers a repeat, an empty patch, an existing dependency, settings or a workflow equal
     to the current ones, re-claiming what you hold, a sourced create with the same inputs,
     removing a webhook that is not there, and revoking what is already ended.
   - Two exceptions, both documented here: board initialization (the chain starts with the
     board's first operation, `board.welcomed` for a new account board) and per-reader inbox
     cursors, which are not board state.
7. **Action names are `<subject>.<past participle>`**, as `pub const` in `chain::actions`,
   listed in `chain::actions::ALL`. Subjects: `ticket`, `lease`, `note`, `dep`, `approval`,
   `board`, `issue`, `wake`. One word of the verb is the participle (`settings_changed`,
   `handed_off`, `set`).
   - `data` names what changed: `from` / `to` for states, the changed fields, the recipient
     under its own key. A helper's keys are reserved (`clear_lease` owns `from` and `to`).
     `board.settings_replaced` names its changed keys (`{changed: [...]}`).
   - `data` never carries a secret (G8). A wake's secret stays out (`set_wake` records only
     recipient and url).
   - The action is also the outbox `kind` that signal sinks and ClickHouse receive. In alpha
     a rename is allowed and goes in the release notes (C17, D7). Events already on a chain
     keep their old names, because the hash covers the action; the chain stays additive.
8. **Return the record as it is now**, re-read inside the transaction (`self.ticket(...)`,
   `self.deps(...)`, `self.approval_in(...)`).
9. **Refusals are stable codes (G4).**
   - The store refuses with `fail("snake_code", message)`, or `invalid(message)` for bad input.
     Stored data that does not decode is `board_damaged`, never read as empty (`decode`).
   - Every code the store can return gets a `next` of its own from the MCP `refuse`
     (`engine/hub/src/agenthub/mcp/refusals.rs`): a line in `next_for`, or one of the cases
     `refuse` builds from the message (`invalid_settings`, `unknown_environment`).
   - Every code a CLI command can reach has a line in the CLI `next_for` and a deliberate class
     in `exit_code_for` (`agenthub/src/cli/errors.rs`). A refusal against current state
     is `EXIT_CONFLICT`.
   - Every code is listed in `docs/agent-board.md` §4.
10. **Transaction bodies can be re-run.**
    - `transaction!` runs SERIALIZABLE and re-runs the body on 40001, 40P01 or 23505
      (`platform/store/src/lib.rs`, `collided`).
    - Fence memory and wakes run at the surface after the call returns Ok (`board_note` then
      `wake_recipient`, `tools/tickets.rs`). G9 covers everything else outside the store.
    - A closure passed into an operation is `Fn`, not `FnOnce` (`revoke_in`'s `which`,
      `hosted/standing.rs`).
    - A secret or id an operation stores is made before the transaction (G9): the standing id
      (`standing_id`), a webhook's secret (`set_wake`).
11. **Board tools keep what boards promise.** The form of each tool follows mcp-tools.md
    (`check_tools`). On top of it:
    - The name is `board_<verb>` or `approval_<verb>`.
    - `read_only_hint = true` exactly for the tools in `READ_ONLY_TOOLS`
      (`engine/hub/src/agenthub/mcp/mod.rs`). A viewer may call those and the
      `PER_CALLER_WRITES`, whose only write is the caller's own read state (`board_inbox`):
      `viewer_may_call`.
    - `idempotent_hint = true` only when the same arguments twice append no event (rule 6).
      `board_claim` is not: without an id it claims the next ready ticket each time.
    - The output is the store record (`Json<T>`), always an object: a list answers inside one
      (`TicketsOut`, `EventsOut`, `DepsOut`). The return type is spelled
      `Result<Json<T>, Json<BoardError>>`, because rmcp reads the output schema from the type as
      written and cannot see through an alias. Refusals go through `refuse`, which adds
      `ticket` and `next`.
    - A tool that hands out a lease calls `remember_fence`, and one that ends it calls
      `forget_fence`, both after the call returns. An omitted fence comes from `fence_for`, and
      its Result goes into the closure.
12. **Every surface call is audited once (G7).**
    - MCP gets this through `board_call` / `board_call_scoped`, refusals included: the seat, a
      missing fence, an unknown dependency kind and a refused webhook URL are refused inside the
      call. A tool that does not go through `board_call` (`board_wait`, `approval_wait`,
      `board_bind`, an unmapped issue repository) writes its one line with
      `record_completion`.
    - A page or REST route writes one line through `Hub::record_completion` after the store
      returns.
    - The tool name of a page write is the MCP tool for the same intent when there is one
      (`ticket_actions.rs` maps "assign", "close" and the rest to `board_update`). Otherwise it
      is a name of the same form (`approval_decide`, `approval_allow`, `approval_revoke`).
    - The line comes after the store committed, and a failed write never turns the answer into
      a refusal: the real result goes back, the failure is logged, and `audit.write_failed`
      rises (D1, chosen by the operator 2026-11-19).
13. **The CLI and the issue routes reach the same operation.**
    - An operation the CLI offers is a `Face` method, with a `Board` forwarder and a `Remote`
      forwarder that calls exactly the MCP tool.
    - An operation an MCP tool reaches has a `Board` method (`board/issues.rs` for the issue
      tools). Tools never call `board.hosted()`. The async REST routes may await the
      `HostedBoard` method directly.
    - The CLI command uses `fence_for`, the claim cache, and `with_id` for errors.
14. **The plugin is generated.** `plugins/devagotchi-agenthub/hooks/hooks.json` equals
    `cli::plugin_hooks()` (`agenthub/src/cli/hooks/install.rs`), and every tool a skill
    names is one the hub serves.

## Where files go

Store: `agenthub/src/hosted/` by subject, each file at most 500 lines (layout rule 6):

- `mod.rs`: `HostedBoard`, `new`, `for_owner`, `blocking`, `initialize`, `scope`,
  `configuration`, `fresh_id`, `ticket`, `get`, `event`, `events_page`, `decode`, `server_now`.
- `tickets.rs`: welcome, create, update, delegate, undo-close (`unheld_edit`).
- `leases.rs`: claim, claim-next, `check_wip`, heartbeat, complete, release, handoff,
  force-release, reap, `clear_lease`, `expire`.
- `notes.rs`: `add_note`, `insert_note`.
- `graph.rs`: deps.
- `approvals.rs`: ask, decide, approve-and-allow (`decide_in`), and the approval reads.
- `standing.rs`: standing authorizations granted (`grant_in`), revoked (`revoke_in`,
  `revoke_own`), live and suggested.
- `settings.rs`: settings and workflow, read, set field by field, replaced whole.
- `inbox.rs`: cursors, wakes.
- `issues.rs`: issue projection and context.
- `reads.rs`: the full ticket view, list, ready, verify-chain.
- `registry.rs`: what a host asks about all the boards in a tenant.

Other locations:

- Action names: `pub mod actions` in `agenthub/src/chain.rs`, beside `GENESIS` and
  `event_hash`.
- The sync face mirrors the store's subjects in `agenthub/src/board/`. Lease verbs stay
  in `lease.rs`, approvals in `approval.rs`, wakes in `wake.rs`. Issue reads and writes are in
  `board/issues.rs`.
- Tables: only new numbered files in `agenthub/migrations/`, shaped like
  `0002_board_sources.sql`: the `board_` prefix, the `(tenant_id, board_id, ...)` key, FORCE
  RLS with `tenant_only`, the `devagotchi_hub` grant, and the `board_append_only()` trigger
  when rows never change (persistence.md).
- MCP: `engine/hub/src/agenthub/mcp/tools/{read,tickets,messages,settings,issues}.rs` by what
  the tool does. A tool that writes never lives in `read.rs`.
- Tests: G10.

## How it is tested

The canonical store tests are `agenthub/src/tests/actions.rs` (the chain's names and
no-op silence) and `agenthub/src/tests/operations.rs` (each multi-row operation, with
its atomicity shown by a refused outbox row). Fixture and skip shape are G13
(`or_skip!(fixture())`, or `or_skip!(postgres())` for a test that opens its own board).

1. **Store test** in `agenthub/src/tests/<topic>.rs`, with
   `#[tokio::test(flavor = "multi_thread")]` (`hosted::blocking` needs a multi-thread runtime).
   It asserts:
   - the happy-path record, field by field;
   - each refusal it can return, by `.code`;
   - `verify_chain().ok`, and the exact event `action` and `data`;
   - for every no-op, `quiet(&mut board_changes(url))` and an unchanged chain (both in
     `tests/mod.rs`);
   - isolation from another board;
   - concurrency for anything keyed (`tests/sources.rs`
     `concurrent_importers_make_one_ticket_and_one_event`);
   - atomicity for a multi-row operation by failure injection: `Refuse` in
     `tests/operations.rs`, or `tests/hosted.rs`
     `outbox_failure_rolls_back_ticket_lease_event_and_fence`.
2. **Hub test** over HTTP-MCP when identity, seat or tenancy matters:
   `engine/hub/src/agenthub/tests/` with `serving::{hub_with, tool}`. Model: `tests/sourced.rs`
   `each_account_imports_its_own_and_a_repeat_from_another_session_is_the_same`, which checks
   `isError`, `structuredContent.code` and `next`, events unchanged by refusals, and
   cross-account `not_found`.
3. **Page test** for a page action: `engine/hub/src/agenthub/web/tests/ticket.rs`, covering
   the chain and the audit line.
4. **CLI test** for the exit class and `next`: `agenthub/src/cli/tests/output.rs`
   (`every_code_a_command_can_reach_names_its_next_step`,
   `a_refusal_against_the_boards_state_is_a_conflict`).
5. **Plugin test**: `engine/hub/src/mcp/tests/plugins.rs`
   (`the_agenthub_plugin_ships_exactly_the_hooks_the_handler_prints`,
   `every_tool_a_skill_names_is_one_the_hub_serves`).

## Anti-patterns

- Two transactions for one intent: `update` then `add_note`; a decision then a grant;
  `settings()` then `create`; `wakes()` then a save; a read of standing grants then a revoke
  of each.
- Writing `board_events` directly, or appending an event for a request that changed nothing.
- Reading a machine clock for anything the board keeps or decides on.
- Reusing a helper's reserved `data` keys.
- An `idempotent_hint` or `read_only_hint` the operation does not keep.
- A tool that writes, filed under `read.rs`. A tool that calls `board.hosted()`.
- A refusal made before the audit line (`.map_err(refuse)?` before `board_call`).
- A list as a tool's answer, or `BoardOut<T>` as a tool's return type (rmcp writes no output
  schema for it).
- A new error code with only the default `next` ("board_prime to re-orient" on MCP, "board
  --help" on the CLI).
- Logic in `Board`, `Face` or `Remote` beyond forwarding.
- SQL on `board_*` tables from the hub crate.
- An operation that takes the actor as `&str`.
- Stored data decoded with `.ok().unwrap_or_default()`.

## Mechanical check

Each landed with the fix it guards (G12), in B3.

1. **`engine/hub/src/agenthub/mcp/tests/contract.rs`** (rules 9 and 11):
   `a_viewer_may_call_exactly_the_board_tools_that_change_nothing_but_its_read_state` holds
   `READ_ONLY_TOOLS`, `PER_CALLER_WRITES` and `viewer_may_call` to the tools' hints;
   `every_code_the_store_refuses_with_names_the_next_call` scans the files where the store
   decides for `fail("` and `BridgeError::new("` and asks `refuse` for each. Titles, hints,
   closed and documented inputs and object outputs are `check_tools`
   (`interfaces/mcp/src/pattern.rs`: N1 to D3, with O4 for object answers and O5 for no boolean schemas), run by `engine/hub/src/mcp/tests/tools.rs`
   against `tools-not-yet.txt`, which holds no board tool.
2. **`agenthub/src/cli/tests/output.rs`** (rule 9, CLI side): the same scan against the
   CLI `next_for`, and the conflict class.
3. **`agenthub/src/tests/actions.rs`** (rules 6 and 7): the form of every name in
   `chain::actions::ALL`; no `"word.word"` literal passed to `event` or `clear_lease` in
   `hosted/` or `approval.rs` (with a check that the scan sees what it looks for); every no-op
   above leaves the chain and the listener quiet; the handoff event's `assignee`, `from` and
   `to`; `board.settings_replaced` naming its changed keys.
4. **bebop's `layout.module-home`** (layout rule 2), against
   `.ai/rules/layout/module-home/baseline.txt`, which only shrinks and lists nothing in AgentHub.
5. **`engine/hub/src/mcp/tests/scan.rs`**: every tool body reaches its audit owner, against
   `audit-not-yet.txt`, which holds no board tool.

Held by review until a test lands:

- Rule 1: the SQL scan (`runtime/devagotchi/tests/sql.rs`, planned in B6, persistence.md)
  refuses `board_` tables outside `agenthub`, except in hub tests.
- Rules 2, 10 and 12: review. For any new multi-row operation, ask for the atomicity test shape
  (`Refuse` in `tests/operations.rs`).

## Upstream sources

- MCP tools, tool execution errors, `structuredContent` and `outputSchema` (objects): https://modelcontextprotocol.io/specification/2025-11-25/server/tools
- MCP `ToolAnnotations` (destructiveHint and idempotentHint are meaningful only when
  readOnlyHint is false; the definition of idempotent): https://modelcontextprotocol.io/specification/2025-06-18/schema#toolannotations
- rmcp `tool_router` (`router = name`, routers combined with `+`): https://docs.rs/rmcp-macros/latest/rmcp_macros/attr.tool_router.html
- rmcp schemas from `Parameters<T>` / `Json<T>` (read from the return type as written): https://github.com/modelcontextprotocol/rust-sdk
- schemars doc comments become the schema description: https://graham.cool/schemars/examples/6-doc_comments/
- Fencing tokens: https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html
- Transactional outbox: https://microservices.io/patterns/data/transactional-outbox.html
- NOTIFY is delivered on commit, and identical payloads are folded: https://www.postgresql.org/docs/current/sql-notify.html
- Retry SERIALIZABLE transactions on serialization failure: https://www.postgresql.org/docs/current/transaction-iso.html
- sqlx `Transaction` rolls back on drop: https://docs.rs/sqlx/latest/sqlx/struct.Transaction.html
- tokio `block_in_place` panics on a current-thread runtime (`hosted::blocking` guards it): https://docs.rs/tokio/latest/tokio/task/fn.block_in_place.html
- CLI exit codes, JSON when not a TTY, next-step hints: https://clig.dev/
- Claude Code hooks (exit 2 per event, `stop_hook_active`, empty matcher, SessionStart
  `additionalContext`): https://code.claude.com/docs/en/hooks

## Known violations

None open. B3 closed every entry this file listed at main 2b3feec (the B3 plan,
[board-operations-b3.md](../dev/plans/board-operations-b3.md), lists each with where it went).

Kept, and why:

- `agenthub/src/tests/lanes.rs` `open_seeds_metadata_settings_and_workflow_once` opens
  its board itself (`or_skip!(postgres())`): opening twice is what it tests.
- `engine/hub/src/notify/signals/mod.rs` reads events through `HostedBoard::events_page`, which
  takes the board row lock like every board read (`scope`).
