# B2: the machine link and join follow their pattern

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Request: the operator, 2026-11-19: carry out [the rewrite](rewrite.md) batch by batch.
- PR: #261 (merged as `86b9d8b`)
- Branch / baseline: `increment-271/link-and-join`, from main `a596db6`.
- Objective: every violation [the link pattern](../../patterns/link.md) listed at main
  2b3feec is fixed, with the pattern's mechanical checks in the tree.
- Non-goals: no new frame, hello field or protocol version; no new lifecycle step; no
  macOS provider; nothing in `devagotchi-cloud`.
- Canonical architecture: [.ai/patterns/link.md](../../patterns/link.md), shared rules in
  [.ai/patterns/README.md](../../patterns/README.md).

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | The wire: shared values and the golden test | B1 | `cargo test -p devagotchi-contracts` | done |
| M2 | The link: folders, named arms, deadlines, fake-hub tests | M1 | `cargo test -p devagotchi-link` | done |
| M3 | The hub: fixture, test split, gates, refusals, socket test, `/enroll` gone, audit outcomes | M1 | hub tests on the test Postgres | done |
| M4 | Join and installers: no personal paths, the code never on a command line | none | `join/tests.rs`, `link_protocol.rs`, install checks in CI | done |
| M5 | Docs say what the code does | M1-M4 | review | done |
| M6 | Validate, push, PR, merge on green | M1-M5 | the suite, e2e, CI | in progress |

## What each violation became

Rule 1, shared values (G3):

- `LINK_HEARTBEAT` (20 s), `LINK_IDLE_TIMEOUT` (60 s) and `LINK_SEND_TIMEOUT` (10 s) in
  `crates/contracts/src/link/mod.rs`, held by `const` assertions; the hub's `IDLE_TIMEOUT`
  and `SEND_TIMEOUT` are those, and the link's own loop uses them (rule 6).
- `MACHINE_KIND_CONTAINER` at the link (`run/mod.rs`, `update/mod.rs`), the hub (`enroll.rs`)
  and the compose test.
- `REFUSAL_UNAVAILABLE` at the hub's `wire_refusal` and the link's `refusal`.
- `UPDATE_WAITING`, `UPDATE_DOWNLOADING`, `UPDATE_INSTALLING`, `UPDATE_DONE`, `UPDATE_FAILED`,
  with `UPDATE_STATES` built from them and `update_finished`, at every site. The hub-only
  `"requested"` is `machines::update::REQUESTED`.
- `valid_build_name` in the contracts; the copies in the link's update and the hub's
  `downloads.rs` are deleted, and `compose.rs` `image` calls it.

Rule 2, additive fields: `Hello.build` is skipped when None, so an enroll hello no longer
sends `"build":null`. The golden list keeps the `null` lines released links send.

Rule 5, exhaustive dispatch: the link's three frame matches name every variant.

Rule 6, bounded reads and writes: the link Pings on every heartbeat, gives the hub up after
`LINK_IDLE_TIMEOUT` of silence (Pong included), and times out every write. Tests play a hub
that goes silent (given up) and one that only answers Pings (kept). They run the loop
through `connect_with` with a short `Liveness`, not paused time, so they do not depend on
how a runtime advances a paused clock while a real socket is pending.

Rules 7 and 12, refusal codes: the wire table checks every code `admit` and `may_enroll`
give, including `protocol_version` (passed through), `audit_write` (`unavailable`) and
`early_access` (`refused`), and that no detail leaks; `admit` refuses a hello one version
ahead as `protocol_version` without spending the code.

Rule 8, one signed door (D3, default (b)):

- `POST /enroll` is removed, with its `discovery.rs` `PRIVATE` entry and doc mentions;
  `enroll_owned` is test-only; tests join through `Hosted::enroll`, which asks the same
  early-access gate the hello does.
- The Windows installer defaults to the code in `DEVAGOTCHI_ENROLL_CODE`; `-EnrollCode`
  warns that it is deprecated. The hub's "Add a machine" page, which still told people to
  pass `-EnrollCode`, sets the variable instead (found in this batch, not in the list).

Rule 9, audit:

- The link's events go through `Audit::record_after`: a failed write raises the operator
  alert from B1 (D1) instead of only printing.
