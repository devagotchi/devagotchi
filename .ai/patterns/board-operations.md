# Pattern: a board operation

This is how an AgentHub operation is defined, stored, put on the hash chain, served and
tested. Follow it for every new operation (claim, claim_next, heartbeat, note, handoff, release,
complete, force_release, create, update, deps, approvals, standing grants, settings, wakes,
issue context) and for every change to one.

The shared rules G1 to G14 in [the patterns README](README.md#shared-rules-stated-once-apply-everywhere)
hold here and are not repeated. This file states only what is particular to boards.

Related documents:

- Contract: [agent board](../../docs/agent-board.md) §4 (item 8: "Every mutation appends its
  event in the same transaction"), §6 (surfaces) and §10 (ticket origins).
- Where files go in general: [layout.md](layout.md).
- The form of every MCP tool (names, titles, hints, closed and documented inputs):
  [mcp-tools.md](mcp-tools.md).
- Migrations, RLS and store fixtures: [persistence.md](persistence.md) and
  [tests-and-ci.md](tests-and-ci.md).

mcp-tools.md, persistence.md and tests-and-ci.md land with this file in batch B0.

## Purpose

One operation is one Postgres transaction, one event on the hash chain for each action it
performs, and one stable refusal code. A person or agent reaches it through any surface (hub
MCP, the `devagotchi board` CLI on its local or remote face, a page, a REST route, a hook) and
gets the same answer. `HostedBoard` decides (G1). Every other layer forwards.

## The layers

| Layer | Home | Shape |
|---|---|---|
| Store (decides) | `crates/agenthub/src/hosted/<subject>.rs` | `impl HostedBoard { pub async fn op(&self, actor: &Actor<'_>, ...) -> BridgeResult<T> }` |
| Sync face | `crates/agenthub/src/board/<subject>.rs` (`lease.rs` for lease verbs) | one line: `self.hosted_call(self.hosted.op(...))` |
| CLI face | `crates/agenthub/src/face.rs` and `cli/remote.rs` | a `Face` method; `Board` forwards; `Remote` makes one `board_<tool>` call |
| CLI command | `crates/agenthub/src/cli/commands/<group>.rs`, dispatched in `commands/mod.rs` | parse, `fence_for`, `with_id(id, board.op(...))`, claim cache, `output.value` |
| MCP tool | `crates/hub/src/agenthub/mcp/tools/<group>.rs`; input in `mcp/schema.rs` | `#[tool]`, then `board_call(tool, project, ticket, ctx, move \|board, actor\| board.op(...))` |
| Page or REST | `crates/hub/src/web/app/agenthub/`, `web/approvals.rs`, `crates/hub/src/agenthub/issue_lifecycle/` | a page: `spawn_blocking` and one `board.op`; an async REST route: one awaited `HostedBoard` method; either way one audit line |
| Hook | `crates/agenthub/src/cli/hooks/`, shipped as `plugins/devagotchi-agenthub/hooks/hooks.json` | `plugin_hooks()` prints it; the plugin ships exactly that |

## Exemplar

`handoff`, end to end. Line ranges are at main 2b3feec with the `crates/agenthub` submodule
at 5c1499c. It is the one operation that touches every concern: a pure check, the fence
guard, a multi-row write, one event, side effects only after commit, refusal codes, the CLI
claim cache and the hub's fence memory. `docs/agent-board.md` §4 item 5 makes "handoff note +
release + assignee, one transaction" its contract. It has one known defect (its event data;
see Known violations, rule 7).

1. Store: `crates/agenthub/src/hosted/operations.rs:583-635`, `HostedBoard::handoff`.
   - `scope` (`hosted.rs:211-225`) takes the tenant scope, locks the board row `FOR UPDATE`
     and returns the database clock.
   - The fence guard is `lease::check_held` (`lease.rs:34-60`).
   - `clear_lease` (`hosted.rs:419-446`) writes the one event through `event`
     (`hosted.rs:329-384`): `board_events`, `board_outbox` and `pg_notify` in the same
     transaction.
   - Runner-up with no defect: `HostedBoard::complete` (`hosted.rs`, from line 711).
2. Sync face: `crates/agenthub/src/lease.rs:113-123`, `Board::handoff`.
3. CLI faces: `face.rs:56-63` (trait), `face.rs:185-194` (`Board`), `cli/remote.rs:201-214`
   (`Remote`, one `board_handoff` call).
4. CLI command: `cli/commands/leases.rs:196-221`, `handoff`.
5. MCP tool: `crates/hub/src/agenthub/mcp/tools/tickets.rs:285-322`, `board_handoff`. Its input
   is `HandoffIn` (`mcp/schema.rs:239-250`). The call goes through `board_call` /
   `board_call_scoped` (`mcp/calls.rs:103-188`), whose actor comes from `actor_for`
   (`calls.rs:72-90`). Refusals become `BoardError` through `refuse` (`mcp/refusals.rs:69-88`).
6. Idempotency models, for an operation a caller may retry:
   - A natural key: `create_in` with a source (`hosted/tickets.rs:19-147`; `sourced`, 151-180).
   - A caller-supplied request id with a stored receipt: `write_issue_context`
     (`hosted/issues.rs:279-375`).
   - In both, the repeat returns what exists and writes nothing, and changed content is a
     conflict code (`source_conflict`, `request_conflict`) refused before any write.

## Canonical shape

```rust
// crates/agenthub/src/hosted/leases.rs (today: hosted/operations.rs)
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

// crates/agenthub/src/lease.rs
impl Board {
    /// A handoff note, a release, and an assignee, in one transaction.
    pub fn handoff(&self, actor: &Actor<'_>, id: &str, fence: i64, note: &str, to: Option<&str>)
        -> BridgeResult<Ticket> { self.hosted_call(self.hosted.handoff(actor, id, fence, note, to)) }
}

// crates/hub/src/agenthub/mcp/tools/tickets.rs
#[tool(name = "board_handoff", title = "Hand off a ticket", description = "...",
       annotations(title = "Hand off a ticket", read_only_hint = false, destructive_hint = false,
                   idempotent_hint = false, open_world_hint = false))]
async fn board_handoff(&self, Parameters(input): Parameters<HandoffIn>,
                       context: RequestContext<RoleServer>) -> BoardOut<Ticket> {
    let key = self.subject_id(&context)?;            // the fence memory's key
    // The Result goes into the closure, so a missing fence is refused there and audited (G7).
    let fence = self.boards.fence_for(&key, &input.project, &input.id, input.fence);
    let id = input.id.clone();
    let result = self.board_call("board_handoff", &input.project, Some(&input.id), &context,
        move |board, actor| board.handoff(actor, &id, fence?, &input.note, input.to.as_deref())).await?;
    self.boards.forget_fence(&key, &input.project, &input.id); // after the call returns, never inside
    Ok(result)
}
```

`actions::TICKET_HANDED_OFF` is the rule 7 constant; today the code passes the literal
`"ticket.handoff"`. `subject_id` still refuses a bad seat before the audit line; that is listed
under G7 in mcp-tools.md.

## Rules

1. **One decider (G1).** Only `HostedBoard` (`crates/agenthub/src/hosted/`) runs SQL on
   `board_*` tables. No `board_*` SQL lives in `crates/hub` outside its tests.
   - `Board`, `Face`, `Remote`, MCP tools, CLI commands, pages and REST routes each make exactly
     one call per user intent, and add no logic beyond forwarding.
   - When a surface needs two writes (a decision plus a standing grant, an assignment plus a
     note, a default plus a create, a read-modify-write of settings or wakes), that is a new
     `HostedBoard` operation.
2. **One operation, one `transaction!`.**
   - Every read the decision depends on, and every write, happens inside it.
   - Checks that need no database (lengths, enum parsing, `actor.human`) run before it.
3. **Scope first, then the database clock only (G9).**
   - The block's first statement is `let now = self.scope(connection).await?;`.
   - Every time the board stores or decides on (expiry, liveness, a standing approval's
     window, the undo window) comes from that `now`. `Board::now()`, `system_clock` and a
     page's clock never decide.
   - A closure that needs the time takes it as an argument. `edit_settings` does not pass `now`
     yet; see Known violations, rule 3.
   - Display-only arithmetic ("12m left") may use the face's clock.
4. **Tenant, board and actor come from authentication (G8).**
   - Every statement binds `tenant_id=$1 AND board_id=$2` from `self.tenant` and
     `self.project`.
   - Every operation takes `&Actor<'_>`, never an actor string. The actor is the one the
     surface resolved: `actor_for` on MCP, `Actor::human(&subject.id)` on pages.
5. **Guards are the shared ones.**
   - A held ticket changes only after `lease::check_held(&ticket, actor.id, Some(fence))`.
   - An unheld edit compares `version` (`version_conflict`).
   - A person-only act checks `actor.human` (`humans_only`).
   - An approval changes only through `decide_approval` (`approval_decide_only`).
6. **One event per action, none for a no-op.**
   - Each action in `chain::actions` is one event. An operation appends one event for each
     action it performs, inside its transaction, through `self.event(...)` directly or through
     `clear_lease` / `expire`. That writes `board_events`, `board_outbox` and `pg_notify`
     together.
   - Handoff is one action (`ticket.handed_off`, covering its note, assignee and release).
     An ask that a standing grant covers is two (`approval.requested`, `approval.decided`).
     Decide-and-allow is two (`approval.decided`, `approval.standing_granted`).
   - An unchanged request returns the current state, writes nothing and announces nothing.
     That covers a repeat, an empty patch, an existing dependency, settings equal to the
     current ones, and re-claiming what you hold.
   - Two exceptions, both documented here: board initialization (the chain starts with the
     board's first operation, `board.welcomed` for a new account board) and per-reader inbox
     cursors, which are not board state.
7. **Action names are `<subject>.<past participle>`**, as `pub const` in `chain::actions`,
   listed in `chain::actions::ALL`. Subjects: `ticket`, `lease`, `note`, `dep`, `approval`,
   `board`, `issue`, `wake`.
   - `data` names what changed: `from` / `to` for states, the changed fields, the recipient
     under its own key. A helper's keys are reserved (`clear_lease` owns `from` and `to`).
   - `data` never carries a secret (G8). A wake's secret stays out (`wake.rs` `set_wake`
     passes only recipient and url).
   - The action is also the outbox `kind` that signal sinks and ClickHouse receive. In alpha
     a rename is allowed and goes in the release notes (C17, D7). Events already on a chain
     keep their old names, because the hash covers the action; the chain stays additive.
8. **Return the record as it is now**, re-read inside the transaction (`self.ticket(...)`,
   `self.deps(...)`, `self.approval_in(...)`).
9. **Refusals are stable codes (G4).**
   - The store refuses with `fail("snake_code", message)`, or `invalid(message)` for bad input.
   - Every code the store can return gets a `next` of its own from the MCP `refuse`
     (`crates/hub/src/agenthub/mcp/refusals.rs`): a line in `next_for`, or one of the cases
     `refuse` builds from the message (`invalid_settings`, `unknown_environment`).
   - Every code a CLI command can reach has a line in the CLI `next_for` and a deliberate class
     in `exit_code_for` (`crates/agenthub/src/cli/errors.rs`). A refusal against current state
     is `EXIT_CONFLICT`.
   - Every code is listed in `docs/agent-board.md` §4.
10. **Transaction bodies can be re-run.**
    - `transaction!` runs SERIALIZABLE and re-runs the body on 40001, 40P01 or 23505
      (`crates/store/src/lib.rs`, `collided`).
    - Fence memory and wakes run at the surface after the call returns Ok (`board_note` then
      `wake_recipient`, `tools/tickets.rs`). G9 covers everything else outside the store.
    - A closure passed into an operation is `Fn`, not `FnOnce` (`edit_settings`,
      `hosted/approvals.rs:95-114`).
    - A secret or id an operation stores is made before the call and passed in (G9).
11. **Board tools keep what boards promise.** The form of each tool follows mcp-tools.md.
    On top of it:
    - The name is `board_<verb>` or `approval_<verb>`.
    - `read_only_hint = true` exactly for the tools in `READ_ONLY_TOOLS`
      (`crates/hub/src/agenthub/mcp/mod.rs`), except the tools in `PER_CALLER_WRITES`, whose
      only write is the caller's own read state (`board_inbox`).
    - `idempotent_hint = true` only when the same arguments twice append no event (rule 6).
    - The output is the store record (`Json<T>`). Refusals go through `refuse`, which adds
      `ticket` and `next`.
    - A tool that hands out a lease calls `remember_fence`, and one that ends it calls
      `forget_fence`, both after the call returns. An omitted fence comes from `fence_for`.
12. **Every surface write is audited once (G7).**
    - MCP gets this through `board_call` / `board_call_scoped`.
    - A page or REST route writes one line through `Hub::recorded` once B1 adds it. Until then
      it calls `audit.record(&AuditEntry{subject, via, tool, broker: project, operation_id:
      ticket, outcome})` as `web/app/agenthub/ticket_actions.rs` `act` does.
    - The tool name of a page write is the MCP tool for the same intent when there is one
      (`ticket_actions.rs` maps "assign", "close" and the rest to `board_update`). Otherwise it
      is a name of the same form (`approval_decide`, `approval_allow`, `approval_revoke`).
    - After the store committed, a failed audit write never turns the answer into a refusal.
      This is G7 as written (C9). It needs the operator's agreement (D1); until then G7 is the
      default.
13. **The CLI and the issue routes reach the same operation.**
    - An operation the CLI offers is a `Face` method, with a `Board` forwarder and a `Remote`
      forwarder that calls exactly the MCP tool.
    - An operation an MCP tool reaches has a `Board` method. Tools never call `board.hosted()`.
      The async REST routes may await the `HostedBoard` method directly.
    - The CLI command uses `fence_for`, the claim cache, and `with_id` for errors.
14. **The plugin is generated.** `plugins/devagotchi-agenthub/hooks/hooks.json` equals
    `cli::plugin_hooks()` (`crates/agenthub/src/cli/hooks/install.rs`), and every tool a skill
    names is one the hub serves.

## Where files go

Store: `crates/agenthub/src/hosted/` by subject, each file at most 500 lines (layout rule 6;
files on the ratchet only shrink):

- `mod.rs`: `HostedBoard`, `new`, `for_owner`, `blocking`, `scope`, `configuration`,
  `fresh_id`, `ticket`, `event`, `decode`, `server_now`. There is no `hosted.rs` beside the
  folder (layout rule 2).
- `tickets.rs`: create, update, view, welcome.
- `leases.rs`: claim, claim_next, heartbeat, complete, release, handoff, force_release, reap.
- `notes.rs`: add_note, insert_note.
- `graph.rs`: deps.
- `approvals.rs`: requests, decisions, standing grants.
- `settings.rs`: settings, workflow, put, set and edit settings.
- `inbox.rs`: cursors, wakes.
- `issues.rs`: issue projection and context.
- `reads.rs`: list, ready, verify_chain.
- `registry.rs`: the queries the hub's board registry needs (names, lease delays, existence).

Other locations:

- Action names: `pub mod actions` in the existing `crates/agenthub/src/chain.rs`, beside
  `GENESIS` and `event_hash`.
- The sync face mirrors the store's subjects in `crates/agenthub/src/board/`. Lease verbs stay
  in `lease.rs`. Issue reads and writes go in `board/issues.rs`.
- Tables: only new numbered files in `crates/agenthub/migrations/`, shaped like
  `0002_board_sources.sql`: the `board_` prefix, the `(tenant_id, board_id, ...)` key, FORCE
  RLS with `tenant_only`, the `devagotchi_hub` grant, and the `board_append_only()` trigger
  when rows never change (persistence.md).
- MCP: `crates/hub/src/agenthub/mcp/tools/{read,tickets,messages,settings,issues}.rs` by what
  the tool does. A tool that writes never lives in `read.rs`.
- Tests: G10.

## How it is tested

The canonical test is `crates/agenthub/src/tests/sources.rs`,
`a_repeat_returns_the_ticket_as_it_is_and_changed_inputs_conflict`: a repeat returns current
state, each changed input is refused, the chain is unchanged, and a `quiet` listener on
`board_changes` proves nothing was announced. Fixture and skip shape are G13
(`or_skip!(fixture())`).

1. **Store test** in `crates/agenthub/src/tests/<topic>.rs`, with
   `#[tokio::test(flavor = "multi_thread")]` (`hosted::blocking` needs a multi-thread runtime).
   It asserts:
   - the happy-path record, field by field;
   - each refusal it can return, by `.code`;
   - `verify_chain().ok`, and the exact event `action` and `data` (the handoff test,
     `tests/lanes.rs` `handoff_notes_releases_and_assigns_in_one_step`, does not, which is how
     its data defect went unseen);
   - for every no-op, `quiet(&mut board_changes(url))` and an unchanged chain (`board_changes`
     is in `tests/mod.rs`; `quiet` is private to `tests/sources.rs` today and moves beside
     `board_changes` when a second file needs it);
   - isolation from another board;
   - concurrency for anything keyed (`tests/sources.rs`
     `concurrent_importers_make_one_ticket_and_one_event`);
   - atomicity for a multi-row operation by failure injection (`tests/hosted.rs`
     `outbox_failure_rolls_back_ticket_lease_event_and_fence`).
2. **Hub test** over HTTP-MCP when identity, seat or tenancy matters:
   `crates/hub/src/agenthub/tests/` with `serving::{hub_with, tool}`. Model: `tests/sourced.rs`
   `each_account_imports_its_own_and_a_repeat_from_another_session_is_the_same`, which checks
   `isError`, `structuredContent.code` and `next`, events unchanged by refusals, and
   cross-account `not_found`.
3. **Page test** for a page action: `crates/hub/src/web/app/agenthub/ticket_tests.rs`, covering
   the chain and the audit line.
4. **CLI test** for the exit class and `next`: `crates/agenthub/src/cli/tests/mod.rs`
   `exit_codes_and_next_hints_by_failure_class`.
5. **Plugin test**: `crates/hub/src/mcp/tests/plugins.rs`
   (`the_agenthub_plugin_ships_exactly_the_hooks_the_handler_prints`,
   `every_tool_a_skill_names_is_one_the_hub_serves`).

## Anti-patterns

- Two transactions for one intent: `update` then `add_note`; `decide_approval` then
  `grant_standing`; `settings()` then `create`; `wakes()` then `save_wakes`; `open_approval`
  then `settings()` then `decide_approval`.
- Writing `board_events` directly, or appending an event for a request that changed nothing.
- Reading a machine clock for anything the board keeps or decides on.
- Reusing a helper's reserved `data` keys.
- An `idempotent_hint` or `read_only_hint` the operation does not keep.
- A tool that writes, filed under `read.rs`. A tool that calls `board.hosted()`.
- A new error code with only the default `next` ("board_prime to re-orient" on MCP, "board
  --help" on the CLI).
- Logic in `Board`, `Face` or `Remote` beyond forwarding.
- SQL on `board_*` tables from the hub crate.
- An operation that takes the actor as `&str`.

## Mechanical check

None of these checks exists yet at main 2b3feec. Two of the files exist (`cli/tests/output.rs`
and `crates/devagotchi/tests/layout.rs`); the tests are added to them. Each check lands green
with the fix it guards (G12), in batch B3.

1. **`crates/hub/src/agenthub/mcp/tests/contract.rs`** (rules 9 and 11), declared with
   `mod contract;` in `mcp/tests/mod.rs`. Titles, hints and input docs are checked for every
   tool by `check_tools` (`crates/mcp/src/pattern.rs`, planned in B1, see mcp-tools.md). This
   file keeps only what boards add (C14):

   ```rust
   //! The board tools held to what boards promise: a viewer may call exactly the tools that
   //! change nothing but its own read state, and every code the store refuses with names the
   //! call to make next.
   use std::{collections::BTreeSet, path::Path};
   use devagotchi_contracts::BridgeError;
   use crate::agenthub::mcp::{PER_CALLER_WRITES, READ_ONLY_TOOLS, refuse};

   /// Store codes no agent can act on: runtime plumbing, or reached only by a page or the
   /// webhook route.
   const INTERNAL: &[&str] = &["runtime", "board_random", "board_id", "board_runtime",
       "delivery_conflict", "unsupported_action", "wake_failed", "no_done_state", "too_many"];

   #[test]
   fn a_viewer_may_call_exactly_the_board_tools_that_change_nothing_but_its_read_state() {
       for tool in crate::mcp::tools() {
           let name = tool.name.to_string();
           if !(name.starts_with("board_") || name.starts_with("approval_")) { continue; }
           let read_only = tool.annotations.as_ref().and_then(|a| a.read_only_hint) == Some(true);
           assert_eq!(READ_ONLY_TOOLS.contains(&name.as_str()),
                      read_only || PER_CALLER_WRITES.contains(&name.as_str()), "{name}");
       }
   }

   #[test]
   fn every_code_the_store_refuses_with_names_the_next_call() {
       let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../agenthub/src");
       let mut files: Vec<_> = ["hosted.rs", "approval.rs", "lease.rs", "wake.rs",
           "issue_lifecycle.rs", "settings.rs", "workflow.rs", "checks.rs", "lib.rs"]
           .iter().map(|f| src.join(f)).filter(|p| p.exists()).collect();
       for dir in ["hosted", "board"] {
           for entry in std::fs::read_dir(src.join(dir)).unwrap() { files.push(entry.unwrap().path()); }
       }
       // `read_only` is the hub's own refusal (`mcp/calls.rs`), so the scan cannot find it.
       let mut codes = BTreeSet::from(["invalid".to_string(), "read_only".to_string()]);
       for path in files {
           let flat: String = std::fs::read_to_string(&path).unwrap().split_whitespace().collect();
           for marker in ["fail(\"", "BridgeError::new(\""] {
               for piece in flat.split(marker).skip(1) {
                   codes.insert(piece.split('"').next().unwrap().to_string());
               }
           }
       }
       let fallback = refuse(BridgeError::new("zz_never_a_code", ""), None).0.next;
       for code in codes.iter().filter(|c| !INTERNAL.contains(&c.as_str())) {
           assert_ne!(refuse(BridgeError::new(code, "m"), Some("x")).0.next, fallback,
                      "{code} has no next of its own in mcp/refusals.rs");
       }
   }
   ```

   The scan covers only the files where the store decides. All of `agenthub/src` would also
   collect CLI-only codes (`no_storage`, `usage`, `invalid_storage`), which MCP never returns.
   The first test does not compile until `PER_CALLER_WRITES` exists, and fails if
   `board_inbox` is not in it. The second fails on the store codes listed under rule 9 below
   and on `read_only`.
2. **`crates/agenthub/src/cli/tests/output.rs`** (rule 9, CLI side; the file exists). Add a test
   that `errors::next_for(code, Some("x")) != "board --help"` for every code a CLI command can
   reach (the codes above minus `INTERNAL` and minus `not_an_approval`, `approval_changed` and
   `already_decided`, plus `no_storage`, `invalid_storage`, `not_signed_in` and
   `hub_unreachable`), and that `exit_code_for("parent_closed") == EXIT_CONFLICT`.
3. **`crates/agenthub/src/tests/actions.rs`** (rules 6 and 7):
   - every const in `chain::actions::ALL` matches
     `^(ticket|lease|note|dep|approval|board|issue|wake)\.[a-z_]+$` and ends in `ed`, `en` or
     `set`; no `"<word>.<word>"` literal is passed to `event`, `clear_lease` or `edit_settings`
     in `hosted/` or `approval.rs`;
   - on `fixture()`: `add_dep` twice, `update` with an empty `Patch`, `set_project` with the
     current repo, `claim` of a held ticket, `create` with the same origin and
     `write_issue_context` with the same request_id each leave `verify_chain().entries`
     unchanged and `quiet(board_changes)` true (with `quiet` moved into `tests/mod.rs`);
   - the handoff event's `data.assignee` is the recipient and `data.to` is the release state.
4. **`crates/devagotchi/tests/layout.rs`** (layout rule 2; the file exists and holds the size
   ratchet): add a walk that fails when `<x>.rs` and `<x>/` sit side by side and `<x>` is not
   `main`, `lib` or `build`. Agree it with layout.md's owner.

Held by review until a test lands:

- Rule 1: the SQL scan (`crates/devagotchi/tests/sql.rs`, planned in B6, persistence.md)
  refuses `board_` tables outside `crates/agenthub`, except in hub tests.
- Rules 2, 10 and 12: review. For any new multi-row operation, ask for the atomicity test shape
  (`outbox_failure_rolls_back_ticket_lease_event_and_fence`).

## Upstream sources

- MCP tools, tool execution errors: https://modelcontextprotocol.io/specification/2025-11-25/server/tools
- MCP `ToolAnnotations` (destructiveHint and idempotentHint are meaningful only when
  readOnlyHint is false; the definition of idempotent): https://modelcontextprotocol.io/specification/2025-06-18/schema#toolannotations
- rmcp `tool_router` (`router = name`, routers combined with `+`): https://docs.rs/rmcp-macros/latest/rmcp_macros/attr.tool_router.html
- rmcp schemas from `Parameters<T>` / `Json<T>`: https://github.com/modelcontextprotocol/rust-sdk
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

Verified at main 2b3feec, submodule `crates/agenthub` at 5c1499c. Paths under `crates/agenthub`
are in the submodule. Each entry is `file (symbol): problem -> rewrite`.

### Rule 1: one decider

- `crates/agenthub/src/approval.rs:229-285` (`Board::ask`): three transactions
  (`open_approval`, `settings()`, `decide_approval`), and standing liveness on `self.now()`. A
  standing grant revoked between the read and the decide is still honoured, and the decision
  is not atomic with the request. -> `HostedBoard::ask(actor, ask, labels)` in
  `hosted/approvals.rs`: one transaction inserts the approval and `approval.requested`, reads
  `settings.standing`, tests `live(now)` with the scope's `now`, and decides through a shared
  `decide_in(connection, ...)` lifted from `decide_approval` (`hosted/approvals.rs:141-197`).
  `Board::ask` becomes one `hosted_call`. (Rules 1, 2, 3.)
- `crates/hub/src/web/approvals.rs` (`decide`, lines 447-462): `decide_approval` then
  `grant_standing`, two transactions. A failed grant (`too_many`, a bad kind) shows "Not
  decided" for a decision already committed. -> One store operation that decides and grants
  in one transaction, appending `approval.decided` then `approval.standing_granted` (rule 6).
  The page calls it once and writes one audit line (rule 12).
- `crates/hub/src/web/app/agenthub/ticket_actions.rs` (`apply`, the `"delegate"` arm):
  `board.update(assignee)` then `board.add_note(...)`, two transactions. A failed note leaves
  an assignment with no note saying why. -> `HostedBoard::delegate(actor, id, version, to,
  note)`: version check, assignee, `insert_note` (kind "note", to), and one `ticket.updated`
  event with `{assignee, note_seq}`, in one transaction. `apply` calls it once.
- `crates/hub/src/agenthub/mcp/tools/tickets.rs` (`board_create`, lines 77-88): the closure
  calls `board.settings()` to default the repo, then `board.create`. The default lives in the
  surface, so CLI `board new` and pages do not get it. -> In `create_in`
  (`hosted/tickets.rs`), use `settings.repo` when `new.anchors.repo` is blank, after `settings`
  is loaded. Anchors are not part of `request_sha256` (`provenance.rs`), so sourced repeats are
  unaffected. The closure becomes `move |board, actor| board.create(actor, new)`.
- `crates/agenthub/src/wake.rs` (`Board::set_wake`, lines 256-287): reads the wakes (one
  transaction), edits them in memory, saves the whole map (`save_wakes`, another). Two
  concurrent calls lose one webhook, and the `MAX_WAKES` check races. -> `HostedBoard::set_wake(
  actor, recipient, url, secret)` in `hosted/inbox.rs`: read through `decode`, apply the change
  and the `MAX_WAKES` check, upsert, and append the event, in one transaction. The secret is
  made before the call and passed in. `Board::set_wake` becomes one `hosted_call`.
- `crates/agenthub/src/hosted.rs` (`claim_in`) and `crates/agenthub/src/hosted/operations.rs`
  (`claim_next`): the WIP-limit check is copied, and `claim_next` runs it again through
  `claim_in`, so the copies can drift. -> `async fn check_wip(&self, connection, actor:
  &Actor<'_>, settings: &Settings, now: i64) -> DbResult<()>`, called from both.
- `crates/hub/src/agenthub/registry.rs` (`Registry::names`, `leases`, `open`): the hub runs SQL
  on `board_projects` and `board_tickets`. A schema change in the submodule then breaks the hub
  silently. -> Move the queries into `crates/agenthub` as `hosted::registry::{names,
  lease_delays, project_count}` (the doc at `crates/hub/src/agenthub/mcp/mod.rs:36` already
  names `hosted::registry`). The hub calls them.
- `crates/hub/src/notify/signals/mod.rs` (`Sink::board`): the board signal sink reads
  `board_projects` and `board_events` with its own SQL, so it has the same silent coupling. ->
  List a tenant's boards through `hosted::registry` and read events through
  `HostedBoard::events_page(after, limit)`. The sink calls them. (Not yet in the B3 plan; add
  it there.)

### Rule 3: the database clock

- `crates/agenthub/src/approval.rs` (`grant_standing`, `revoke_standing`, `standing`,
  `suggestions`: `self.now()` at lines 343, 384, 410, 419), `crates/agenthub/src/board/mod.rs:86-90`
  (`Board::now`) and `crates/hub/src/web/app/me.rs:390` (`stop_own`): standing approvals take
  grant, expiry and revocation times and `live(now)` from the machine clock, while leases use
  Postgres time. -> `HostedBoard::edit_settings` takes `F: Fn(&mut Settings, i64) ->
  BridgeResult<Value>` and passes the scope's `now`. Reads (`standing`, `suggestions`) return
  `database_now` with the list. Remove `Board::now` once nothing calls it.
- `crates/hub/src/web/app/agenthub/ticket_actions.rs` (`apply`, the `"undo"` arm, lines
  206-213): compares the hub's `now_ms()` with the ticket's `closed_at` (database clock) to
  decide whether the undo window is open. -> `undo_close(actor, id, version, from)` in the
  store, comparing `closed_at + UNDO_MS` with the scope's `now`. The page's countdown may keep
  the local clock.

### Rule 6: one event per action, none for a no-op

- `crates/agenthub/src/hosted/operations.rs` (`add_dep`): the insert is `ON CONFLICT DO
  NOTHING` but `dep.added` is always appended, so a repeat grows the chain and wakes every
  waiter, although `board_dep` declares `idempotent_hint = true`. -> When `.rows_affected()` is
  0, return `self.deps(connection, id, false)` without an event. Store test: the same dep twice
  gives one `dep.added` and a quiet listener.
- `crates/agenthub/src/hosted/operations.rs` (`set_project`): a given field is recorded in
  `changed` even when it equals the stored value, so a repeated `board_settings {repo}` saves
  and appends another `board.settings_changed`, although `board_settings` declares
  `idempotent_hint = true`. -> Insert into `changed` only when the value differs; save and
  append only when `changed` is non-empty. Add the no-op case to
  `crates/hub/src/agenthub/tests/settings.rs`.
- `crates/agenthub/src/hosted/operations.rs` (`put_settings`): always appends
  `admin.settings.put` with data `{}`, so the chain does not say what changed and identical
  settings still append. Used only by tests today (`board/settings.rs`, `#[allow(dead_code)]`).
  -> Read the current settings in the transaction; return Ok with no event when equal;
  otherwise append `board.settings_replaced` with `{changed: [keys]}`. (Rules 6, 7.)

### Rule 7: action names and data

- `crates/agenthub/src/hosted/operations.rs` (`handoff`) with `crates/agenthub/src/hosted.rs`
  (`clear_lease`, lines 434-435): handoff passes `{"to": to}`, and `clear_lease` overwrites
  `data["to"]` with the target state, so the chain never records the recipient. -> Pass
  `{"assignee": to}`. In `tests/lanes.rs` `handoff_notes_releases_and_assigns_in_one_step`,
  assert the last event: the handoff action, `data.assignee == "codex"`, `data.from` the
  active state, `data.to == "ready"`.
- `crates/agenthub/src/hosted.rs` (`heartbeat`), `hosted/operations.rs` (`handoff`,
  `put_settings`, `put_workflow`) and `hosted/inbox.rs` (`save_wakes`): five names break the
  form the other 18 follow: `ticket.heartbeat`, `ticket.handoff`, `admin.settings.put`,
  `admin.workflow.put`, `notes.wake_set`. -> Rename them `lease.extended`,
  `ticket.handed_off`, `board.settings_replaced`, `board.workflow_replaced` and `wake.set`. Put
  all 23 names as `pub const` in `pub mod actions` in `crates/agenthub/src/chain.rs` and pass
  the consts. Note the renames in the release notes (D7). Old events keep their names.

### Rule 9: refusals

- `crates/agenthub/src/hosted/inbox.rs` (`cursors`, `wakes`): decode with
  `.and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()`. A corrupt webhook row
  becomes "no webhooks"; a corrupt cursor row resets every reader's cursor and is then
  overwritten. -> `Ok(raw.map(|raw| decode(&raw)).transpose()?.unwrap_or_default())` in both
  (G4).
- `crates/agenthub/src/hosted/operations.rs` (`handoff`): builds its input error with
  `fail("invalid", ...)` instead of `invalid()`. -> `return Err(invalid("a handoff needs a note
  for whoever picks it up"));`
- `crates/hub/src/agenthub/mcp/refusals.rs` (`next_for`, lines 18-45): these codes reach MCP
  callers but fall through to "board_prime to re-orient": `invalid`, `parent_closed`,
  `not_held`, `dependency_cycle`, `approval_decide_only`, `not_an_approval`,
  `approval_changed`, `already_decided`, `read_only`, `request_conflict`, `board_damaged`.
  -> Add a line each, for example: `invalid` "fix the argument the message names and call
  again"; `parent_closed` "board_create without parent, or reopen the parent with
  board_update"; `not_held` "board_get {id}: nobody holds it; nothing to release";
  `dependency_cycle` "board_get both tickets: one already depends on the other";
  `approval_decide_only` "approvals change only by a person's decision on the approvals
  page"; `approval_changed` and `already_decided` "approval_wait {id} to see it as it is now";
  `not_an_approval` "board_get {id}: it is a ticket, not an approval"; `read_only` "you are a
  viewer here; ask the board's owner for a member role"; `request_conflict` "retry with the
  same content, or use a new request_id for new content"; `board_damaged` "the board's stored
  data is damaged; tell the hub's operator (board_verify shows the chain)".
- `crates/agenthub/src/cli/errors.rs` (`next_for`, `exit_code_for`): `parent_closed`,
  `dependency_cycle`, `not_held`, `read_only`, `invalid` and `approval_decide_only` fall to
  "board --help", and `exit_code_for` puts `parent_closed` in `EXIT_REFUSED` although it is a
  conflict with current state. -> Add the codes with CLI verbs ("board show {id}", "board dep
  ... --remove", ...) and add `parent_closed` to the `EXIT_CONFLICT` arm. `approval_decide_only`
  is reachable through `board set` on an approval ticket (`update`). The other approval codes
  (`not_an_approval`, `approval_changed`, `already_decided`) are not reachable from the CLI
  (`Face` has no approval method).
- `docs/agent-board.md` (§4, the stable code list, lines 106-110): names 20 codes and leaves
  out `not_held`, `already_decided`, `approval_changed`, `approval_decide_only`,
  `not_an_approval`, `unknown_environment`, `invalid_settings`, `plan_limit`, `not_owner`,
  `read_only`, `not_granted`, `unknown_board`, `too_many`, `board_damaged`,
  `request_conflict`, `delivery_conflict`, `no_done_state`, `wake_failed`. -> Extend the list
  and name the contract test as its source.

### Rule 11: board tools

- `crates/hub/src/agenthub/mcp/mod.rs` (`READ_ONLY_TOOLS`, lines 107-120) with
  `crates/hub/src/agenthub/mcp/tools/messages.rs` (`board_inbox`): `board_inbox` is in the
  viewer list but declares `read_only_hint = false`, because `ack_through` writes the caller's
  read cursor. The list's doc ("The board tools that change nothing") is then wrong. -> Add
  `const PER_CALLER_WRITES: &[&str] = &["board_inbox"];` beside it with a doc comment, fix the
  `READ_ONLY_TOOLS` doc, and hold both in the contract test.
- `crates/hub/src/agenthub/mcp/tools/tickets.rs` (`board_claim`, line 101): `idempotent_hint =
  true`, but without `id` it claims the next ready ticket each time. -> `idempotent_hint =
  false`. The description already says re-claiming by id returns the current lease.
- The tool-form violations on board tools (no `deny_unknown_fields` on the 22 input structs,
  46 undocumented input fields, missing annotation titles, `board_settings`
  `destructive_hint`, and refusals made before the audit line) are listed once, in
  mcp-tools.md.

### Rule 12: audit

- `crates/hub/src/agenthub/mcp/calls.rs` (`board_call_scoped`, lines 170-186) and
  `crates/hub/src/web/app/agenthub/ticket_actions.rs` (`act`, lines 115-136): both write the
  audit line after the store committed, and on a failed write report a refusal (`refuse(error)`;
  `done=no`) for a write that happened. -> G7: return the store's result, log the failure and
  raise `audit.write_failed`. No "started" pre-row (C9).
- `crates/hub/src/web/approvals.rs` (`grant` line 256, `revoke` line 293, `test` line 380,
  `decide` lines 447-462), `crates/hub/src/web/app/me.rs` (`stop_own`) and
  `crates/hub/src/agenthub/issue_lifecycle/mod.rs` (`shared_state_write_route`, line 254): these
  writes reach the board's chain but leave no audit line. -> One line each, with tool
  `approval_allow`, `approval_revoke`, `approval_request`, `approval_decide` or
  `board_issue_checkpoint`, broker the project and operation_id the ticket, under rule 12.

### Rule 13: one operation from every surface

- `crates/hub/src/agenthub/mcp/tools/read.rs` (`board_issue_context`, `board_issue_checkpoint`,
  `board.hosted()` at lines 75 and 116) and `crates/agenthub/src/hosted/issues.rs`
  (`write_issue_context`, which takes `actor: &str`): the tools skip the `Board` face, and the
  operation takes a string actor. -> `Board::issue_context` and `Board::write_issue_context(
  actor: &Actor<'_>, ...)` in a new `board/issues.rs`. The store takes `&Actor<'_>` and uses
  `actor.id` inside. The async REST routes may keep awaiting the `HostedBoard` method.
  (Rules 13, 4.)

### Where files go (layout)

- `crates/hub/src/agenthub/mcp/tools/read.rs` (`board_issue_checkpoint`): a tool that writes,
  in the read router. -> Move it, with `board_attach` and `board_issue_context`, to a new
  `mcp/tools/issues.rs` (`board_issue_router`), joined in `mcp/tools/mod.rs`.
- `crates/agenthub/src/hosted.rs` (823 lines, beside `hosted/`, on the ratchet in
  `crates/devagotchi/tests/layout/oversize.txt`): -> `hosted/mod.rs` with the items listed
  under Where files go; claim, heartbeat, complete, release, expire, clear_lease to
  `hosted/leases.rs`; add_note and insert_note to `hosted/notes.rs`. Move the ratchet entry and
  lower its number.
- `crates/agenthub/src/hosted/operations.rs` (709 lines, on the ratchet): a grab-bag. -> Split
  by subject: `leases.rs` (claim_next, handoff, force_release, reap, next_lease_expiry),
  `settings.rs` (settings, workflow, put_settings, save_configuration as `pub(super)`,
  set_project, put_workflow, and edit_settings from `hosted/approvals.rs`), `graph.rs` (deps,
  add_dep, remove_dep), `reads.rs` (list_in, list, ready, verify_chain), `tickets.rs`
  (welcome_if_new, update). Remove its ratchet entry.
- `crates/agenthub/src/migrations.rs` (18 lines, beside `migrations/`, which holds only
  `tests.rs`): -> `src/migrations/mod.rs`, keeping `#[cfg(test)] mod tests;`.
- `crates/agenthub/src/cli/hooks/mod.rs` (front door): starts with `use`, with no `//!`, and
  keeps its tests inline (lines 185-347); a front door moves its tests out (G10, kept under
  C10). -> A `//!` naming the four phases, the 256 KiB stdin bound, the exit codes (2 blocks
  Stop; 5 when `stop_hook_active` or at session end) and `plugin_hooks()` as the plugin's
  source. Tests to `cli/hooks/tests.rs`.
- `crates/agenthub/src/board/settings.rs` (`issue_source`, lines 57-62): a GitHub issue read in
  the settings face. -> `board/issues.rs` (see rule 13).

### Docs match code (G11)

- `docs/agent-board.md` (§6, line 167): "Nineteen tools", but the hub serves 26 `board_*` and
  `approval_*` tools (read.rs 10, tickets.rs 10, messages.rs 4, settings.rs 2). -> Name them by
  group, or point to `crate::mcp::tool_names()` (hub crate) and the contract test instead of a count.
- `plugins/README.md` (lines 48-49): names `crates/hub/src/plugin_tests.rs`, which does not
  exist. -> `crates/hub/src/mcp/tests/plugins.rs`.
- `crates/hub/src/agenthub/tests/ahead.rs` (module doc, lines 6-7): "On SQLite, and on Postgres
  when the test server is set". Boards are Postgres only. -> "On the test Postgres
  (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI's hub-postgres job sets it)."
- `crates/hub/src/agenthub/mcp/mod.rs` (front-door doc, lines 1-3 and 12-15): cites `cli.rs`
  (it is `crates/agenthub/src/cli/`), calls a configured board "a directory on this host" and
  puts account boards "under `account_boards.dir`", a field `AccountBoardsConfig` does not
  have. -> Configured boards are named in the hub config and live in the hub's Postgres under
  `SHARED_OWNER`; account boards are made by `board_bind` and counted against
  `account_boards.free_limit`; the CLI is `crates/agenthub/src/cli/`.
