# Pattern: tests and CI

> ALPHA = PRE-RELEASE. Read [../README.md](../README.md) and [README.md](README.md) first.
> This pattern describes how tests are written and what CI runs now. It is not a release gate.

A test proves one claim or fails. It never passes without running. This file says how
Devagotchi is tested, what each CI workflow runs, and on what cadence. Follow it for every new
test, fixture, probe and workflow step.

The shared rules in [README.md](README.md) apply and are not repeated here. This pattern builds
most on:

- [G9](README.md) (time: tests never sleep for a condition),
- [G10](README.md) (where tests live, and inline `mod tests` in a leaf file),
- [G11](README.md) (comments and docs state what is true, YAML included),
- [G12](README.md) (every rule has a check; baselines only shrink; CI never sets a write
  variable),
- [G13](README.md) (sentence names, the store fixture shape, `or_skip!`, the REQUIRE flags,
  `rdeps(devagotchi-store)`, no retries).

Product context: one person self-hosts a hub on Linux, joins Windows and Linux machines,
approves on an Android phone, and lets AI clients work through MCP. Each layer below exists
because one of those can break.

## Layers and where they run

| Layer | What it proves | Where it lives | Runs in |
|---|---|---|---|
| static | deps and licences, layout ratchet, generated schemas, owner grep, shellcheck | `crates/devagotchi/tests/layout.rs`, `security.yml`, `ci.yml` `repository-owner`, the shellcheck steps in `hub-deploy` and `host-install` | every PR: the layout tests and the owner grep; `security.yml` when the PR changes `Cargo.lock`, a `Cargo.toml` or `deny.toml`. Main and weekly: all of it |
| unit | pure logic, parsers, renderers, OS backends | `src/<domain>/tests.rs` (G10) | every PR: `core` on ubuntu-24.04 and windows-2025. Main: all six OS legs |
| store | everything that takes a store, on real Postgres (and ClickHouse) | `src/<domain>/tests.rs` with `or_skip!` | every PR, `hub-postgres` |
| binary e2e | the one binary as processes: broker, `mcp`, hub, link, update, install | `crates/devagotchi/tests/*.rs`, `tests/e2e.py` and its probes, `tests/file_mutation_concurrency.py`, `deploy/linux/ci-install-check.sh`, `deploy/windows/ci-install-check.ps1` | every PR: `e2e (ubuntu-24.04)`. Main: `e2e` on Linux x64 and arm64, Windows 2022, 2025 and arm64, and `host-install` |
| browser | the hub's pages, passkeys, phone emulation, real AI clients over OAuth | `tests/*_browser.py`, `tests/clients_oauth.py` | every PR, `e2e (ubuntu-24.04)` |
| deploy | compose stack, install, update and backup scripts | `deploy/compose/check.sh`, `tests/deploy_scripts.sh` | main: `hub-compose`, `hub-deploy` |
| release artifacts | dist archives install from every shell; the image joins a machine | `install-check.yml` (`tests/install_check.py`), `image.yml` (`tests/compose_join.py`) | tags, through `release.yml`; `host` runs only when install-check succeeded or was skipped. Main runs only `plan` at 2b3feec (see Known violations, Cadence) |
| live | the deployed test hub, seen from outside | `tests/live_check.py` | hourly (`live.yml`) |
| hardware | a real Android phone, the operator's machines | plan receipts | by hand, before alpha is confirmed |

Until alpha (operator, 2026-11-24), a pull request runs the smaller set above and every push to
main runs all of it (`ci.yml`, `security.yml`). A red main is fixed on main.

Known gaps, recorded here rather than hidden:

- There is no macOS broker provider (`ci.yml` `core` matrix comment), so there is no macOS e2e.
- No real Android device runs in CI. The browser layer is emulation.
- No job runs `cargo fmt` or clippy yet. See Known violations.

**Cadence.**

- Pull request: static through deploy. A required workflow is never path-filtered, because a
  workflow skipped by a path filter leaves its required checks pending.
- Main: the same. `release.yml` also runs on main, but at 2b3feec only its `plan` job runs: the
  builds, install-check and the image are skipped (see Known violations, Cadence).
- Tag: plus the `release.yml` builds, install-check, the image and `host`. `host` runs only when
  install-check succeeded or was skipped (`release.yml:285`).
- Schedule: cargo deny weekly, live hourly.

## Exemplar

Rust test against the store:

- Fixture: `crates/agenthub/src/tests/mod.rs`.
  - The module doc (:1-4) names `DEVAGOTCHI_TEST_POSTGRES_URL` and says each test skips
    without it. `pub(crate) use devagotchi_store::or_skip;` is :6.
  - `fixture() -> Option<Fixture>` (:22-29) documents the runtime flavor its calls need.
  - `postgres()` (:110-113) is the one-line wrapper over `testing::postgres_with`.
- Time: `Fixture::advance` (:32-82) moves the stored lease and standing-authorization times
  instead of sleeping, because the board decides on Postgres's clock (G9).
- Test: `expired_lease_goes_back_to_ready_and_the_old_fence_is_rejected`
  (`crates/agenthub/src/tests/lanes.rs:213-244`). The fixture is its first statement, its name
  is a falsifiable sentence, every fallible call is labelled with `.expect("..")` or
  `.expect_err("..")`, it moves time with `f.advance(60_001)`, and the one non-obvious step
  carries a comment.
- The same folder still has the old `stores()` loop helper (`mod.rs:115-118`). The exemplar is
  the `fixture()` and `advance` shape, not the whole folder.

Fixture machinery: `crates/store/src/testing.rs`.

- `postgres_with` (:17-26) and `empty_postgres` (:28-52): a new database per test, named
  `dvg_test_<secs>_<hex>`.
- `sweep` (:67-83): databases older than `STALE_SECS` (2 h) are dropped once per process.
- `assert_line_endings` (:107-120): every migration is LF.
- `or_skip!` (:122-133).
- This is the contract of `#[sqlx::test]`, built by hand because the store opens through
  `Store::open` with role and RLS logic and several migrators.

Other shapes to copy:

- Table-driven unit test with the input as the message:
  `a_stores_place_never_shows_its_user_password_or_query` (`crates/store/src/tests.rs:3-23`).
