# Patterns

> ALPHA = PRE-RELEASE. Read [../README.md](../README.md) first. These patterns describe how code
> is written now. They are not release gates. Alpha never excuses broken authorization, tenant
> boundaries, secret handling, hidden failures, or claiming something works when it was not run.

A pattern is the team's single answer to "how do we write this kind of thing here". Each one
names an exemplar in the code, its rules (each rule testable), where files go, the canonical
test, its anti-patterns, and the mechanical check that holds it. Upstream practice is cited
where the repo follows it, and where the repo departs from it the pattern says why.

## Read them in this order

| # | Pattern | One line |
|---|---|---|
| 1 | [layout.md](layout.md) | Where Rust code lives: a crate per boundary, a folder per domain, front doors, the 500-line ratchet, table prefixes. |
| 2 | The shared rules below | What holds in every pattern. Each pattern states only what is particular to it. |
| 3 | [mcp-tools.md](mcp-tools.md) | Every MCP tool on both surfaces: names, titles, hints, closed inputs, `Json<T>` answers, coded refusals, one audit line, one description. |
| 4 | [machine-capability.md](machine-capability.md) | One thing an AI can do on a machine, end to end: contract, broker, local tool, hub tool, probes. Exemplar `broker_capabilities` / `machine_inspect`. |
| 5 | [link.md](link.md) | The machine-to-hub WebSocket: frames in contracts, additive protocol 1, gated hub frames, bounded loops, join templates. |
| 6 | [board-operations.md](board-operations.md) | An AgentHub operation: one `HostedBoard` transaction, one chain event, one refusal code, one call from every surface. Exemplar `HostedBoard::handoff` (`crates/agenthub/src/hosted/operations.rs`). |
| 7 | [persistence.md](persistence.md) | Postgres through `devagotchi-store`: migrations, prefixes, forced RLS, `transaction!`, `scope`, store fixtures. |
| 8 | [identity-and-config.md](identity-and-config.md) | Settings (file vs `/admin`), sign-in providers as data, cookies, sessions, origin, the hub's OAuth server. |
| 9 | [hub-pages.md](hub-pages.md) | An axum page and its form: owned routes, one guard per caller, POST-303-GET, one render path, the site's words, the phone frame. |
| 10 | [connectors.md](connectors.md) | An AI client is a catalog entry, not code. |
| 11 | [tests-and-ci.md](tests-and-ci.md) | A test proves one claim or fails; it never passes without running. What CI runs, and on what cadence. |

Product order, for deciding what comes first: the self-hosted Desktop Commander (patterns 3 to 5),
then AgentHub (6), then what serves both (7 to 9), then plugins and clients (10). The commercial
multi-tenant cloud is a separate later module. It never shapes these patterns, except that
tenant isolation (G8) holds from day one.

Some names below are planned and do not exist yet. Each one is marked with the batch of the
patterns rewrite plan that adds it (B0 to B6). Status is as of `main` at 2b3feec plus commit
f13b8e1 ("CI: a store test never passes without running"), which lands with this README.

## Shared rules (stated once, apply everywhere)

**G1. One authority, thin adapters.** Each decision has one owner. Every other layer forwards
and adds nothing:

| Decision | Owner |
|---|---|
| What runs or changes on a machine | the broker (`crates/machine-engine`) |
| What a board does | `HostedBoard` (`crates/agenthub/src/hosted.rs` and `crates/agenthub/src/hosted/`) |
| SQL, transactions, tenant scope | `Store` (`crates/store`) |
| Wire shapes, bounds, defaults, compared words | `crates/contracts` |
| A tool's description | `crates/contracts/src/describe.rs` |
| A client's connect steps | the connector renderer (`Connector::render`, `crates/connectors/src/lib.rs`) |
| A setting's range and precedence | the `SETTINGS` catalog (`crates/hub/src/config/live/catalog.rs`) |

A surface (local MCP, hub MCP, CLI, page, REST, hook) makes exactly one call per user intent.
Two writes for one intent means a missing operation in the owner.

