# Pattern: a machine capability, end to end

A capability is one thing an AI can do on a user's machine: run a command, read a file, search,
list processes, read a URL. It is one broker request, served by two MCP tools: the local one
(`devagotchi mcp`) and the hub one, which adds `machine`. Follow this pattern for every new
capability and every change to one.

The shared rules [G1-G14][G] hold here and are not repeated. This page states only what is
particular to a capability. The general form of an MCP tool (names, titles, hints, answers,
audit) is [mcp-tools.md](mcp-tools.md). Where a file goes is [layout.md](layout.md), with G10.

Who decides what (G1):
- The broker (`crates/machine-engine`) is the only authority. Serde refuses unknown fields; the
  broker checks every bound and every policy.
- The local adapter (`crates/mcp`) passes the request through.
- The hub names the machine and passes the request on (module docs of
  `crates/hub/src/machines/system.rs`), and `Hub::call` audits the call.

## Exemplar

`broker_capabilities` (hub: `machine_inspect`) is the exemplar for the chain and its tests. It
is the only capability with a test at every layer.

| Layer | Where |
|---|---|
| Contract | `crates/contracts/src/system.rs:53-80` (`CapabilitiesInput`, `BrokerCapabilities` with field docs); `REQUEST_METHODS` at `system.rs:21-47` |
| Contract tests | `crates/contracts/src/system/tests.rs`: `capabilities_request_is_read_only_and_takes_nothing` (wire shape, unknown param refused, `held_for`, `target`, `caller`), `request_methods_are_every_request_the_wire_reads`, `capabilities_carry_no_path_grant_or_account` (an allow-list of answer fields) |
| Broker dispatch | `crates/machine-engine/src/broker/system.rs:32-35`, in `Broker::on_system` |
| Broker implementation | `crates/machine-engine/src/broker/settings.rs:107-137` (`Broker::capabilities`): policy only; a closing or faulted broker refuses. `GRANTED_REFUSES` at `settings.rs:22` |
| Broker tests | `crates/machine-engine/src/broker/tests/capabilities.rs`: `capabilities_describe_both_policies_without_work_or_private_configuration`, `capabilities_refuse_a_closing_or_faulted_broker` |
| Shared bounded helper | `crates/mcp/src/capabilities.rs` (`response`), tested in `crates/mcp/src/capabilities/tests.rs`; used by both surfaces |
| Local tool | `crates/mcp/src/adapter/system_tools.rs:36-57`, description `describe::INSPECT` |
| Hub tool | `crates/hub/src/machines/system.rs:101-120` (`machine_inspect`), through `Hub::call` in `crates/hub/src/mcp/hub.rs`; Rust test `crates/hub/src/machines/tests/inspect.rs` |
| Probes | `tests/adapter_probe.py:91-108`, `tests/hub_probe.py:183-207`, `tests/mcp_protocol.py:31-32` |

For a request with inputs that pages, read `search_files`:
- Contract: `crates/contracts/src/search.rs` (module docs, named bounds, documented defaults).
- Implementation: `Scope::search_for` in `crates/machine-engine/src/files/search.rs`. It checks
  every input before it touches the disk.
- Tests: the inline module in the same file, including
  `pages_go_on_from_their_cursor_without_losing_or_repeating`.

For the simplest pass-through pair, read `set_config_value`:
`crates/mcp/src/adapter/system_tools.rs:59-74` and `crates/hub/src/machines/system.rs:122-144`.

Neither exemplar is perfect. `machine_inspect` is the one allowed name exception (rule 14).
`search_files` has the `PAGE_TIME` deadline bug, a description copied by hand and an inline
test module (see Known violations).

## The chain

| # | Layer | Where | What you add |
|---|---|---|---|
| 1 | Contract | the family module in `crates/contracts/src/` (`files`, `search`, `system`, `web`, `interactive`); `lib.rs` keeps the operation core and the envelope | `XInput`, the answer type, its bounds and defaults as named constants |
| 2 | Envelope | `lib.rs`, `interactive.rs`, `system.rs` | `RequestBody::X`; a `ResponseBody` kind (or a reused one) with its `kind()` arm; the `REQUEST_METHODS` entry; the `target()`, `caller_slot()`, `caller()` and `held_for()` arms |
| 3 | Words | `crates/contracts/src/describe.rs` | `pub const X: &str`, the tool description, written once |
| 4 | Broker | `Broker::handle` in `broker.rs`, the family dispatcher, the domain implementation | a dispatch arm, the implementation, the policy decision |
| 5 | Local tool | `crates/mcp/src/adapter/<family>_tools.rs` | a `#[tool]` that passes `XInput` through |
| 6 | Hub tool | `crates/hub/src/machines/<family>.rs` | a `#[tool]` with the same name taking `{machine, ...XInput}` through `Hub::call`, and a scope in `scope_for` (`crates/hub/src/identity/accounts/api_keys/scope.rs`) |
| 7 | Tests | see "The canonical test" | contract, domain, broker, hub parity, probes |

Also update:
- the export root `BrokerContracts` (`crates/contracts/src/bin/export-contracts.rs`), then run
  `node scripts/generate-contracts.cjs`;
- both servers' instructions: the adapter's `#[tool_handler(instructions = ...)]` in
  `crates/mcp/src/adapter.rs`, and `mcp.instructions` in `crates/hub/site/default/site.json`;
- the *Serves* section of `crates/hub/src/machines/mod.rs`;
- `.ai/docs/desktop-commander-parity.md`. A changed tool schema is noted there and in the
  changelog. Tool schemas change freely in alpha (C17); the link wire does not.

## Canonical shape

### 1. Contract (modelled on `crates/contracts/src/search.rs`)