- An environment skip that cannot happen in CI: `crates/ipc/src/windows/security.rs:488-500`
  asserts `GITHUB_ACTIONS` is unset, then prints why it skips. Its sibling
  `a_shell_script_runs_in_git_bash` (`crates/machine-engine/src/execution/platform/windows/tests.rs:50-53`)
  says "fails here, never skips".
- Bounded wait: `output_holding` (`crates/machine-engine/src/broker/tests/interactive.rs:113-125`)
  waits on a 60 s deadline and puts the last output in the panic. The Python equivalents are
  `settled` (`tests/claim_browser.py:86-95`) and `poll` (`tests/app_browser.py:107-117`).
- Injected durations: `links.offline_after` (`crates/hub/src/machines/link/tests.rs:89`) and
  `nomen.retry` (`crates/hub/src/identity/nomen/tests/mod.rs:268`).
- Real-server HTTP test: `world` and `world_with` (`crates/hub/src/web/tests/mod.rs:142-256`)
  bind `127.0.0.1:0`, run `axum::serve` for the fake Nomen and for the hub, and drive them with
  reqwest. This is axum's `the_real_deal` shape.
- A ratchet check: `crates/devagotchi/tests/layout.rs`. Its baseline only shrinks, with
  `DEVAGOTCHI_WRITE_LAYOUT` to lower entries (:163), and its own logic is unit-tested on
  synthetic input (:183-261).

Python: `tests/e2e.py`.

- `run_probe` (:125-129) is labelled, prints `--- <label>: exit N` and has a timeout.
- :330-346 fold every step into one `ok`, print `E2E PASSED` or `E2E FAILED`, and exit nonzero
  on failure.
- `tests/claim_browser.py` `main` (:98-110): `--devagotchi` takes an absolute binary; a missing
  Playwright prints `NOT RUN` and exits 2 unless `--allow-skip`.

CI:

- `.github/workflows/ci.yml` `hub-postgres` (:69-99): actions pinned to a SHA with a version
  comment, `persist-credentials: false`, `timeout-minutes`, throwaway services from repo
  scripts, and the env scoped to the step that needs it. Its header comment is the one false
  part (see Known violations).
- `.github/workflows/notify.yml`: `permissions: {}` (:24), and pull-request titles and branch
  names reach the script only through the step's `env:` (:35-46).

## Rules

Each rule states only what G9 to G13 do not. Rule numbers are stable; other patterns and the
rewrite plan cite them.

### Rust

1. **A test's name is the claim it proves** (G13). It is snake_case with a subject and a verb,
   at least three `_`-separated words (`ids_are_unique` is fine). It names no storage engine:
   the hub has one, Postgres, so `_on_postgres`, `_on_both_engines` and `_on_sqlite` say
   nothing. The `test_` prefix is not banned.
2. **It lives with its domain** (G10).
   - Unit and store tests go where G10 says, including an inline `#[cfg(test)] mod tests` in a
     leaf file under the cap.
   - Tests that drive the built binary go in `crates/devagotchi/tests/` and start it with
     `env!("CARGO_BIN_EXE_devagotchi")` (`crates/devagotchi/tests/one_binary.rs:8-10`).
   - The hub's `web/*_tests.rs` files move once, with S8 (C15), not before.
3. **One store fixture shape** (G13 states it).
   - Each fixture's doc says what it builds and that it is `None` without
     `DEVAGOTCHI_TEST_POSTGRES_URL`.
   - A sync test takes its fixture with `or_skip_blocking!(runtime, fixture())` from
     `store::testing`.
   - Never: a `for .. in stores().await` loop over a 0-or-1 `Vec`, `if let Some(..) =
     postgres().await`, a hand-written `let .. else { return }`, a private copy of the macro,
     or a helper whose `Option<&str>` store argument means "make one".