- Every link outcome is `ok` or a code. Detail moved to the target: the owner, the build and
  `force`, the update's detail, the refusal count, the revoke counts, the join way. A failed
  update is `update_failed`. `operator_event` takes a target, so the CLI's revoke does the
  same.

Rule 11, join templates:

- `join.ps1` installs under `$env:DEVAGOTCHI_ROOT`, else `%LOCALAPPDATA%\Devagotchi`; the
  comment naming one person's machines is gone.
- The installer's `-Root` defaults the same way. An upgrade that passes no `-Workspace` keeps
  the workspace its broker config names, so an existing install keeps its own; a fresh one
  uses `<Root>\workspace`. `ci-install-check.ps1` upgrades without `-Workspace` and checks
  that no new workspace appeared.
- No drive-letter path or `localdev` in any join template or installer, checked by
  `link_protocol.rs`.

Rule 12, test layers:

- The golden test (`SEEN`, `hub_name`, `link_name`, pinned signing bytes and version).
- Hub gate tests in `tests/updates.rs`: `update_unsupported` for a hello without a build,
  `AlreadyOn`, `no_build`, and a sent update whose reports are recorded.
- The hub socket test (`tests/socket.rs`) on `hosted_with` with `tokio-tungstenite`: a
  result routed to its call, a mid-session hello ending as `protocol_error` in the audit,
  and a revoke sending `device_revoked` and closing within two revocation checks.
- `Session::fake` replaces the five hand-built sessions; `links!` and its tempdirs are gone
  (`or_skip!(test_links())`); the absence window is derived from `offline_after`.

Layout (G10): `crates/link/src/{run,session,update}/` with `tests.rs` each; `lib.rs` is the
command line. `machines/link/tests/` is one file per subject. `join/mod.rs` moved its tests
to `join/tests.rs` (482 to 291 lines). The `enroll` helper lives in
`tenancy/tests/machines.rs`, so `tenants.rs` shrank (its ratchet entry lowered).

Docs (G11): the stale paths (`machine_update.rs`, `link.rs`, the `devagotchi-link` binary),
`crates/link/README.md` (the real command, a placeholder hub, the installers, Update and
Layout sections), `AGENTS.md`'s contracts sentence, and `.ai/patterns/link.md` rewritten to
name functions instead of lines.

Also in this batch:

- `crates/mcp/src/arguments.rs` imported `ContentBlock` outside its tests (a warning from
  B1).
- `git_bash.rs` `runs_shell_script` was dead code that duplicated `bash_line`; it is folded
  into `bash_line`'s doc and tests.
- The rewrite plan itself is committed as [rewrite.md](rewrite.md), with the operator's
  colocated-styles and fluid-clamp request recorded under B5.

## Risks and approved decisions

- D3 (remove `/enroll`): the plan's default, (b). No released program calls the route; the
  installers and the join lines all enroll through the signed hello.
- The Windows default root moves from `D:\localdev\devagotchi` (when `D:\localdev` existed)
  to `%LOCALAPPDATA%\Devagotchi`. An installed machine keeps its root: an update passes
  `-Root` from its own state directory. A person who wants another root sets
  `DEVAGOTCHI_ROOT` before the join line.
- Audit outcomes change shape (alpha; C17). Nothing reads the old strings.
- The link now Pings the hub every 20 s and expects traffic within 60 s. Every hub answers
  Pings (axum's WebSocket replies automatically), so no hub update is needed first.

## Verification receipts

2026-11-19, Windows 11, test Postgres required: the workspace suite 841 passed, 0 failed, no
SKIPPED; `python tests/e2e.py` on a fresh database passed; fmt, the layout ratchet and
`generate-contracts --check` clean. CI on #261: 18 of 18 checks passed (Linux x86_64 and
arm64, Windows 2022, 2025 and 11 arm, macOS core, e2e, install checks, compose, Postgres).

## Delivery

Implementation: done. Validation: done (above). Push: done.
PR: #261, squash-merged. Main: `86b9d8b`, branch deleted.
Not yet: on Texas and the phone (the path's receipt). The release build of `86b9d8b` skipped
every job (fixed in [main builds again](main-builds.md)), so no build of B2 exists to deploy.