```rust
//! `thing_tool`: what it does, how it pages, what it never holds open.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most results one page returns.
pub const MAX_THING_RESULTS: u32 = 500;
/// Results on a page when the caller names no number.
pub const DEFAULT_THING_RESULTS: u32 = 100;

/// Public, so the hub mirror takes the same default (rule 15, G3).
pub fn default_thing_results() -> u32 {
    DEFAULT_THING_RESULTS
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThingInput {
    /// Directory (absolute, `~/...`, or relative to the workspace).
    pub path: String,
    /// Most results on this page, 1..=500 (default 100).
    #[serde(default = "default_thing_results")]
    pub max_results: u32,
    /// Where to go on from: the previous page's `next_cursor`. Absent for the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// A struct, never a bare Vec or enum: MCP restricts outputSchema to an object root.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)] // closed on the wire; tools/list opens it (devagotchi_mcp::outputs)
pub struct ThingPage {
    pub path: String,
    pub matches: Vec<ThingMatch>,
    pub complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
```

Then wire it into the envelope:
- `lib.rs`: `RequestBody::Thing(ThingInput)`, `ResponseBody::Thing(ThingPage)`, and
  `Self::Thing(_) => "thing"` in `kind()`. A request that does not wait on purpose goes in the
  zero arm of `held_for()`.
- `system.rs`: `"thing"` in `REQUEST_METHODS`, in `RequestBody` order.
- `interactive.rs`: `Self::Thing(input) => input.path.clone()` in `target()`, and the `Thing`
  arm in `caller_slot()` and `caller()`.
- `describe.rs`: `pub const THING: &str = "...";`.

### 2. Broker (from `Broker::on_system` and `Broker::processes` in `crates/machine-engine/src/broker/system.rs`, condensed)

```rust
// broker.rs, Broker::handle: every variant named; a family goes to its dispatcher.
request @ (RequestBody::FileSearch(_) | /* ... */) => self.on_files(request).await,

// broker/system.rs, on_system: (tool name, answer); a refusal goes to the broker log.
RequestBody::ProcessList(input) => (
    "list_processes",
    self.processes(input).await.map(ResponseBody::Processes),
),

// The implementation: refuse first, then do blocking work on the blocking pool.
async fn processes(&self, input: ProcessListInput) -> BridgeResult<ProcessPage> {
    self.account_only(PROCESSES)?;                               // policy
    if !(1..=MAX_PROCESS_RESULTS).contains(&input.max_results) { // every bound, named
        return Err(invalid(format!("max_results must be 1..={MAX_PROCESS_RESULTS}")));
    }
    let roots = self.job_roots();
    blocking(move || {
        let table = Table::read(true)?;
        Ok(table.page(&roots, input.filter.as_deref(), input.max_results as usize))
    })
    .await
}
```

### 3. Local tool (from `crates/mcp/src/adapter/system_tools.rs:59-74`, with C3 applied)

```rust
#[tool(
    name = "set_config_value",
    description = describe::SET_CONFIG_VALUE,
    title = "Change a broker limit",
    annotations(title = "Change a broker limit", read_only_hint = false, destructive_hint = true,
                idempotent_hint = true, open_world_hint = false)
)]
async fn set_config_value(
    &self,
    Parameters(input): Parameters<ConfigSetInput>,
    context: RequestContext<RoleServer>,
) -> Result<Json<ConfigView>, Json<BridgeError>> {
    match self.request(RequestBody::ConfigSet(input), context).await? {
        ResponseBody::Config(view) => Ok(Json(view)),
        other => Err(Json(unexpected("config", &other))), // a single-kind decoder keeps `other`
    }
}
```

`destructive_hint` is `true` because lowering `max_operations` retires kept jobs
(`shrink` in `crates/machine-engine/src/broker/history.rs`). Today's code still says `false`
on both surfaces (Known violations, rule 16).

The family router is `#[tool_router(router = system_router, vis = "pub(super)")]`. It is added
in `BridgeAdapter::router()` (`crates/mcp/src/adapter.rs:332-340`), which opens the output
schemas (`devagotchi_mcp::outputs::open_outputs`).

### 4. Hub tool (from `crates/hub/src/machines/system.rs:122-144`, with C3 and C4 applied)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetConfigInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    // Every field of ConfigSetInput: same names, same types, same docs, same defaults.
    pub key: String,
    pub value: u64,
}

#[tool(name = "set_config_value", description = describe::SET_CONFIG_VALUE,
       title = "Change a machine's broker limit",
       annotations(/* the same four hints as the local tool */))]
async fn set_config_value(
    &self,
    Parameters(input): Parameters<SetConfigInput>,
    context: RequestContext<RoleServer>,
) -> Result<Json<ConfigView>, Json<BridgeError>> {
    let body = RequestBody::ConfigSet(ConfigSetInput { key: input.key, value: input.value });
    match self.call("set_config_value", &input.machine, body, context).await? {
        ResponseBody::Config(view) => Ok(Json(view)),
        other => Err(unexpected_response("config", &other)),
    }
}
```

A field with a default takes the contract's type and the contract's function (C4):

```rust
/// Most processes to return, 1..=1000 (default 200), busiest first.
#[serde(default = "devagotchi_contracts::default_process_results")]
pub max_results: u32,
```

`Hub::call` (`crates/hub/src/mcp/hub.rs`):
- sets the caller from the session (G8);
- routes to the machine's link or local socket;
- honours MCP cancellation (`request_cancelled`);
- writes the one audit line (G7).

A composite (`run`, `read_multiple_files`, `read_image`) calls its shared function in
`crates/mcp/src/` (`run::run`, `files::read_many`, `image::read_image`) with
`|body| self.send(&route, body)` (`run` uses `send_hinted`) and writes its own one audit line.
It moves to `Hub::recorded` once that lands (G7; `Hub::recorded` comes with plan B1, the
composites move to it in B4).

## Rules

### Contract

1. **One request, one variant, one input type.** A capability is one `RequestBody` variant,
   whose method is its snake_case name, with one `*Input` struct. Its answer is one
   `ResponseBody` kind. Reuse a kind only when the answer type is the same (`Info`, `Written`,
   `Operation`, `Config`).
2. **Closed inputs (G2).** Every wire struct derives `Serialize, Deserialize, JsonSchema` and
   carries `#[serde(deny_unknown_fields)]`. Every local tool's `inputSchema` then has
   `additionalProperties: false`
   (`every_output_schema_is_open_and_every_input_closed` in `crates/mcp/src/adapter/tests.rs`);
   the hub side is held by the parity test.