4. **One database per test.**
   - `empty_postgres` makes `dvg_test_<secs>_<hex>` and sweeps databases older than 2 h.
   - A test never drops its own database (a store's pool outlives the test).
   - A process test gives each hub its own database. A database holds one hub (`ci.yml` `e2e`
     env comment, migration 0007).
5. **A skip is impossible where the condition is guaranteed, and visible where it is not.**
   - Every reader of the test services goes through `store::testing`: `empty_postgres`,
     `server_url()` and `clickhouse()`. Each panics on a missing URL under its REQUIRE flag
     (G13). No test reads `DEVAGOTCHI_TEST_*` itself.
   - Every skip prints the one text, `SKIPPED: no DEVAGOTCHI_TEST_POSTGRES_URL` (C13), from a
     `testing` constant.
   - nextest hides a passing test's output by default, so the flag is the guarantee and the
     print is a courtesy that junit keeps (rule 20).
   - Any other environment skip follows `crates/ipc/src/windows/security.rs:488-500`: it
     asserts it is not CI, and it says why it skips. AGENTS.md forbids hidden test skips.
6. **Every store-backed test runs with Postgres in CI** (G13: `hub-postgres` runs
   `rdeps(devagotchi-store)`). That filter is exactly the crates that enable the store's
   `testing` feature: store, agenthub, hub and devagotchi. No hand-kept package list.
7. **No fixed sleep stands in for a condition** (G9). In particular:
   - Ordering: wait for a signal that the other side is parked.
   - Proving that nothing happens: wait a window derived from the injected duration, or for a
     signal that the attempt ran, never a bare number.
   - Expiry: inject the duration or mint the time already past.
   - `tokio::time::pause` only in a `current_thread` test of tokio timers.
8. **No retries** (G13). `retries = 0` in every nextest profile (`.config/nextest.toml`). A
   test that fails under load is a bug: record the evidence in the plan, find the cause, then
   fix it. Only after the cause is known may a nextest test-group with `max-threads` serialize
   it.
9. **A failure says which case failed.** `assert_eq!` already prints both sides. A loop or table
   case carries its input as the message (`crates/store/src/tests.rs:20`). A boolean `assert!`
   carries the value it judged (`"{view:?}"`).
10. **A generated artefact is checked by a test that can rewrite it** (`DEVAGOTCHI_WRITE_LAYOUT`,
    `DEVAGOTCHI_WRITE_SCHEMA`, `DEVAGOTCHI_WRITE_SIGNAL_SCHEMA`), and by a CI job that runs the
    check (G12).

### Python (tests/)

11. **One script is one scenario, and `tests/e2e.py` is the one scenario runner.**
    - A script takes `--devagotchi <absolute path>` (or `--bin-dir`) and makes its own temp dir
      and free port.
    - It reads its database from `DEVAGOTCHI_HUB_STORAGE_URL`.
    - It prints `--- <step>: exit N` and exits nonzero if any step failed.
    - e2e.py runs probes through `run_probe` and folds every result into one `ok`. A probe that
      should run on every OS is added there, not as an extra ci.yml step (C16).
12. **A missing tool is NOT RUN, with exit 2.**
    - `--allow-skip` is for local use only. No workflow passes it.
    - A part a flag asked for fails when its input is missing (G13). It never prints "skipped"
      and counts as a pass.
    - A part skipped by platform or by a flag the job chose (`--no-hub`, Windows) is named in
      the output and in the job's step name.
    - A `unittest` run that skipped cases is not a success unless `--allow-skip` is given.
13. **Wait on a condition with a deadline.** A polling loop ends with the last observed state in
    the failure (`settled`). Prefer Playwright locator `expect()` where it is verified to work
    under the hub's CSP. The hand-written poll stays where the CSP blocks Playwright's polling
    (`tests/app_browser.py:107-111`).
14. **Shared helpers live in a helper module, never in a runnable script.**
    - `tests/fake_nomen.py` is the precedent.
    - `free_port`, `serve`, `wait_ready`, `stop`, `settled`, `call`, `Person`, `start`,
      `Connector`, `Callback`, `downloads` and `ROOT` move to one module, `tests/harness.py`.
    - A runnable script imports only helper modules.
15. **Every script and every stated gate is run by CI, or is listed as manual.**
    - A file under `tests/` is run by a workflow, by a script a workflow runs, imported by one
      as a helper module, or listed in `tests/MANUAL.md` with why and how.
    - The same holds for every pre-push gate in `.ai/dev/workflow.md`.

### Workflows (.github/workflows)

16. **Least privilege, pinned, no credential left behind.**
    - The top-level `permissions` is `contents: read` or `{}`, with wider grants per job.
    - Every third-party `uses:` is pinned to a 40-hex SHA with a version comment.
    - `persist-credentials: false` on every checkout.
    - Untrusted text reaches a script only through `env:` (`notify.yml`).
17. **Every job has `timeout-minutes`.** A reusable-workflow job (`uses: ./...`) inherits its
    jobs' timeouts.
18. **One step per independent check.** The default shell is `bash -e`, so the first failure in
    a multi-script step hides every check after it.
19. **Throwaway services start from repo scripts** (`tests/throwaway_postgres.sh`,
    `tests/throwaway_clickhouse.sh`). Their URLs are set only on the steps that need them.
20. **Results are kept.** Every nextest step uploads `target/nextest/ci/junit.xml` with
    `if: always()`, and `[profile.ci.junit]` sets `store-success-output = true`, so skips and
    flakes leave a record.

A workflow comment that names a trigger, a gate or a guarantee is held by G11, not by a rule
here.

## Where files go

| What | Where |
|---|---|
| Unit and store tests | G10: `src/<domain>/tests.rs`, `src/<domain>/tests/<subject>.rs`, or inline in a leaf file under the cap |
| Store fixtures and skip machinery | `crates/store/src/testing.rs` (feature `testing`); a domain's own fixtures in its `tests` module |
| Tests that drive the built binary | `crates/devagotchi/tests/<subject>.rs` |
| Workspace-wide source scans | `crates/devagotchi/tests/`, sharing `layout.rs`'s walker (G12) |
| Their baselines | `crates/devagotchi/tests/<check>/` (as `crates/devagotchi/tests/layout/`) |
| Scenario scripts and probes | `tests/<scenario>.py`, run from `tests/e2e.py` or a named ci.yml step |
| Shared Python helpers | a non-runnable module in `tests/` (`tests/fake_nomen.py` today; `tests/harness.py` once rule 14 lands) |
| Manual-only scripts | listed in `tests/MANUAL.md` (to be created, rule 15) |
| Throwaway services | `tests/throwaway_*.sh` |
| nextest profiles | `.config/nextest.toml` |
| Workflows | `.github/workflows/`; `release.yml` is generated by dist, with hand edits kept under `allow-dirty` (`dist-workspace.toml`) |

## The canonical test

From `crates/agenthub/src/tests`, as it is at main:

```rust
// tests/mod.rs
//! ... Every
//! board here is a standalone board (`project.rs`) in a database of its own on the test
//! Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`); without one, each test skips (`or_skip!`).

pub(crate) use devagotchi_store::or_skip;

/// A standalone board, `devagotchi`, made in a new database the way `board init` makes one,
/// or `None` without a test Postgres. Its synchronous calls want a multi-thread runtime:
/// `#[tokio::test(flavor = "multi_thread")]`.
pub(crate) async fn fixture() -> Option<Fixture> {
    let test = postgres().await?;
    let board = crate::open_board(&test.url, PROJECT, true).await.unwrap();
    Some(Fixture { board, test })
}