**G2. Closed inputs, open answers.**
- Every struct read from outside is `#[serde(deny_unknown_fields)]`: wire inputs, MCP tool
  inputs, link frames, config sections, site JSON. Flat structs only: serde does not support
  `flatten` together with `deny_unknown_fields`
  (https://serde.rs/container-attrs.html#deny_unknown_fields).
- Answers only ever gain fields, and each added field is `Option<T>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. That keeps it off the wire for
  older peers and out of the output schema's `required`.
- A field is never renamed, removed or made required on a released wire. Released wires are the
  link frames, MCP tool schemas, outbox kinds and the hash chain.

**G3. A value two places compare is defined once.** Bounds are `pub const MAX_*`. Defaults
are `pub const DEFAULT_*`, returned by a `pub fn default_*` that serde names. Words both sides
compare are `pub const`, and rules both sides apply are `pub fn`. They live in
`crates/contracts` when they cross crates, and in the owning module otherwise. A literal that
restates one is a defect. That covers `unwrap_or(200)`, a range in a message or description,
and `"container"` written on both sides.

**G4. Refusals are coded and honest.**
- A refusal is `{code, message}` (`BridgeError` in `crates/contracts/src/lib.rs`, or
  `BoardError` with `ticket` and `next` in `crates/hub/src/agenthub/mcp/refusals.rs`). The code
  is stable snake_case. The message names what to do next.
- A store or service failure is never a "no", an empty list, "signed out" or a 404. It is 503
  `unavailable` on pages, `issuer_unavailable` on JSON, and a coded tool error over MCP.
- No success-shaped fallbacks: no `.ok()` or `unwrap_or_default()` on stored data that is shown
  or decided on. Use `decode` (`crates/agenthub/src/hosted.rs`), which refuses with
  `board_damaged`.
- An MCP tool returns refusals as a tool execution error (`isError: true` with
  `structuredContent.code`). That includes refusals made around the tools (API-key limits,
  early access) and arguments serde refuses for a known tool (`invalid_arguments`, MCP
  2025-11-25; B1 adds the code). JSON-RPC errors are only for an unknown or hidden tool and a
  malformed request.

**G5. Tables name every variant.** A `match` that classifies every request, answer, frame or
state (`handle`, `on_files`, `on_system`, `target`, `caller`, `caller_slot`, `held_for`, `kind`,
`jobs_saw`, the link and hub frame dispatches) has no `_ =>` or `Ok(_)` arm. A new variant then
fails to compile until each table decides. A decoder that expects one kind keeps its
`other =>`.

**G6. Everything is bounded, and budgets are derived.**
- Before trust: sizes, counts and handshake time.
- After trust: every socket read has a deadline and every write a timeout.
- An internal time budget is derived from the transport's deadline
  (`devagotchi_ipc::IO_TIMEOUT + held_for()`) and held by a `const` assertion beside it.
- Blocking filesystem, process or `std::fs` work runs on the blocking pool (`spawn_blocking`),
  never on an async worker.

**G7. Audit: one line, true outcome, never hidden.**
- Every hub tool call and every surface write writes exactly one audit line. It holds the
  subject, the tool, the place (machine or project, else `""`), the target, and the outcome
  `ok`, `running` or the returned error's code. Detail goes in the target.
- Write the line through `Hub::call` (`crates/hub/src/mcp/hub.rs`), `board_call` /
  `board_call_scoped` (`crates/hub/src/agenthub/mcp/calls.rs`), or `Hub::recorded` (a
  `Result` in, the line written, `Json` out; B1 adds it to `crates/hub/src/ops/audit.rs`,
  where the free `fn recorded` today is a row decoder, not this). There is no `?` or
  `return Err` before it, except on taking the subject.
- Before the effect, a failed audit write refuses the call (`audit_write`). After a committed
  effect, a failed audit write never turns the answer into a refusal: the caller gets the real
  result, and the failure is logged and raised as an operator signal (`audit.write_failed`;
  B1, operator decision D1).
- Exempt: `get_recent_tool_calls` and `get_usage_stats`, which read the audit.

**G8. Who and which tenant come from authentication, never from a payload.**
- Identity comes from the token or session (`set_caller`, `Hub::subject`, `actor_for`). The
  broker's `caller` field is `#[schemars(skip)]`.
- A tenant's rows sit behind forced RLS, and every statement still names `tenant_id = $n`.
- A secret is a `*_env` name or a 0600 `*_file`, never a value in a file, an argument, argv, a
  log, an audit line or a chain event.
- The process environment is read at entry points only and passed in as
  `&dyn Fn(&str) -> Option<String>`.

**G9. Time and randomness.**
- A decision uses one clock. AgentHub decides on the database clock (`let now =
  self.scope(connection).await?` inside the transaction). The hub's own stores take `now` once,
  before `transaction!`.
- A table's times never mix the two clocks.
- Ids and random bytes are taken before a transaction, because its body may run again.
- Nothing outside the store (HTTP, push, wake, mail, cache, fence memory) runs inside
  `transaction!`.
- Tests never sleep for a condition: they poll with a deadline, inject the duration, or move the
  stored time.

**G10. Layout ([layout.md](layout.md)), with two clarifications every pattern follows.**
- *Tests.* A domain's tests are `<domain>/tests.rs` or `<domain>/tests/<subject>.rs`. A leaf
  file (not a front-door `mod.rs`, not `lib.rs`/`main.rs`) may keep an inline
  `#[cfg(test)] mod tests { .. }`, the Rust book's unit-test idiom
  (https://doc.rust-lang.org/book/ch11-03-test-organization.html), while file plus tests stay
  under 500 lines. A front door, or a file that would cross the cap, moves its tests out. No
  `<x>_tests.rs` beside code.
- *Contracts.* `crates/contracts` is the one crate whose `lib.rs` may re-export its family
  modules flat (`devagotchi_contracts::FileSearchInput`, from `search.rs`). The wire's public
  paths are its API, and the exporter and every crate import them. This is the only exception
  layout rule 5 allows. Other `lib.rs` files that re-export today (AgentHub's, `crates/store`,
  `crates/ipc`, `crates/mcp`) are not covered by it.
- A domain's tools, pages and SQL live in that domain. Front doors carry Owns / Serves /
  Settings / Depends on.

**G11. Docs and comments state what is true.** Paths in docs, comments and front doors name
files that exist. A comment that names a guarantee ("every test that takes a store runs here")
matches the code and the YAML. An applied migration is never edited, so new migration headers
name a domain or a plan, never a file path.

**G12. Every rule has a check, and every check is a ratchet.**
- A rule is held by a test or lint, or the pattern says "held by review" and names the test that
  will replace review.
- An allow-list or baseline file only shrinks. Its `DEVAGOTCHI_WRITE_<X>` mode only lowers or
  drops entries, never adds them. CI never sets a write variable.
- A check lands green together with the fix it guards, never red on `main`.
- Workspace-wide source scans live in `crates/devagotchi/tests/` and share one walker with
  `crates/devagotchi/tests/layout.rs`. Single-crate scans live in that crate's tests and share
  the walker lifted from `crates/hub/src/site/brand_tests.rs`.

**G13. A test proves one claim or fails.**
- Its name is a falsifiable sentence.
- A store fixture is `async fn <name>() -> Option<T>`, taken with
  `let f = or_skip!(<name>());` (`crates/store/src/testing.rs`).
- `or_skip!` prints `SKIPPED: no DEVAGOTCHI_TEST_POSTGRES_URL`. In CI,
  `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1` (and `DEVAGOTCHI_TEST_REQUIRE_CLICKHOUSE=1`, which B0
  adds) turns any missing URL into a failure.
- `hub-postgres` runs `rdeps(devagotchi-store)`.
- No retries. A probe that a flag asked for fails when its input is missing.

**G14. How we work** (carried over from the previous README):
1. Evidence before assertions. Keep the chain task, base state, action, observation, patch,
   test, review, outcome. A chat's "passed" is not a test receipt.
2. Durable incremental capture. Journal during the session, not only on shutdown. Retry with
   stable identity; reject changed-content replay. Mark missing data explicitly.
3. Instruction is not enforcement. Instructions guide models; admitted hooks run commands; local
   gates cooperate with agents; remote branch protection enforces merges.
4. No secret-by-default telemetry. Metadata is allow-listed; content needs separate informed
   consent. Recording a chat never makes it eligible for training on its own.
5. No hidden reasoning extraction. Store authorized visible messages, concise decision rationale
   and observable tool evidence, not private internal reasoning.
6. Use existing [architecture decisions](../../docs/product-and-platform-plan.md), not new
   abstractions, whenever the existing owner already fits.

## The mechanical checks, in one place

"Planned" means the check does not exist yet; until it lands, the rule it holds is held by
review.

| Check | Holds | Lives in | Status |
|---|---|---|---|
| Size ratchet | layout 6 | `crates/devagotchi/tests/layout.rs` | in place |
| One home per domain | layout 2 | `crates/devagotchi/tests/layout.rs` | planned; held by review |
| Test shape: names, skip shapes, scripts run by CI, workflow pins and timeouts | G13, tests-and-ci | `crates/devagotchi/tests/test_shape.rs` | planned (B6) |
| Link wire scan: dispatch wildcards, compared literals, personal paths | G5, G3, link 11 | `crates/devagotchi/tests/link_protocol.rs` | planned (B2) |
| SQL scan: qualified tables, `AssertSqlSafe`, `QueryBuilder`, `tenant_id` ratchet | persistence 12-18 | `crates/devagotchi/tests/sql.rs` | planned (B6) |
| Link wire golden (append-only) | G2 | `crates/contracts/src/link/tests.rs` | planned (B2); today's frame tests are `crates/contracts/src/link_frame_tests.rs` |
| `describe::BROKER_TOOLS` covers `REQUEST_METHODS` | machine-capability 7 | `crates/contracts/src/system/tests.rs` | planned (B1 adds `BROKER_TOOLS` and the test); the file exists |
| Every tool's form, through shared `check_tools` | mcp-tools N1, T1, A1, A2, I1, I2, O1, D1, D3 | `crates/mcp/src/pattern.rs`, called by `crates/mcp/src/adapter/tests.rs` and `crates/hub/src/mcp/tests/tools.rs` | planned (B1) |
| Hub audit and uniqueness scan | G7, N1 | `crates/hub/src/mcp/tests/scan.rs` | planned (B1) |
| Local/hub parity | G1, G3, machine-capability 8, 14-16 | `crates/hub/src/machines/tests/parity.rs` | planned (B1) |
| Board contract: viewer tools match hints; every code has a `next` | board-operations 9, 11 | `crates/hub/src/agenthub/mcp/tests/contract.rs` | planned (B3) |
| Board actions and no-op silence | board-operations 6, 7 | `crates/agenthub/src/tests/actions.rs` | planned (B3) |
| RLS, grants and lookup-function locks from the catalog | G8, persistence 8-10 | `crates/hub/src/store/tests.rs` | planned (B6); replaces `TENANT_TABLES` in `crates/hub/src/identity/accounts/tenants/tests.rs` |
| Table prefixes | layout 8 | `crates/hub/src/store/tests.rs` (`every_table_in_the_hub_schema_carries_its_domain_prefix`) | in place |
| Migration names and checksums ratchet | persistence 2, 3 | `crates/hub/src/store/tests.rs`, `crates/agenthub/src/migrations/tests.rs` | planned (B6); both files exist |
| Every POST refuses another site; one origin helper | G4, hub-pages 2 | `crates/hub/src/web/tests/forms.rs` | planned (B5) |
| Cookies, provider names, env reads | G8, identity P3/K1/C1 | `crates/hub/src/identity/tests/conventions.rs` | planned (B5) |
| Copy ratchet | hub-pages 7 | `crates/hub/src/web/tests/copy.rs` | planned (B5) |
| Generated contracts current (`node scripts/generate-contracts.cjs --check`); `devagotchi ai check` | machine-capability 9 | a new `static` job in `.github/workflows/ci.yml` (no CI job runs either today) | planned (B1) |
| Store tests cannot skip in CI | G13 | `.github/workflows/ci.yml` `hub-postgres`, `crates/store/src/testing.rs` (`server_url`) | in place for Postgres (f13b8e1); ClickHouse flag planned (B0) |

## Upstream ground

| Subject | Sources |
|---|---|
| MCP | Tools spec (tool execution errors for input validation): https://modelcontextprotocol.io/specification/2025-11-25/server/tools. Annotations (same meaning in 2025-06-18): https://modelcontextprotocol.io/specification/2025-06-18/schema#toolannotations |
| rmcp 3.9.0 | https://github.com/modelcontextprotocol/rust-sdk (`ToolRouter` `+` calls `merge`, and a later route with the same name replaces the earlier one: `crates/rmcp/src/handler/server/router/tool.rs`) |
| serde, schemars | https://serde.rs/container-attrs.html, https://graham.cool/schemars/deriving/attributes/ |
| sqlx | `migrate!`, checksums, `#[sqlx::test]`: https://docs.rs/sqlx |
| axum | `Router::merge`, real-server tests: https://github.com/tokio-rs/axum/tree/main/examples |
| tokio | `spawn_blocking`, `time::pause`: https://docs.rs/tokio |
| OWASP | CSRF Prevention and Session Management cheat sheets: https://cheatsheetseries.owasp.org |
| OAuth and OIDC | RFC 9700 (OAuth security BCP): https://www.rfc-editor.org/rfc/rfc9700. OpenID Connect Core §3.1.3.7 (ID Token validation): https://openid.net/specs/openid-connect-core-1_0.html#IDTokenValidation |
| nextest | profiles, junit, `rdeps()` filtersets: https://nexte.st |
| GitHub Actions | security hardening (SHA pins, least privilege, untrusted input via `env:`): https://docs.github.com/actions/security-for-github-actions |
| Anthropic | Writing effective tools for agents: https://www.anthropic.com/engineering/writing-tools-for-agents |