3. **Every input field has a `///` doc, and a bounded field states its range and default**, as
   in `1..=500 (default 100)`. The doc is what the AI reads in `inputSchema`. The bound and the
   default are named in the contract module (G3: `MAX_*`, `DEFAULT_*`, `default_*`), so the
   broker, the hub mirror and the description use one number.
4. **A closed set in an input is an enum** with `rename_all = "snake_case"` (`SearchTarget`,
   `WriteMode`, `ListScope`, `InputMode`, `Key`), so `inputSchema` lists the values. One
   exception: `ConfigSetInput.key` stays a string, so the broker can say why `workspace`,
   `exec_policy` and `allowed_executables` are the operator's (`Broker::set_config` in
   `broker/settings.rs`).
5. **The transport sets `caller` (G8).** A caller-carrying variant is named in `caller_slot()`
   and `caller()`, and its input is in the schema loop of
   `crates/contracts/src/interactive/tests.rs`.
6. **Tables name every variant (G5).** The capability tables are `Broker::handle`, `on_files`
   and `answer`, `mutation`, `on_system`, `RequestBody::{target, caller, caller_slot, held_for}`,
   `ResponseBody::kind` and the hub's `Links::jobs_saw`. The precedent is `mutation` in
   `broker/files.rs`: "Every variant is named, so a new request must decide here."