// tests/lanes.rs:213-244
#[tokio::test(flavor = "multi_thread")]
async fn expired_lease_goes_back_to_ready_and_the_old_fence_is_rejected() {
    let f = or_skip!(fixture());
    let jesse = Actor::human("jesse");
    let claude = Actor::agent("claude");
    let codex = Actor::agent("codex");
    let id = f.new_ticket(&jesse, "something slow");
    let lease = f.board.claim(&claude, &id, Some(60_000)).expect("claim");
    assert_eq!(lease.ticket.lease_ttl_ms, Some(60_000));

    f.advance(60_001);
    let lapsed = f.board.get(&id).expect("get").ticket;
    assert!(lapsed.lease_expired, "lapsed but not yet reaped");

    // Codex claims it; the expiry happens lazily inside the claim and the fence moves on.
    let taken = f
        .board
        .claim(&codex, &id, None)
        .expect("claim after expiry");
    assert_eq!(taken.fence, 2);
    assert_eq!(taken.ticket.expiry_count, 1);
    let late = f
        .board
        .heartbeat(&claude, &id, lease.fence, None)
        .expect_err("the woken-up old holder is rejected");
    assert_eq!(late.code, "stale_fence");
    let late_complete = f
        .board
        .complete(&claude, &id, lease.fence, "done?")
        .expect_err("same for complete");
    assert_eq!(late_complete.code, "stale_fence");
}
```

`f.advance(60_001)` moves the stored lease time back by 60,001 ms instead of sleeping, because
the board reads Postgres's clock.

## Anti-patterns

- `for store in stores().await { .. }`: zero iterations is a green test.
- `let Some(url) = .. else { return };` and `if let Some(test) = .. { .. }` in a test body.
- `tokio::time::sleep(Duration::from_secs(3))` to outlive a token, and `sleep(300ms)` to hope a
  waiter is parked.
- `fn registration_refusals()` (a noun phrase), and `keys_hold_on_postgres` (an engine name).
- One `run: |` holding ten browser scripts.
- A probe that prints "skipped" and sets its `_ok = True` because an env var is missing.
- One test script importing `free_port` from another.
- `retries = 2` to make CI green.

## Not rules (tracked plan items)

- Building the same binary in `core`, `e2e` and `host-install`.
- Building the hub image in both `hub-compose` and `hub-deploy`.

These are CI cost, not correctness. They follow `docs/implementation-and-release-plan.md`
(the CI optimisation list after the baseline commands): measure first, and keep the separate
ci-a and ci-b builds.

## The mechanical check

None of these exists at main 2b3feec. Until each lands, its rules are held by review (G12), and
the check named here replaces review. Each lands green together with the fix it guards.

1. **The REQUIRE guard in `store::testing`** (B0). It holds rules 5 and 6 for every skip shape
   at once, because every shape reaches `empty_postgres`, `server_url()` or `clickhouse()`.
   - `pub const REQUIRE_ENV: &str = "DEVAGOTCHI_TEST_REQUIRE_POSTGRES";`, and the ClickHouse
     twin `DEVAGOTCHI_TEST_REQUIRE_CLICKHOUSE`.
   - The decision is factored into a pure function and table-tested in
     `crates/store/src/tests.rs`; spawning a test binary is not needed.
   - Proof: run hub-postgres once with `DEVAGOTCHI_TEST_POSTGRES_URL` unset. It must go red.
2. **ci.yml** (B0 and B6).
   - `hub-postgres` runs one step, `cargo nextest run --locked --profile ci -E
     'rdeps(devagotchi-store)'`, with both URLs and both REQUIRE flags (rule 6).
   - junit uploaded after every nextest step (rule 20).
   - A `static` job: rustfmt, clippy `-D warnings`, `node scripts/generate-contracts.cjs
     --check`, `devagotchi ai check` (rules 10, 15).
   - An `if: always()` aggregate job, `ci-ok`, whose `needs:` names every job and which fails
     unless each result is success. Branch protection then requires one check.
3. **`crates/devagotchi/tests/test_shape.rs`** (B6), beside `layout.rs` and in its ratchet style.
   - Baseline `crates/devagotchi/tests/test_shape/allowed.txt`, one `<rule>\t<path>:<line or
     name>` per line. It only shrinks; `DEVAGOTCHI_WRITE_TEST_SHAPE=1` drops entries that are
     gone and never adds any. Its decision functions get synthetic-input unit tests.
   - It reuses `layout.rs`'s walker (`rust_files`, which skips `target`, `node_modules` and dot
     folders).
   - `every_test_is_named_as_a_claim` (rule 1): the fn after a `#[test]` or `#[tokio::test..]`
     line fails with fewer than 3 `_`-separated words, or when it matches
     `(_on_postgres|_on_both_engines|_on_sqlite)`.
   - `a_store_backed_test_skips_only_through_or_skip` (rules 3 and 5), in test files:
     - `^\s*for .* in .*stores\(.*\)\.await` (catches tuple patterns, wrappers that take
       arguments such as `stores(&cell, &[])`, and `hosted_stores`);
     - `Vec<Option<String>> = .*empty_postgres\(` (the four `stores` copies and the two inline
       loops in `web/analytics_tests.rs` and `web/operator_tests.rs`);
     - `if let Some\(.*\) = .*(postgres|empty_postgres)\(`;
     - a `let (...) = ...` naming `postgres`, `POSTGRES_ENV` or `CLICKHOUSE` followed within 6
       lines by `else {` and `return`;
     - `env::var(_os)?\(.*(POSTGRES_ENV|"DEVAGOTCHI_TEST_)` outside `crates/store/src/testing.rs`;
     - a `macro_rules!` containing `None => return` outside `crates/store/src/testing.rs`;
     - `fn \w+\(store: Option<&str>`, and a `^\s*store: Option<&str>,` parameter line in a
       multi-line signature (`hosted_with`, `hosted_with_ingress`).
     The baseline starts with today's sites and must reach zero.
   - `every_script_under_tests_is_run_by_ci_or_listed_as_manual` (rule 15): every
     `tests/*.py` and `tests/*.sh` is named in a workflow, in a script one runs (one level:
     `tests/e2e.py`, `deploy/linux/ci-install-check.sh`, `deploy/windows/ci-install-check.ps1`),
     imported by one, or listed in `tests/MANUAL.md`.
   - `every_workflow_job_has_a_timeout_and_every_action_is_pinned` (rules 16 and 17): each job
     holds `timeout-minutes:` or a job-level `uses: ./`; each non-local `uses:` matches
     `@[0-9a-f]{40}`; the top-level `permissions:` is `contents: read` or `{}`.
   - Proof: `cargo test -p devagotchi --test test_shape` fails on today's tree for the five names
     under rule 1 and for every skip site missing from the baseline.

Final receipt for B6: junit from hub-postgres shows 0 `SKIPPED:` lines.

## Upstream sources

| Source | What it supports here |
|---|---|
| https://doc.rust-lang.org/book/ch11-03-test-organization.html | Unit tests in a `#[cfg(test)]` module, integration tests in `tests/`, shared helpers in a module that is not run. The same principle applies to Python helpers (rule 14). |
| https://github.com/rust-lang/rust/issues/68007 | Runtime ignore is still open: an early return on a missing env var reports `ok`, "misleading the user that the test was successful". Hence the REQUIRE flag (rule 5). |
| https://docs.rs/sqlx/latest/sqlx/attr.test.html | `#[sqlx::test]`: a fresh database per test and cleanup of earlier runs. `store::testing` reimplements this because the hub opens through `Store::open` (rule 4). |
| https://github.com/tokio-rs/axum/blob/main/examples/testing/src/main.rs | `oneshot` and `the_real_deal`. The hub uses only the real-server shape, because its tests prove cookies, OAuth redirects and MCP over HTTP. |
| https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/crates/rmcp/tests/test_message_protocol.rs | rmcp names tests `test_*`. Sentence names are a house convention, so the prefix is not banned (rule 1). |
| https://docs.rs/tokio/latest/tokio/time/fn.pause.html | `pause` needs `test-util` and a `current_thread` runtime and affects only tokio's `Instant` (rule 7). |
| https://nexte.st/docs/features/retries/ | A test that passes on retry counts as a success by default. Hence `retries = 0` (rule 8). |
| https://nexte.st/docs/configuration/test-groups/ | `[test-groups]` with `max-threads = 1` and a platform-scoped override serializes a subset without retries (rule 8). |
| https://nexte.st/docs/filtersets/reference/ | `rdeps(name)` selects the named crate and every crate that depends on it (rule 6). |
| https://nexte.st/docs/machine-readable/junit/ | With `--profile ci` the report is `target/nextest/ci/junit.xml`; `store-success-output` defaults to false (rule 20). |
| https://nexte.st/docs/configuration/reference/ | `success-output` defaults to `never`, so a passing test's `SKIPPED` line is invisible by default (rule 5). |
| https://playwright.dev/python/docs/test-assertions | Locator assertions retry until a timeout. Whether they work under the hub's CSP is not verified (rule 13). |
| https://docs.github.com/en/actions/reference/security/secure-use | Pin actions to a full commit SHA; pass untrusted input through an environment variable (rule 16). |
| https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax | The default `run` shell on non-Windows runners is `bash -e {0}` (rule 18). |
| https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency | A concurrency group keyed on `github.ref` with `cancel-in-progress`; `ci.yml` matches. |
| https://docs.github.com/en/enterprise-cloud@latest/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks | A job skipped by a conditional reports success; a workflow skipped by a path filter leaves required checks pending. Hence no path filters and the `ci-ok` gate. |
| https://doc.rust-lang.org/clippy/continuous_integration/github_actions.html | Clippy in CI with `-D warnings`. The toolchain profile is minimal, so the job adds the rustfmt and clippy components. |
| https://axodotdev.github.io/cargo-dist/book/reference/config.html | `[dist.github-action-commits]` (dist 0.29.0 and later; the repo is on 0.33.0) pins the actions dist writes; `allow-dirty = ["ci"]` keeps hand edits (rules 16, 17). |

## Known violations

Verified at main 2b3feec (submodule `crates/agenthub` at 5c1499c). Each entry is
`file (symbol): problem -> rewrite`.

### Rule 1 (the name is a claim)

- `crates/hub/src/identity/accounts/passkey/tests.rs` (`registration_refusals`, `assertion_refusals`, :42, :112): noun phrases. -> `a_registration_is_refused_for_each_bad_field_and_names_it` and `an_assertion_is_refused_for_each_bad_field_and_names_it`, from what each asserts.
- `crates/hub/src/ops/analytics/tests.rs` (`the_tables_are_the_hubs_own_on_both_engines_and_a_milestone_is_kept_once`, :475): names two engines; there is one. -> `the_analytics_tables_are_the_hubs_own_and_a_milestone_is_kept_once`.
- `crates/hub/src/identity/accounts/api_keys/tests.rs` (`keys_hold_on_postgres`, :256) and `crates/hub/src/identity/preferences/tests.rs` (`preferences_hold_on_postgres_under_row_level_security`, :184): engine names. -> rename each to what its `holds` helper asserts (read the helper first), for example `a_key_keeps_its_limits_and_is_refused_once_revoked`. The skip shape is under rule 3.

### Rule 3 (one store fixture shape)