7. **`REQUEST_METHODS` lists every method in `RequestBody` order**, and every method but
   `operations_changed` (the desktop's own) is a tool, named once in `describe::BROKER_TOOLS`.
   Tests: `request_methods_are_every_request_the_wire_reads` now; the `BROKER_TOOLS` coverage
   test with plan B1.
8. **The description is written once**, as `contracts::describe::X` (G1, C1). Both surfaces use
   it unchanged, in machine-neutral words ("the machine", "its broker"). The hub's routing hint
   lives in the `machine` field's doc and the server instructions, never in the description.
9. **Every input and answer type is in the export root** `BrokerContracts`, and the generated
   schema is current (`node scripts/generate-contracts.cjs --check`).

### Broker

10. **Validate, then act, then answer.** The implementation checks policy and every bound
    first, quoting the named constant in the message, before it touches the filesystem, the
    process table or the network. Blocking work runs on the blocking pool (G6) through
    `on_files_with` or `blocking`. A file mutation takes the `MutationGate`.
11. **Policy is the broker's.** Paths go through `Scope::resolve`, which refuses with
    `path_outside_workspace` under `granted`. Whatever a `granted` broker always refuses
    (`account_only`, or `set_config`'s own check) is listed in `GRANTED_REFUSES`, so
    `broker_capabilities` leaves it out. The broker capabilities test holds the two together.
    MCP annotations are hints and enforce nothing.
12. **Refusals follow G4.** A bound the broker refuses is `invalid_request`. An argument serde
    refuses for a known tool is a tool execution error with `invalid_arguments` (C2); an
    unknown tool stays JSON-RPC -32602 (`tests/mcp_protocol.py`, `unknown_tool_rejected`).
    Until `call_tool` maps it, rmcp 3.9.0 returns -32602 for both (Known violations, rule 12).
13. **The broker answers inside the transport's deadline.** A local client and the machine link
    wait `devagotchi_ipc::IO_TIMEOUT + request.held_for()` (`exchange` in
    `crates/ipc/src/lib.rs`, reached through `devagotchi_ipc::request`; the link forwards
    through the same call in `crates/link/src/session/mod.rs`). Every internal time
    budget is derived from that and held by a `const` assertion beside it (G6), as
    `MUTATION_WAIT` is in `broker/files.rs`. `held_for()` is only for requests that wait on
    purpose (`operation_wait`, `operations_changed`); the link counts them in its waits pool
    (`crates/link/src/session/mod.rs`).

### Tools

14. **The same name on both surfaces.** The local tool takes `Parameters<XInput>` and passes it
    through, with no validation and no defaults of its own. The one exception:
    `broker_capabilities` is `machine_inspect` on the hub
    (`.ai/dev/plans/desktop-commander-parity.md`, P11).
15. **The hub input is the contract input plus `machine`.** It is a flat, closed struct (G2).
    It has the same property names, types, docs and defaults. A default is
    `#[serde(default = "devagotchi_contracts::default_x")]` on the contract's type (C4); never
    an `Option` the contract does not have, and never `unwrap_or` with a number. The hub sends
    the request through `Hub::call` under the tool's own name.
16. **The same annotations on both surfaces.** The hint rules themselves are
    [mcp-tools.md](mcp-tools.md)'s. Two that capabilities trip: `idempotent_hint` is true only
    when the same arguments in every mode add nothing (so `write_file`, with `append`, is
    false), and a read-only tool sets no `destructive_hint` or `idempotent_hint`.
    `set_config_value` is `destructive_hint = true` (C3).
17. **A composite has one implementation** in a module of `crates/mcp/src/` (`run.rs`,
    `files.rs`, `image.rs`), generic over a `send` closure and used by both surfaces. Its bounds
    and defaults live there (`run::bounded`, `run::DEFAULT_WAIT_S`), not in either tool.
18. **Registration.** The family router is in `BridgeAdapter::router()` and in
    `Hub::machine_router()`, the operation tools' router too (C5). The hub tool has an API-key
    scope in `scope_for` (test: `every_tool_is_either_scoped_or_deliberately_closed_to_keys`
    in `crates/hub/src/identity/tests/keys.rs`). Both servers' instructions name the tool.

## Where files go

G10 and layout.md decide. For a capability:

- **Contract:** the family module in `crates/contracts/src/`, re-exported flat from `lib.rs`
  (G10's contracts exception, C11). Its tests: `<family>/tests.rs`.
- **Broker:** dispatch in `broker.rs`; the family dispatcher in `broker/<family>`; the
  implementation in its domain (`files/<op>` on `Scope`, `broker/system/<op>`,
  `broker/settings`). Broker-level tests: `broker/tests/<family>.rs`. Domain tests: the
  domain's `tests.rs`, or inline in a leaf file under the cap (G10, C10).
- **Local tool:** `crates/mcp/src/adapter/<family>_tools.rs`. Composite logic:
  `crates/mcp/src/<module>.rs` (today `run.rs`, `files.rs`, `image.rs`) with
  `<module>/tests.rs`.
- **Hub tool:** `crates/hub/src/machines/<family>.rs` (C5). The `operation_*` tools go in a
  new `machines/operations.rs` (`operations_router`, merged in `machine_router()`); `machines`,
  `machine_add` and `machine_update` go in a new `machines/manage.rs`. Both files are created
  by plan B1. `crates/hub/src/mcp/` keeps
  the `Hub` surface: `call`, `caller`, `send_hinted`, `outdated`, `whoami` and the shared
  schema. Hub tests: `machines/tests/<subject>.rs`. A test that needs no store runs in CI's
  `core` job; one that takes a store runs in `hub-postgres` (G13).

Several capability files predate layout.md: `crates/contracts/src/system.rs` beside `system/`,
`crates/contracts/src/interactive.rs` beside `interactive/`, `crates/machine-engine/src/files.rs`
beside `files/`, and `crates/machine-engine/src/broker/system.rs` beside `broker/system/`. They stay until structure-cleanup S10
(`.ai/dev/plans/structure-cleanup.md`) moves them. New files follow layout.md.

## The canonical test

1. **Contract** (`crates/contracts/src/system/tests.rs`,
   `capabilities_request_is_read_only_and_takes_nothing`): the request parses from
   `{"method", "params"}`, serializes back the same, refuses an unknown param, and has the right
   `held_for`, `target` and `caller`. Add an allow-list of answer fields when the answer must not
   leak (`capabilities_carry_no_path_grant_or_account`).
2. **Domain** (`crates/machine-engine/src/files/search.rs`, inline tests): the implementation on
   `Scope` or `Table` against a `tempfile` tree, or a local server for the web. One test for the
   happy path and one per refusal code. A paged request also needs an invariant test: paging
   one result at a time, or stopping on time at every entry, finds the same results as one page.
3. **Broker** (`crates/machine-engine/src/broker/tests/capabilities.rs`): through
   `Broker::handle(RequestBody::X)` on both `open_broker` (granted) and `account_broker`. Check
   the answer kind, the policy refusal codes, and that a query admits, journals and signals
   nothing.
4. **Hub parity** (see The mechanical check): the tool exists on both surfaces with one
   description, the same hints, and hub properties equal to the contract's plus `machine`. Add
   a hub Rust test (as `crates/hub/src/machines/tests/inspect.rs`) when the hub adds behaviour
   of its own, such as a bound or an audit outcome.
5. **Probes**, run by `tests/e2e.py` in CI's `e2e` job on Linux and Windows:
   - `tests/adapter_probe.py`: the name in `missing`, and a call in the `--account` section.
   - `tests/hub_probe.py`: one `check("<tool> <what it does>", ...)` on `args.run_broker`, one
     refusal on the granted `args.broker`, and the name in `broker_tools`.
   - `tests/mcp_protocol.py`: a `TOOL_SCHEMAS` entry. Today nothing runs this probe; plan B1
     runs it from `tests/e2e.py` (C16).

## Anti-patterns

- A wildcard in a capability table (rule 6). The link uses `caller()` to refuse a hub that
  speaks for the machine's own account (`from_a_hub` in `crates/link/src/session/mod.rs`); a new
  caller-carrying request missing there passes silently.
- Validation or defaults restated in a tool or a hub mirror (`unwrap_or(200)`), or a bound
  restated as a literal (`"1..=4096"`): two numbers that drift (G3).
- A description written twice. The hub and local texts have drifted already (`edit_file`,
  `operation_read`, `operation_start`).
- An internal time budget at or over the IPC deadline (`PAGE_TIME = 5 s` and
  `FETCH_TIME = 20 s` against `IO_TIMEOUT = 5 s`).
- A hub input nested as `{broker, input}`, or without `deny_unknown_fields`.
- A probe that polls `operation_get` in a loop. Use `operation_wait`.
- A capability added without its probe lines.

## The mechanical check

Held today:
- `request_methods_are_every_request_the_wire_reads` (rule 7).
- `every_output_schema_is_open_and_every_input_closed` (rule 2, local surface).
- `every_tool_is_either_scoped_or_deliberately_closed_to_keys` (rule 18, on Postgres).
- The broker capabilities tests (rule 11).
- The `MUTATION_WAIT` const assertion (rule 13).
- The directory gate in `tests/hub_probe.py` (every hub tool has a title and the hints).
- rustc's exhaustiveness on every table that has no wildcard (rule 6).

Held by review until plan B1 lands these, each green with the fix it guards (G12):

1. **Parity test**, a new `crates/hub/src/machines/tests/parity.rs` (rules 2, 8, 14, 15, 16).
   It is the one parity test; the form checks belong to `check_tools`, a new
   `crates/mcp/src/pattern.rs` that [mcp-tools.md](mcp-tools.md) owns (C14). `Hub::router()` needs no store, so it runs in `core`. It needs a local tool list:
   make `BridgeAdapter::router()` `pub(crate)` and add to `crates/mcp/src/lib.rs`:

   ```rust
   /// Every tool `devagotchi mcp` serves, as tools/list describes it: for parity tests.
   pub fn local_tools() -> Vec<rmcp::model::Tool> { adapter::BridgeAdapter::router().list_all() }
   ```

   ```rust
   // Uses std::collections::BTreeMap, serde_json::Value, rmcp::model::{JsonObject, Tool}
   // and devagotchi_contracts::describe.
   const HUB_NAME: [(&str, &str); 1] = [("broker_capabilities", "machine_inspect")];
   const COMPOSITES: [&str; 3] = ["run", "read_multiple_files", "read_image"];
   fn hints(t: &Tool) -> [Option<bool>; 4] {
       let a = t.annotations.clone().unwrap_or_default();
       [a.read_only_hint, a.destructive_hint, a.idempotent_hint, a.open_world_hint]
   }
   // An input with no fields may have no `properties` at all, so never index.
   fn props(s: &JsonObject) -> BTreeMap<String, Value> {
       s.get("properties").and_then(Value::as_object).cloned().unwrap_or_default().into_iter().collect()
   }
   #[test]
   fn every_machine_tool_is_the_same_tool_on_both_surfaces() {
       let local: BTreeMap<_, _> = devagotchi_mcp::local_tools().into_iter().map(|t| (t.name.to_string(), t)).collect();
       let hub: BTreeMap<_, _> = crate::mcp::tools().into_iter().map(|t| (t.name.to_string(), t)).collect();
       for name in describe::BROKER_TOOLS.iter().map(|(t, _)| *t).chain(COMPOSITES) {
           let hub_name = HUB_NAME.iter().find(|(l, _)| *l == name).map_or(name, |(_, h)| *h);
           let (l, h) = (&local[name], &hub[hub_name]);
           assert_eq!(l.description, h.description, "{name}: one description (describe::)");
           assert_eq!(hints(l), hints(h), "{name}: the same annotations");
           assert_eq!(h.input_schema.get("additionalProperties"), Some(&Value::Bool(false)), "{name}: hub arguments closed");
           let mut hp = props(&h.input_schema);
           assert!(hp.remove("machine").is_some(), "{name}: the hub names the machine");
           let lp = props(&l.input_schema);
           assert_eq!(lp.keys().collect::<Vec<_>>(), hp.keys().collect::<Vec<_>>(), "{name}: the contract's fields");
           for (k, v) in &lp {
               assert_eq!(v.get("description"), hp[k].get("description"), "{name}.{k}: the contract's words");
           }
       }
   }
   ```

   From then on CI fails on a capability added to one surface only, a copied description, a
   drifted hint or doc, or a nested `{broker, input}` hub input.
2. **`describe::BROKER_TOOLS`** (rule 7), with a test in `crates/contracts/src/system/tests.rs`:

   ```rust
   /// Each broker request an AI reaches, by its tool name. `operations_changed` is the desktop's own.
   pub const BROKER_TOOLS: [(&str, &str); 22] = [("operation_start", "operation_start"), /* ... */];

   #[test]
   fn every_request_but_the_desktops_own_is_a_tool() {
       let mut methods: Vec<_> = crate::describe::BROKER_TOOLS.iter().map(|(_, m)| *m).collect();
       methods.push("operations_changed");
       methods.sort();
       let mut all = REQUEST_METHODS.to_vec();
       all.sort();
       assert_eq!(methods, all);
   }
   ```
3. **Deadline assertions** beside each budget (rule 13):
   - `files/search.rs`: `const _: () = assert!(PAGE_TIME.as_millis() < devagotchi_ipc::IO_TIMEOUT.as_millis());`
   - `broker/system/web.rs`: the same for `FETCH_TIME` (D2: derived under `IO_TIMEOUT`,
     `held_for` not used).
   - `broker/system.rs`: `const _: () = assert!(EXIT_WATCH.as_millis() + 1000 < devagotchi_ipc::IO_TIMEOUT.as_millis());`
4. **No wildcards** in the five tables listed under rule 6 in Known violations, so rustc holds
   rule 6. Single-kind decoders keep `other =>`. If CI gains clippy, deny
   `clippy::wildcard_enum_match_arm` on those functions only, since the decoders would trip it
   crate-wide.
5. **CI steps:**
   - `node scripts/generate-contracts.cjs --check` and `devagotchi ai check` in a new CI
     `static` job (rule 9). `.github/workflows/ci.yml` has no such job today.
   - `tests/mcp_protocol.py --devagotchi <bin> --socket UNUSED_ENDPOINT` from `tests/e2e.py`,
     so its exact `TOOL_SCHEMAS` match runs on Linux and Windows (C16).
   - In `tests/adapter_probe.py`, build `missing` from `mcp_protocol`'s `TOOL_SCHEMAS` keys, so a
     new tool must be named there.

## Upstream sources

- MCP tools, 2025-11-25 (the version the repo negotiates, `INITIALIZE_VERSION` in
  `tests/mcp_protocol.py`): https://modelcontextprotocol.io/specification/2025-11-25/server/tools.
  Input validation and business errors are tool execution errors with `isError: true`; protocol
  errors are for unknown tools and malformed requests. Servers must validate all tool inputs.
  Grounds rules 2 and 12.
- MCP schema, 2025-11-25: https://modelcontextprotocol.io/specification/2025-11-25/schema.
  `Tool.outputSchema` is restricted to `type: "object"` at the root, so every answer is a
  struct. `ToolAnnotations` defaults (readOnly false, destructive true, idempotent false,
  openWorld true); destructive and idempotent mean something only when readOnly is false.
  Annotation meanings are the same in 2025-06-18. Grounds the struct answer in the canonical
  shape, and rule 16.
- rmcp `#[tool_router]` with `router`, `vis` and `+`:
  https://docs.rs/rmcp/latest/rmcp/attr.tool_router.html. The per-family routers follow it.
- rmcp's counter example:
  https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/common/counter.rs.
  The adapter follows it and overrides `call_tool` to keep its history.
- rmcp README: https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md. In the
  locked rmcp 3.9.0, `parse_json_object` in `src/handler/server/tool.rs` turns a failed
  `Parameters<T>` into `ErrorData::invalid_params` (-32602), the deviation under rule 12.
- serde `deny_unknown_fields` and `flatten`:
  https://serde.rs/container-attrs.html#deny_unknown_fields and https://serde.rs/field-attrs.html.
  The two do not combine, so the hub mirrors the contract fields beside `machine` (rule 15).
- serde adjacently tagged enums: https://serde.rs/enum-representations.html. The
  `RequestBody {method, params}` and `ResponseBody {kind, value}` wire shape.
- tokio `spawn_blocking`: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html, and
  `tokio::fs`: https://docs.rs/tokio/latest/tokio/fs/index.html. Grounds rule 10.
- clippy `wildcard_enum_match_arm`:
  https://rust-lang.github.io/rust-clippy/master/index.html#wildcard_enum_match_arm. Upstream
  backing for rule 6.
- Desktop Commander, the product being matched: https://github.com/wonderwhy-er/DesktopCommanderMCP.
  Tool names follow it. Stateless paging (`search_files` cursor, `operation_id`) is a deliberate,
  documented difference (`.ai/docs/desktop-commander-parity.md`).

## Known violations

Checked at main `2b3feec`. Plan B1 fixes all of them unless noted.

### Rule 3: docs, named bounds and defaults

- `crates/machine-engine/src/files.rs` (`MAX_ENTRIES`, `MAX_DEPTH`): list_directory's bounds
  are private to the broker; the contract docs on `FileListInput` restate "0..=3" and
  "1..=1000" by hand, and the hub docs "0 to 3" and "1 to 1000". ->
  `pub const MAX_LIST_ENTRIES: u32 = 1000;` and
  `pub const MAX_LIST_DEPTH: u32 = 3;` in `crates/contracts/src/files.rs`, used by the broker's
  checks in `Scope::list`.
- `crates/machine-engine/src/broker/settings.rs` (`OUTPUT_LIMIT`, `OPERATIONS`, `CONCURRENT`,
  `checked`) and `crates/contracts/src/describe.rs` (`SET_CONFIG_VALUE`): the ranges are
  private; `checked` writes "1..=4096" and "1..=64" as literals, and the description restates
  "1024 to 16777216", "1 to 4096" and "1 to 64". -> Range constants beside
  `CHANGEABLE_SETTINGS` in `crates/contracts/src/system.rs`, `checked`'s messages formatted
  from them, and a contract test that `describe::SET_CONFIG_VALUE` contains each range's ends.
- `crates/contracts/src/system.rs` (`ProcessKillInput.pid`) and `crates/contracts/src/files.rs`
  (`FileWriteInput.mode`): no `///`, so the properties carry no description. ->
  `/// The process ID, from list_processes.` and
  `/// overwrite (default), create (refuse if it exists) or append.`

### Rule 5: the transport sets `caller`

- `crates/contracts/src/interactive/tests.rs`
  (`a_caller_in_a_tool_call_is_not_offered_and_a_transport_replaces_it`): the schema loop checks
  `StartInput`, `ListInput`, `SendInput` and `ResizeInput`, but not `GetInput`, `ReadInput` or
  `WaitInput`, which also carry `caller`. -> Add `schema_for!` of the three to the loop.
- `crates/contracts/src/interactive.rs` (`RequestBody::target`): the doc line "The caller this
  request names, for a kind of request that carries one." sits on `target()`, not on
  `caller()`, so rustdoc shows both wrong. -> Move it onto `pub fn caller(&self)`.

### Rule 6: tables name every variant (G5)

- `crates/contracts/src/interactive.rs` (`RequestBody::caller`): ends with `_ => None`. The link's
  `from_a_hub` uses it, so a new caller-carrying request missed here passes that check
  silently. -> Name every variant, as `caller_slot` does.
- `crates/contracts/src/lib.rs` (`RequestBody::held_for`): ends with
  `_ => std::time::Duration::ZERO`, so a new request that holds gets the 5 s deadline silently.
  -> Name the other variants in an explicit `=> Duration::ZERO` arm.
- `crates/hub/src/machines/link/calls.rs` (`Links::jobs_saw`): `_ => return` leaves out
  `ResponseBody::InputSent`, whose `operation` carries the job's state after `operation_input`,
  so the hub's kept job state does not move on it. -> Add
  `ResponseBody::InputSent(sent) => (&sent.operation, None)`, list the kinds that carry no single
  operation in a `=> return` arm, and add a case beside the `jobs_saw` tests in
  `crates/hub/src/web/app/tests.rs`.
- `crates/machine-engine/src/broker/files.rs` (`answer`): ends with
  `_ => ("file request", Err(internal_error))`, while `mutation` in the same file names every
  variant. -> List the non-file variants in the error arm, or pass `answer` a narrowed type.
- `crates/machine-engine/src/broker/system.rs` (`Broker::on_system`): ends with
  `_ => ("system request", Err(internal_error))`. -> The same as `answer`.

### Rule 8: the description is written once

- `crates/mcp/src/adapter/file_tools.rs` (`SEARCH_DESCRIPTION`) and
  `crates/hub/src/machines/search.rs` (`SEARCH_ON_MACHINE`): the same text copied by hand, the
  hub's with a prefix. -> `describe::SEARCH_FILES` on both surfaces; the machine hint goes in the
  `machine` field doc.
- 16 tools with two descriptions written separately: `run` and six `operation_*` tools in
  `crates/mcp/src/adapter.rs`, nine file tools in `crates/mcp/src/adapter/file_tools.rs`, and on
  the hub `crates/hub/src/machines/run.rs`, `crates/hub/src/mcp/tools/operations.rs` (five),
  `crates/hub/src/mcp/callers/mod.rs` (`operation_list`) and `crates/hub/src/machines/files.rs`
  (nine). They have drifted: the hub's `edit_file` says you are told how many times `old_text`
  occurs, the hub's `operation_read` drops "Reading does not advance another reader's cursor",
  only the local `operation_start` says "For most commands use run instead", and the hub's `run`
  omits `operation_wait`. -> `describe::{RUN, OPERATION_START, OPERATION_GET, OPERATION_WAIT,
  OPERATION_READ, OPERATION_CANCEL, OPERATION_LIST, READ_FILE, LIST_DIRECTORY, WRITE_FILE,
  EDIT_FILE, READ_MULTIPLE_FILES, READ_IMAGE, GET_FILE_INFO, CREATE_DIRECTORY, MOVE_FILE}` in
  machine-neutral words, used by both `#[tool]`s. `OPERATION_LIST` says what holds on both
  surfaces: the caller's own account's operations.
- `crates/mcp/src/adapter/terminal.rs` (`INPUT_DESCRIPTION`, `RESIZE_DESCRIPTION`), re-exported
  by `crates/mcp/src/lib.rs` and imported by `crates/hub/src/mcp/callers/mod.rs`: shared, but
  not from `describe.rs` (C1). -> `describe::{OPERATION_INPUT, OPERATION_RESIZE}`; drop the
  re-export.

### Rule 9: the export root

- `crates/contracts/src/bin/export-contracts.rs` (`BrokerContracts`): lacks `SendInput`,
  `ResizeInput` and `InputSent`, so the published schema has no `operation_input` or
  `operation_resize` types. `node scripts/generate-contracts.cjs --check` runs by hand only
  (`.ai/dev/workflow.md`); `.github/workflows/ci.yml` never runs it. -> Add the three fields,
  regenerate, and run `--check` in the new CI `static` job.

### Rule 10: blocking work on the blocking pool (G6)

- `crates/machine-engine/src/broker/settings.rs` (`Broker::set_config`, `save`,
  `Broker::config_view`, `account_name`): `save` runs `std::fs` read, write, set_permissions and
  rename on an async worker under the registry lock, and `account_name` runs sysinfo on one for
  `get_config` too. Saving under the lock is deliberate; doing it on an async worker is not. ->
  Run both through `blocking`, with `set_config` and `config_view` as functions of the cloned
  shared state so the closure is `'static`. The lock order stays.

### Rule 12: refusals (G4)

- `crates/mcp/src/adapter.rs` (`BridgeAdapter::call_tool`) and `crates/hub/src/mcp/handler.rs`
  (`call_tool`): rmcp 3.9.0 turns an argument serde refuses into JSON-RPC -32602, where MCP
  2025-11-25 wants a tool execution error. Low severity; it is upstream's default. -> Map
  `INVALID_PARAMS` for a tool the router has to `CallToolResult::error` with
  `BridgeError::new("invalid_arguments", message)`; leave an unknown tool at -32602. Record the
  deviation in `.ai/docs/desktop-commander-parity.md` until then.

### Rule 13: inside the transport's deadline

- `crates/machine-engine/src/files/search.rs` (`PAGE_TIME`): 5 s, equal to `IO_TIMEOUT`, with
  `held_for()` zero. A page stopped by time answers after the client has stopped waiting, so it
  reaches the AI as `ipc_timeout` instead of a page with a cursor, locally and through the hub.
  -> `const PAGE_TIME: Duration = devagotchi_ipc::IO_TIMEOUT.saturating_sub(Duration::from_secs(2));`
  with a const assertion, as `MUTATION_WAIT` does.
- `crates/machine-engine/src/broker/system/web.rs` (`FETCH_TIME`): 20 s with `held_for()` zero.
  A fetch over about 5 s fails as `ipc_timeout` while the broker keeps fetching. -> Default D2:
  `IO_TIMEOUT.saturating_sub(Duration::from_secs(1))` with a const assertion. A read is ordinary
  work, not a deliberate hold.
- `crates/machine-engine/src/broker/system.rs` (`EXIT_WATCH`): `kill_process` reads the whole
  process table and then watches up to 3 s, leaving under 2 s of the deadline, with no assertion.
  -> `const _: () = assert!(EXIT_WATCH.as_millis() + 1000 < devagotchi_ipc::IO_TIMEOUT.as_millis());`
  If the table read measures over 1 s on Windows CI, lower `EXIT_WATCH` and update
  `describe::KILL_PROCESS` ("3 seconds").

### Rule 15: the hub input is the contract input plus `machine`

- `crates/hub/src/machines/files.rs` (`read_file`, `list_directory`, `edit_file`,
  `read_multiple_files`), `crates/hub/src/machines/search.rs` (`search_files`),
  `crates/hub/src/machines/system.rs` (`list_processes`) and
  `crates/hub/src/machines/history.rs` (`get_recent_tool_calls`): the hub restates contract
  defaults with `unwrap_or` (`MAX_FILE_READ_BYTES`, 200, 1, 100, 200, 50) and widens the fields
  to `Option`, so the hub's `inputSchema` differs. The contract defaults are private
  (`default_file_read_bytes`, `default_list_entries`, `default_replacements` in `files.rs`,
  `default_search_results` in `search.rs`, `default_process_results` in `system.rs`,
  `default_recent_calls` in `crates/contracts/src/history.rs`). -> `pub const DEFAULT_*` and `pub fn default_*`
  (G3, C4); the hub fields take the contract's type with
  `#[serde(default = "devagotchi_contracts::default_*")]` and pass through unchanged.
- `crates/hub/src/machines/search.rs`, `crates/hub/src/machines/system.rs` and
  `crates/hub/src/machines/files.rs` (the input structs): the hub field docs differ from the
  contract's. `regex` drops "(Rust syntax, like ripgrep's)", `force` drops "On Windows a process
  is always ended at once", `pid` has no doc, ranges read "1 to 1000" against "1..=1000", and
  paths say "on that machine". -> Copy the contract's docs verbatim.
- `crates/hub/src/mcp/schema.rs` (`StartRouted`, `GetRouted`, `WaitRouted`, `ReadRouted`) and
  `crates/hub/src/mcp/callers/mod.rs` (`ListRouted`, `InputRouted`, `ResizeRouted`): the eight
  `operation_*` tools take `{broker, input: {...}}`, while the other hub machine tools take a flat
  `machine`; none has `deny_unknown_fields`, so an unknown top-level argument is accepted.
  `Machine.name`'s doc says "The name to pass as `broker`". -> Flat closed structs such as
  `struct OperationGetInput { machine: String, operation_id: String }`, the contract input built
  in the body, the doc changed to `machine`, and the `{broker, input}` calls in
  `tests/hub_probe.py` updated. Note the schema change in the parity doc and the changelog (C17).

### Rule 16: the same annotations

- `crates/hub/src/machines/files.rs` (`write_file`): `idempotent_hint = true`; the local
  `write_file` says `false`. With `append` the same arguments add content again. -> `false`.
- `crates/mcp/src/adapter/system_tools.rs` and `crates/hub/src/machines/system.rs`
  (`read_url`): `destructive_hint = false` and `idempotent_hint = true` on a read-only tool. ->
  `annotations(title = "...", read_only_hint = true, open_world_hint = true)` on both.
- `crates/mcp/src/adapter/system_tools.rs` and `crates/hub/src/machines/system.rs`
  (`set_config_value`): `destructive_hint = false`, but lowering `max_operations` retires kept
  jobs (C3). -> `destructive_hint = true` on both.

### Rule 17: one composite implementation

- `crates/mcp/src/adapter/file_tools.rs` and `crates/hub/src/machines/files.rs`
  (`read_multiple_files`): each tool restates `max_bytes_each.unwrap_or(MAX_FILE_READ_BYTES)`.
  -> Move the default into `read_many` in `crates/mcp/src/files.rs`, which both call.

### Rule 18 and where files go

- `crates/hub/src/mcp/tools/operations.rs` and `crates/hub/src/mcp/callers/mod.rs`: the
  `operation_*` tools live in `mcp/` and are added in `Hub::router()`, not through
  `Hub::machine_router()`, while the *Serves* section of `crates/hub/src/machines/mod.rs` says
  machines serves them. `machines`, `machine_add` and `machine_update` sit in
  `crates/hub/src/mcp/tools/machines.rs`, with their helpers in `crates/hub/src/mcp/machines.rs`.
  -> C5: `machines/operations.rs` with
  `operations_router`, merged in `machine_router()`; `machines/manage.rs` for the three
  machine-management tools. *Serves* becomes true by moving the code.
- `crates/machine-engine/src/files/search.rs` (inline `mod tests`): the file is 463 lines, 164
  of them tests, near the 500-line cap (C10 keeps it). -> A new
  `crates/machine-engine/src/files/search/tests.rs`, in S10's machine-engine PR.
- `crates/machine-engine/src/broker/system.rs` (inline `mod tests`): a `<domain>.rs` beside
  `broker/system/` (C10 keeps it). -> A new `crates/machine-engine/src/broker/system/mod.rs` with `system/tests.rs`.
- `AGENTS.md` (Implementation, line 39): says "Contracts belong in `crates/contracts/src/lib.rs`",
  but requests live in family modules. -> Plan B0's sentence: "Contracts belong in a family
  module of `crates/contracts/src/`, re-exported from lib.rs; a new machine capability follows
  `.ai/patterns/machine-capability.md`. Regenerate artifacts, never edit them."

### The canonical test

- `tests/mcp_protocol.py` (`TOOL_SCHEMAS`, `validate_catalog`): 19 entries against 27 served
  tools. Missing: `search_files`, `get_config`, `set_config_value`, `list_processes`,
  `kill_process`, `read_url`, `get_recent_tool_calls`, `get_usage_stats`; `list_directory` lacks
  `cursor`. The catalog check needs an exact match, so the probe fails against today's adapter,
  and neither CI nor `tests/e2e.py` runs it. -> Add the eight entries and `cursor`, and run it
  from `tests/e2e.py` (C16).
- `tests/adapter_probe.py` (`missing` and the `--account` section): never calls `search_files`,
  `get_config`, `set_config_value`, `list_processes`, `kill_process`, `read_url` or
  `operation_cancel`; `operation_resize` is in `missing` but never called. -> Call each in the
  `--account` section, as `tests/hub_probe.py` does, and add each name to `missing`.
- `tests/adapter_probe.py` (the `operation_get` loop after `operation_start`): polls every 0.1 s
  for up to 30 s, which the adapter's own instructions forbid. -> One `operation_wait` with
  `timeout_ms: 30000`, then one `operation_get` asserting the same terminal state.
- `tests/hub_probe.py` (the `run_broker` section): `read_image` is called on a 1x1 PNG written
  with `write_file` `bytes_base64`, and its `image/png` block and its `not_an_image` refusal are
  checked, but nothing checks the audit line the hub's hand-routed `read_image` composite writes.
  -> Check that `get_recent_tool_calls` with `tool: "read_image"` shows both calls, the refusal
  with its code.
- `crates/hub/src/machines/tests/mod.rs`: no `parity` module; only `machine_inspect` has a hub
  Rust test. -> Add `parity.rs` (The mechanical check, step 1).
- `crates/machine-engine/src/broker/tests/`: no `processes.rs`. `process_list` and
  `process_kill` on an account broker have no Rust test, so `operation_process`,
  `broker_process` and `process_not_found` are unchecked. -> A new `crates/machine-engine/src/broker/tests/processes.rs` on
  `account_broker`: list a spawned child with a filter, kill it (signal `terminate` on Unix,
  `kill` on Windows), and refuse a job's root, the broker's own PID and an unused PID.
- `crates/machine-engine/src/broker/system/web.rs` (`read`): no Rust test. -> A new `crates/machine-engine/src/broker/system/web/tests.rs`
  against a local server: a page and `next_offset`, a server that ignores Range, 416 at the end,
  `file:` refused `invalid_request`, and `max_bytes` 0 refused.

[G]: README.md#shared-rules-stated-once-apply-everywhere