- `crates/hub/src/store/mod.rs` (`testing::stores`, :86-89) and `crates/agenthub/src/tests/mod.rs` (`stores`, :115-118): turn an `Option` into a 0-or-1 `Vec` so callers can loop. -> delete both; callers write `let test = or_skip!(testing::postgres());` (AgentHub: `or_skip!(super::postgres())`).
- `crates/hub/src/ops/tests.rs` (`stores`, :20-27; loops :101, :257), `crates/hub/src/workflows/tests.rs` (`stores`, :68-75; loops :244, :397, :482, :628, :680, :795, :1005, :1214, :1497), `crates/hub/src/offers/tests.rs` (`stores`, :22-29; loops :109, :314, :486), `crates/hub/src/tenancy/tests/access.rs` (`stores`, :69-76; loops :80, :260): four identical `Vec<Option<String>>` copies; every loop runs zero times without Postgres. -> delete the four; each test opens with `let hub = or_skip!(hosted_with(..));` (see the `hosted_on` entry).
- `crates/hub/src/web/analytics_tests.rs` (`the_operator_sees_the_counts_and_the_funnel_move_and_nobody_else_sees_anything`, :50-57) and `crates/hub/src/web/operator_tests.rs` (`the_operator_lets_a_waiting_person_in_and_their_next_call_works`, :91-98): the same `Vec<Option<String>>` built inline from `empty_postgres`, then `for store in stores`. -> open with `let hub = or_skip!(..)` on the split fixture (see the `hosted_on` entry).
- `crates/hub/src/identity/accounts/tests.rs` (`stores` :6, `hosted_stores` :785), `crates/hub/src/identity/accounts/claim/tests.rs` (`stores`, :9), `crates/hub/src/identity/accounts/oauth/tests.rs` (`stores`, :14), `crates/hub/src/identity/accounts/sessions/tests.rs` (`stores`, :13), `crates/hub/src/identity/accounts/tenants/tests.rs` (`stores`, :22), `crates/hub/src/tenancy/access/tests.rs` (`stores`, :44): seven wrappers return `Vec<(TestStore, Accounts)>` (`Arc<Accounts>` in `tenancy/access`) for tests to loop over. -> each becomes `async fn accounts(..) -> Option<(TestStore, Accounts)>` (or `hosted_accounts`) built with `testing::postgres().await?`; tests write `let (_store, accounts) = or_skip!(accounts());` and lose one indent.
- Direct `for .. in testing::stores().await` loops, 16 sites: `crates/hub/src/identity/accounts/passwords/tests.rs:7`; `crates/hub/src/tenancy/operator/tests.rs:27`; `crates/hub/src/tenancy/orgs/tests.rs:29,58,146,290,345`; `crates/hub/src/ops/analytics/tests.rs:317,421,478,527`; `crates/hub/src/store/tests.rs:43,53,92,200`; `crates/hub/src/agenthub/tests/hosted.rs:16`. -> `let test = or_skip!(testing::postgres());` and un-indent the body.
- `crates/agenthub/src/tests/hosted.rs` (:17, :294) and `crates/agenthub/src/tests/hosted_lifecycle.rs` (:15, :152): `super::stores()` loops beside the exemplar fixture. -> `let test = or_skip!(super::postgres());`. This lands in the devagotchi/agenthub repo first, then the submodule moves (B3).
- `crates/hub/src/machines/link/tests.rs` (`links!`, :16-24; uses :88, :174, :196, :235, :281, :325, :337, :370, :421, and `crates/hub/src/machines/link/tests/container.rs:33`): a private copy of `or_skip!` (`None => return`). -> delete it; write `let links = or_skip!(test_links(dir.path()));` at each use.
- `crates/devagotchi/tests/hub_recovery.rs` (`hub_set_password_reads_stdin_keeps_the_owner_and_never_prints_the_password`, :19-22), `crates/hub/src/cli/tests.rs` (:258-260), `crates/hub/src/identity/cli/tests.rs` (:66-68, :187-189): sync tests hand-write `let Some(url) = runtime.block_on(empty_postgres()) else { return }`. -> add `or_skip_blocking!(runtime, fixture)` to `store::testing` and write `let url = or_skip_blocking!(runtime, empty_postgres());`.
- `crates/store/src/tests.rs` (`the_hub_role_is_taken_only_where_the_database_granted_it`, :78-80): a hand-written let-else. -> `let url = or_skip!(testing::empty_postgres());`.
- `crates/hub/src/identity/accounts/api_keys/tests.rs` (`keys_hold_on_postgres`, :257-259) and `crates/hub/src/identity/preferences/tests.rs` (`preferences_hold_on_postgres_under_row_level_security`, :185-187): `if let Some(test) = testing::postgres().await { .. }` skips silently. -> `let test = or_skip!(testing::postgres());`, then call the helper.
- `crates/hub/src/identity/accounts/tenants/tests.rs` (`stores_from_before_tenants`, :61-66): builds a 0-or-1 `Vec` through `if let Some(url) = empty_postgres()`. -> return `Option<(String, TempDir)>` with `empty_postgres().await?`, and `or_skip!` at the call site.
- `crates/hub/src/tenancy/tests/tenants.rs` (`hosted_on` :38, `hosted_with` :43, `hosted_with_ingress` :55), with the `run(None)` and `pages(None)` wrappers at `crates/hub/src/agenthub/tests/ahead.rs:146`, `crates/hub/src/agenthub/tests/notes.rs:139`, `crates/hub/src/connections/tests.rs:287`, `crates/hub/src/identity/tests/keys.rs:192`, `crates/hub/src/identity/tests/me.rs:276`, `crates/hub/src/tenancy/tests/orgs.rs:253,434`, `crates/hub/src/web/hub_settings_tests.rs:254`, and the same `store: Option<&str>` argument on `hub_on` (`crates/hub/src/ops/tests.rs:30`, `crates/hub/src/workflows/tests.rs:57`, `crates/hub/src/web/operator_tests.rs:56`) and `hub_with` (`crates/hub/src/tenancy/tests/access.rs:46`): a `store: Option<&str>` where `None` means "make one", and thin tests that hide the fixture in a helper. -> split into `hosted_with(extra) -> Option<Hosted>` (makes its own database) and `hosted_on_store(url, extra) -> Hosted` for a test that reopens one; inline each wrapper so the test opens with `let hub = or_skip!(hosted_with(..));`.

### Rule 5 (a skip is impossible where guaranteed, and visible)

Fixed in f13b8e1 (B0): `testing::server_url()` with `DEVAGOTCHI_TEST_REQUIRE_POSTGRES`, the one `SKIPPED:` text in `or_skip!`, `store/src/tests.rs` and `hub_recovery.rs` moved onto them. Still open below: the ClickHouse flag and the synchronous skips.

- `crates/hub/src/ops/warehouse/tests.rs` (`the_real_clickhouse_keeps_one_event_per_id_and_answers_a_boards_statistics`, :254-259): a tuple let-else returns silently when Postgres or ClickHouse is missing; nothing fails if hub-postgres loses the ClickHouse URL. -> `let test = or_skip!(testing::postgres()); let ch = or_skip!(testing::clickhouse());`, where `clickhouse()` reads `DEVAGOTCHI_TEST_CLICKHOUSE_URL`, `_USER` and `_PASSWORD` and panics under `DEVAGOTCHI_TEST_REQUIRE_CLICKHOUSE=1`.
- `crates/hub/src/store/tests.rs` (`without_a_url_there_is_no_store_and_the_configs_is_used_as_written`, :31-33): returns without a word when `STORAGE_URL_ENV` is set. -> say why it skips, or assert it is not CI, as `crates/ipc/src/windows/security.rs:488-500` does (rewrite plan B6).

### Rule 6 (every store-backed test runs with Postgres in CI)

- `.github/workflows/ci.yml` (`hub-postgres`, :91 and :97) and `crates/devagotchi/tests/hub_recovery.rs` (:15-23): hub-postgres runs a hand-kept package list plus `-p devagotchi --test board`, so `hub_set_password_reads_stdin_keeps_the_owner_and_never_prints_the_password` runs only in `core`, where it skips and passes. No job can fail it. -> one step, `cargo nextest run --locked --profile ci -E 'rdeps(devagotchi-store)'`, with `DEVAGOTCHI_TEST_POSTGRES_URL`, the ClickHouse URL and user, and both REQUIRE flags set to `1`.

### Rule 7 (no fixed sleep for a condition)

- `crates/hub/src/identity/nomen/tests/mod.rs` (:242, :274, :306, :426), `crates/hub/src/identity/nomen/tests/kept.rs` (:42), `crates/hub/src/web/tests/nomen_login.rs` (:296): sleeps of `RETRY_FIRST` + 100 ms, 150 ms, 3 s, 2.1 s, 3 s and 1.2 s wait out real retry, cache and token expiry; `RETRY_FIRST` is 2 s (`crates/hub/src/identity/nomen/mod.rs:69`), so about 11.5 s per run, fragile at the margins. -> extend the verifier's existing injection (`nomen.retry`, `nomen.fresh_for`, mod.rs:267-268) to its confirmation TTL and leeway, in milliseconds; mint tokens whose `exp` is already past.
- `crates/hub/src/web/passkey_tests.rs` (:1442): `sleep(5 s)` waits out the device-code poll interval (`interval` 5, :1338). -> make the interval a config field the test sets to its minimum; test `slow_down` with an interval the test controls.
- `crates/hub/src/workflows/tests.rs` (:1094, :1127, :1581): `sleep(300ms)` inside a `join!` assumes the waiter is parked first; under load the event lands first and the parked-wait path goes untested. -> a test hook (a `tokio::sync::Notify` the wait path signals once subscribed) awaited before firing, or poll the subscriber count with a deadline.
- `crates/machine-engine/src/broker/tests/lifecycle.rs` (:192): `sleep(1500ms)`, then an assertion that assumes the root has exited. -> poll with a deadline until the root is gone, then assert the operation is still running.
- `crates/hub/src/machines/link/tests.rs` (`wait`, :91), `crates/hub/src/tenancy/tests/access.rs` (:409), `crates/hub/src/site/tests.rs` (:778): bare 200, 200 and 800 ms windows to prove something did not happen; the positive assertions in link/tests.rs after `wait()` also sit behind the fixed wait. -> positive cases poll with a deadline (`output_holding` shape); negative cases wait a window derived from the injected duration (`links.offline_after`) or a signal that the attempt ran (for example a reload-attempt counter on the site watcher).

### Rule 8 (no retries; a cause before a test-group)

- `.config/nextest.toml` (no `[test-groups]`): `.ai/dev/plans/path.md:134` records 2 of 14 local `cargo test -p devagotchi-machine-engine` runs on Windows failing in the console test (Ctrl-C into `ping -t`). The cause is open. -> find the cause first. Only then, if needed: `[test-groups] os-console = { max-threads = 1 }` with `cfg(windows)` overrides in both profiles for the interactive and console tests. Keep `retries = 0`.

### Rule 10 and rule 15 (gates run by CI)

- `.ai/dev/workflow.md` (:44-45, `node scripts/generate-contracts.cjs --check`, `devagotchi ai check`): pre-push gates no workflow runs, so the desktop types and the AI contract can drift. -> run both in a new ci.yml `static` job: `npm ci --prefix apps/desktop --ignore-scripts`, then the generate check, then `cargo run --locked -- ai check`.

### Rule 12 (a missing capability fails rather than passes)

- `tests/e2e.py` (analytics block, :233-244): with `--clickhouse-container` given but `DEVAGOTCHI_CLICKHOUSE_URL` unset, it prints "skipped" and sets `analytics_ok = True`. -> exit nonzero before the probes when the container was asked for and the URL is missing; skip only when no container was asked for.
- `tests/security_boundary.py` (:563, :582, :680): `unittest.skipUnless(os.geteuid() == 0)` skips the foreign-UID and peer-UID cases, and `result.wasSuccessful()` counts skips as success. -> exit nonzero when `result.skipped` is non-empty unless `--allow-skip`; run it under `sudo` in CI.
- `tests/device_login_browser.py` (`main`, :107) and `tests/clients_oauth.py` (`main`, :663): no NOT RUN contract; a missing Playwright is an ImportError. -> copy `tests/claim_browser.py:101-110`: `--allow-skip`, and `NOT RUN` with exit 2.

### Rule 13 (wait on a condition with a deadline)

- `tests/analytics_browser.py` (:197-198): `time.sleep(1.5)` because "the writer batches for a second", then one read. -> poll `/operator/analytics?days=7` with a deadline until the counts appear, with the last text in the failure.

### Rule 14 (shared helpers in a helper module)

- `free_port` copied 7 times: `tests/claim_browser.py:39`, `tests/clients_oauth.py:83`, `tests/device_login_browser.py:44`, `tests/e2e.py:119`, `tests/machine_update_probe.py:123`, `tests/operator_browser.py:45`, `tests/passkey_browser.py:33`. Runnable scripts importing runnable scripts: `tests/analytics_browser.py:35`, `tests/android_browser.py:37`, `tests/app_browser.py:43-45`, `tests/compose_join.py:49`, `tests/first_connect_browser.py:37-39`, `tests/orgs_browser.py:29`, `tests/workflow_browser.py:50-52`, `tests/native_ui_smoke.py:34`. An edit to one scenario silently changes others, including `compose_join.py` in `image.yml`. -> `tests/harness.py` as a non-runnable module, as `tests/fake_nomen.py` is; scenarios import only from it.

### Rule 15 (every script run by CI or listed as manual)

- `tests/mcp_protocol.py`, `tests/mcp_smoke.py`, `tests/security_boundary.py`: gates in `.ai/dev/workflow.md:41-43` that no workflow runs. `.ai/dev/plans/path.md:134` records that `mcp_protocol.py`'s catalog lacks the P7 to P9 tools, so it has drifted unseen. -> run `mcp_protocol.py` from `tests/e2e.py`, so it runs on Linux and Windows (C16); run `security_boundary.py` under `sudo` in the ci.yml `e2e` job on ubuntu-24.04, with `mcp_smoke.py` (B6).
- `tests/native_ui_smoke.py`, `tests/retry_lifecycle.py`: no workflow runs either. native_ui_smoke drives the compiled Tauri app and needs Tauri WebDriver, WebKitWebDriver, Xvfb and D-Bus (`apps/desktop/README.md:114`); retry_lifecycle runs 4097 real retirements by default (its docstring). -> list both in `tests/MANUAL.md` (it does not exist yet) with the command and when to run them.
- `apps/desktop/src-tauri/src/lib.rs` (tests at :244, :259, :291), `apps/website/package.json` (`test`, :15), `apps/desktop/package.json` (`qa:test`, :11): the 3 desktop Rust tests are only compiled (`ci.yml` :63-64, `cargo check --all-targets`); the website and desktop JS tests are run by nothing. -> on windows-2025 `core`, `cargo nextest run --locked --profile ci -p devagotchi-desktop`; list the JS tests in `tests/MANUAL.md` or add a node step.

### Rule 16 (least privilege, pinned)

- `.github/workflows/release.yml` (`uses:` at :63, 73, 89, 127, 138, 146, 177, 194, 211, 216, 223, 241, 292, 297, 304, 317, 324, 357): 18 refs are tags (`actions/checkout@v7`, `actions/upload-artifact@v7`, `actions/download-artifact@v8`, `swatinem/rust-cache@v2`, `actions/attest@v4`), and `build-local-artifacts` holds `id-token: write` and `attestations: write` (:119-122). Every other workflow pins SHAs. -> `[dist.github-action-commits]` in `dist-workspace.toml` with the SHAs ci.yml, install-check.yml and image.yml already use, plus resolved SHAs for `swatinem/rust-cache` and `actions/attest`; then `dist generate`.
- `.github/workflows/release.yml` (top-level `permissions`, :17-18): `contents: write` for every job, including `plan` and the builds. -> top level `contents: read`; `contents: write` on `host` and `announce` only, kept as a hand edit under `allow-dirty` (`dist-workspace.toml:42`).

### Rule 17 (every job has timeout-minutes)

- `.github/workflows/ci.yml` (`repository-owner`, :16) and `.github/workflows/security.yml` (`deny`, :14): no `timeout-minutes`. -> `timeout-minutes: 10` on both.
- `.github/workflows/release.yml` (`plan` :53, `build-local-artifacts` :95, `build-global-artifacts` :202, `host` :278, `announce` :345): the jobs dist generates carry no timeout. -> add them (plan 10, builds 60, host 20, announce 10) as hand edits under `allow-dirty`, and say in the `dist-workspace.toml` comment that they are hand-kept.

### Rule 18 (one step per independent check)

- `.github/workflows/ci.yml` (`e2e`, step "Sign-in pages in Chrome", :258-272): ten browser scripts in one `run: |` under `bash -e`; the first failure hides up to nine results. -> the pip install as its own step, then one step per script, each named for what it proves, with its own `DEVAGOTCHI_HUB_STORAGE_URL` in `env:`.

### Rule 20 (results are kept)

- `.config/nextest.toml` (`[profile.ci.junit]`, :13-14) with the nextest steps at `ci.yml` :67, :91, :97: junit.xml is written and never uploaded, and skip lines would be dropped because `store-success-output` defaults to false. -> `store-success-output = true`; after each nextest step, `actions/upload-artifact` pinned to the SHA `live.yml` uses, `if: always()`, named per job and OS, `retention-days: 14`.

### Static layer (no rule number; `docs/implementation-and-release-plan.md:545`)

- `.github/workflows/ci.yml` (no fmt or clippy step): `cargo fmt --all -- --check` is named as the baseline and `Cargo.toml:98-99` sets clippy `all = "warn"`, but no job runs either, and `rust-toolchain.toml`'s `profile = "minimal"` installs neither component. The backlog is unmeasured. -> a `static` job on ubuntu-24.04 with `timeout-minutes: 15`: `rustup component add rustfmt clippy`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, then the rule 10 and 15 gates. Measure the backlog first; the job lands green together with the fixes it needs (G12).

### Cadence (release builds on main)

- `.github/workflows/release.yml` (`build-local-artifacts`, `if:` at :100): it runs only when `publishing` is true (a tag) or dist's `pr_run_mode` is `upload`. #258 (2b3feec) changed `pr-run-mode` from `"upload"` to `"skip"` (`dist-workspace.toml:37`), so on main `build-local-artifacts` is skipped, and with it `build-global-artifacts`, `custom-install-check` and `custom-hub-image`, which need it. The Release run for 2b3feec on main ran only `plan`; the runs for 18a4096 and f1b48e8 built every target, passed install-check and published the image. The comments at `release.yml:44-46`, `dist-workspace.toml:33-36`, `release.yml:260-264`, `image.yml:10-12` and `ci.yml:148-149` still say that every main commit is built, or that main publishes the image. -> those comments record the operator's decision (2026-11-18) that main builds: add `|| github.ref == 'refs/heads/main'` to that `if:` as a hand edit kept under `allow-dirty`, then confirm on the next main run that install-check and the image run. If main is not to build after all, rewrite the five comments instead.

### G11 (comments are true)

Fixed in f13b8e1 (B0): the `hub-postgres` header now names `rdeps(devagotchi-store)` and the REQUIRE flag.

- `.github/workflows/install-check.yml` (header, :7): "pull requests and main run it too", but `release.yml` has no `pull_request` trigger (`release.yml:42`, `dist-workspace.toml:37` `pr-run-mode = "skip"`), and at 2b3feec main skips it too (see Cadence above). -> once main builds again: "Main and every release tag run it (release.yml). Pull requests get the installers checked from their own build by ci.yml's e2e Windows installer and host-install jobs." Until then, "Every release tag runs it (release.yml)" in place of the first sentence.
- `.github/workflows/image.yml` (step comment, :132-134): "A pull request's image is never pushed" describes a caller image.yml no longer has; only `release.yml` calls it, on push. -> "The image of this run is not pushed until the join below passes, so the image just built stands in under that name." Keep the `github.event_name != 'pull_request'` guards (:157-195) as defence in depth.
