# Pattern: the machine link

How a machine and its hub talk, and how each step of a machine's life is added. Follow it for
every new frame, hello field, gate or lifecycle step.

This pattern states only what is particular to the link. The shared rules G1 to G14 are in
[README.md](README.md) and hold here too. Where a rule below says "G<n>", the shared
rule is the base and this file adds the link's specifics. Layout comes from
[layout.md](layout.md).

A self-hosted, one-user hub on Linux reaches the user's own Windows and Linux machines, and
machines in Docker, through one outbound WebSocket. Each machine's `devagotchi link` holds it
open.

```text
AI client -> hub /mcp -> Links::call -> HubFrame::Call -> /link <-wss- devagotchi link -IPC-> broker
```

References below name a file and a function, not a line: lines move, names are checked by
the compiler.

## The parts

| Part | Home |
|---|---|
| The wire: frames, sizes, versions, deadlines, signed bytes, words both sides compare | `crates/contracts/src/link/mod.rs`, re-exported flat from `lib.rs` (G10's contracts exception); its golden tests in `link/tests.rs` |
| Machine side | `crates/link/src/`: `lib.rs` (the command line), `run/` (enroll, run, backoff), `session/` (one connection), `update/` (updating the machine, with `handoff.rs`, `idle.rs` and the Windows updater script), `state.rs` |
| Hub side | `crates/hub/src/machines/link/`, one job per file: `serve.rs` (the session loop), `enroll.rs` (`admit`, `may_enroll`), `calls.rs`, `updates.rs`, `presence.rs`, `registry.rs`; the door in `crates/hub/src/machines/routes.rs`; what the hub remembers about updates in `crates/hub/src/machines/update.rs` |
| The AI's verbs `machines`, `machine_add`, `machine_update` | tools in `crates/hub/src/machines/manage.rs`; `update_machine` in `crates/hub/src/mcp/machines.rs` |
| Join: the one line (sh, ps1, compose) and the installers it downloads | `crates/hub/src/machines/join/`, `deploy/linux/install-devagotchi.sh`, `deploy/windows/install-devagotchi.ps1` |
| Build name | `crates/devagotchi/build.rs` (`DEVAGOTCHI_BUILD`). The hub offers its downloads' version (`crates/hub/src/serve/mod.rs`, the `.offering(..)` call). |

Versions are on three axes, each with one job:

- `LINK_PROTOCOL_VERSION` (1) must match exactly in the hello (`enroll.rs` `admit`). It is
  part of the signed bytes (`link_signing_message`).
  - A hub that refuses the version never holds a session for that machine. `machine_update`
    then cannot reach it (`updates.rs` `ask_update`), and the link exits 2
    (`run/mod.rs` `dispatch`).
  - **Bumping the version strands every machine until someone reinstalls it by hand.**
- `API_VERSION` (1) is checked by the link on every `call` (`session/mod.rs` `handle`).
- The build (`main-<sha7>`, `ci-<sha7>`, `X.Y.Z`, `dev`; `valid_build_name` in the contracts)
  is a capability signal. Only a session whose hello carried one may be sent `update`
  (`updates.rs` `ask_update`).

## Exemplar

Three exemplars, on main after B2 (increment 271).

**Change shape: the container `kind` slice** (increment 209). It is the one protocol change
that touches every layer with its own test, and the one that uses serde's forward-compatible
field idiom.

- Contracts: `LinkFrame::Hello.kind` and `MACHINE_KIND_CONTAINER`
  (`crates/contracts/src/link/mod.rs`). Its golden line in `link/tests.rs` `SEEN`, and
  `fields_added_after_v1_stay_off_the_wire_when_none`.
- Link: `KIND_ENV` and `run` (`crates/link/src/run/mod.rs`), `Updater::kind` and
  `Updater::request` (`crates/link/src/update/mod.rs`). Test
  `a_container_refuses_every_update_and_says_so` (`update/tests.rs`).
- Hub: `admit` (`enroll.rs`), `Session.container` (`machines/link/mod.rs`), `ask_update` and
  `auto_update_now` (`updates.rs`). Test
  `a_container_is_never_asked_to_update_and_is_told_how_to_move` (`tests/container.rs`),
  which asserts on the device's receiver that nothing was sent.
- Join: `compose.yml` and its test in `compose.rs`, which asserts the kind from the constant.
- Real OS: `tests/compose_join.py` `docker_leg`: `machine_update` on the Docker machine is
  refused as a container's, and a second `docker compose up -d` leaves it online.

**Loop shape: the hub session**, `Links::serve` (`crates/hub/src/machines/link/serve.rs`).

- Bounded before trust: the handshake permit (`routes.rs` `link_upgrade`), a hello of at most
  4 KiB within 10 s (`HELLO_TIMEOUT`, `MAX_HELLO_BYTES`), every write within
  `LINK_SEND_TIMEOUT` (`send`).
- Exhaustive dispatch with no wildcard.
- Each exit is a named reason, audited as `link.disconnect`.
- Redacted wire refusals (`wire_refusal`) with an itemized, rate-limited audit (`refused`).

**The same shape on the machine**, `connect_with` (`crates/link/src/session/mod.rs`): its
three frame matches name every variant; the serve loop Pings on every `LINK_HEARTBEAT`, gives
the hub up after `LINK_IDLE_TIMEOUT` with nothing from it (its Pong included), and times out
every write after `LINK_SEND_TIMEOUT` (`write`). `Liveness` carries the three so the tests in
`session/tests.rs` run the loop in milliseconds against a fake hub.

## The canonical shape

The wire is one enum per direction, internally tagged and strict (G2). An optional field added
later looks like `kind`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkFrame {
    Hello {
        // ...v1 fields...
        /// What it says, and what an older peer sends instead.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_field: Option<String>,
    },
    // ...
}
/// Sent only to a session whose hello said `new_field` (gate: Session.new_field).
// HubFrame::NewThing { .. }
pub const NEW_FIELD_VALUE: &str = "..."; // every word either side compares (G3)
```

On the hub, the hello field becomes a `Session` field (and a `Session::fake` argument), the
gate refuses with a code and a reason, and the send goes through `control` (`enroll.rs`
`admit`, `machines/link/mod.rs` `Session`, `updates.rs` `ask_update`):

```rust
let session = self.sessions.lock().expect("links").get(device).cloned().ok_or_else(|| offline(device))?;
if !session.new_field {
    return Err(BridgeError::new("<x>_unsupported", "<why, and what the person can do>"));
}
session.control.try_send(HubFrame::NewThing { .. }).map_err(|_| offline(device))?;
self.event(subject, via, "machine.<x>", device, &target, "ok").await;
```

A dispatch decodes, then names every variant of the frame enum (`serve.rs` `Links::serve`):

```rust
match serde_json::from_str::<LinkFrame>(&text) {
    Ok(LinkFrame::Result { id, response }) => { /* ... */ }
    Ok(LinkFrame::Heartbeat) => {}
    Ok(LinkFrame::UpdateStatus { state, detail }) => { /* ... */ }
    Ok(LinkFrame::Hello { .. }) | Err(_) => break "protocol_error",
}
```

## Rules

1. **The wire lives in `crates/contracts` (G1, G3).**
   - Every frame is a variant of `HubFrame` or `LinkFrame`.
   - G3 applies to every value one crate writes and the other compares, and to two values
     whose correctness depends on each other across the crates.
   - For the link that means: message sizes (`MAX_LINK_MESSAGE_BYTES`), the liveness trio
     (`LINK_HEARTBEAT`, `LINK_IDLE_TIMEOUT`, `LINK_SEND_TIMEOUT`, held together by `const`
     assertions), the update states (`UPDATE_*`, `UPDATE_STATES`, `update_finished`), the
     refusal word `REFUSAL_UNAVAILABLE`, the machine kind `MACHINE_KIND_CONTAINER` and the
     build-name rule `valid_build_name`.
   - The refusal words only the hub writes (`refused`, `protocol_version`, `device_revoked`)
     and the hub-only update state `REQUESTED` (`machines/update.rs`) stay hub constants.
2. **Evolve within protocol 1, additively (G2).**
   - Never bump `LINK_PROTOCOL_VERSION` or change `link_signing_message` without an operator
     decision recorded in a plan.
   - Every shape a released peer sends keeps parsing, including today's `null`s. The golden list
     (`SEEN`, Mechanical check 1) is append-only.
   - Why additive in alpha, when `.ai/README.md` rules out compatibility layers: an enrolled
     machine cannot be fixed from the hub. A machine that stops parsing its hub needs a person
     at it to reinstall. This is the only reason, and it covers only the link wire.
3. **Hubs update first, and the hub sends a post-v1 `HubFrame` only to a session that said it
   understands it.**
   - The hub learns this from the hello and keeps it on `Session`
     (`docs/spec-machine-update.md`, section U2).
   - The gate refuses with a code and a reason the person can act on (`update_unsupported` in
     `updates.rs` `ask_update`).
   - Without the gate, an old link cannot parse the unknown frame, so it drops the connection
     and reconnects.
4. **The link sends a post-v1 `LinkFrame` only as an answer to a hub frame, or to a hub that
   updates first.** `UpdateStatus` answers `Update` (`session/mod.rs`, the `Update` arm). The
   handoff outcome follows an `Update` the previous session got (`update/handoff.rs`
   `outcome`).
5. **Dispatch is exhaustive (G5).**
   - The link's dispatches are the two handshake matches and the serve loop in
     `session/mod.rs` `connect_with`. The hub's is the session loop in `serve.rs`
     `Links::serve`.
   - Extracting one variant (`if let`, `let ... else`, as in `serve.rs` reading the hello and
     `enroll.rs` `admit`) is a decoder, not a dispatch, and may ignore the rest (G5).
6. **Bound everything before trust, and every read and write after it, on both sides (G6).**
   - Hub, before trust: 32 handshakes, 503 at the door (`MAX_HANDSHAKES`, `routes.rs`
     `link_upgrade`). A hello of at most 4 KiB within 10 s. 1 MiB per message on both sides
     (`MAX_LINK_MESSAGE_BYTES`, set on the hub's upgrade and the link's `WebSocketConfig`).
   - After trust, both sides: every write within `LINK_SEND_TIMEOUT`, silence beyond
     `LINK_IDLE_TIMEOUT` ends the connection (Ping and Pong count as traffic). The hub queues
     64 calls per device (`CALL_QUEUE`).
   - Link: 16 calls and 8 waits at once, `link_busy` beyond them (`MAX_IN_FLIGHT`,
     `MAX_WAITS`).
   - A new step that reads or writes the socket uses its side's helpers (`serve::send`,
     `session::send`, `session::write`, `next_frame`), never its own.
7. **Refusals say little on the wire and everything in the audit (G4).**
   - A handshake refusal says only `refused`, `unavailable` (retry) or `protocol_version`
     (`serve.rs` `wire_refusal`). The reason is audited at most once a second, with a count of
     the rest (`serve.rs` `refused`).
   - A device that already proved its key and is then revoked is told `device_revoked`
     (`serve.rs` `revoked_frame`).
   - The link treats `REFUSAL_UNAVAILABLE` as Retry. Any other refusal is permanent: exit 2,
     so supervisors stop (`session/mod.rs` `refusal`, `run/mod.rs` `dispatch`).
8. **A key is bound only by proving it (G8).**
   - Binding happens only in a hello signed over `link_signing_message(hub, device, nonce)`.
     Weak keys are refused, then the signature is checked with `verify_strict` (`enroll.rs`
     `admit`). There is no other door: `POST /enroll` was removed in B2 (decision D3).
   - The enroll code is a secret under G8. Every program on the join path reads it from the
     environment or stdin, never from argv (`crates/link/src/lib.rs` `USAGE`, `run/mod.rs`
     `enrollment_code`, `join.sh`, `join.ps1`, `compose.yml`, the hub's "Add a machine" page
     in `web/machines.rs`). The Windows installer's `-EnrollCode` is deprecated and warns.
9. **Every lifecycle transition is audited (G7)** under a dotted name:
   - `link.connect`, `link.disconnect` (outcome: its reason), `link.refused`, `link.enroll`,
     `link.code_issued`, `link.join_script`, `link.revoke`, `machine.update`,
     `machine.update_status`.
   - The outcome is `ok` or a code (`update_failed` for a failed update, the word
     `push_machine` uses). Detail goes in the target: the key fingerprint and owner, the
     build and `force`, the update's detail, the count of refusals not itemized, the revoke
     counts, the join way.
   - A new step adds its name. It never records the code, and records a key only as its
     fingerprint.
   - The link's events are written after what they record already happened, so a failed
     audit write raises the operator alert (`Audit::record_after`, D1) instead of failing it.
10. **Lifecycle steps are idempotent and say what happened.**
    - `enroll` again with the same hub, device and key succeeds with no network call
      (`run/mod.rs` `already_enrolled`).
    - A reconnect replaces the old session (`serve.rs` `Links::serve`).
    - A second update request while one runs returns the current status (`update/mod.rs`
      `Updater::request`).
    - A step that hands off to something that outlives the link records it in the state dir,
      and the next link reports it once (`update/handoff.rs` `pending` and `outcome`).
    - An upgrade that passes no `-Workspace` keeps the workspace the machine's broker config
      already names (`install-devagotchi.ps1`).
11. **Join artifacts are templates.**
    - Each value is filled only after a strict validator passes. It is quoted for its format:
      single quotes in sh and ps1 (`join/mod.rs`). In compose, every value passes a strict
      charset check first, so it is safe in YAML whether or not the template quotes it
      (`compose.rs` `file`).
    - Every placeholder is filled. The code appears exactly once and never on a command line.
    - Neither a template nor an installer it downloads carries a machine- or person-specific
      path or default. Windows installs under `$env:DEVAGOTCHI_ROOT`, else
      `%LOCALAPPDATA%\Devagotchi`.
    - A compose refusal is never a 200 (`compose.rs` `serve`). A dead or unknown code on the sh
      or ps1 line gets a 200 script that only fails with the reason (`join/mod.rs` `refusal`).
12. **Every change lands with its test layers** (The canonical test).
    - A gate's test asserts on the device's receiver that nothing was sent.
    - A refusal's test asserts the exact code.

## Where files go

G10 holds.

- `crates/contracts/src/link/mod.rs`: the frames, constants and signing bytes. `lib.rs`
  re-exports them flat with `mod link; pub use link::*;` (G10's contracts exception). Tests
  go in `link/tests.rs`.
- `crates/link/src/<step>/mod.rs`, with `tests.rs` beside it: `run/`, `session/`, `update/`.
  `lib.rs` keeps `USAGE`, `parse_args` and `cli`, and the `parse_args` tests.
- `crates/hub/src/machines/link/<job>.rs`, one job per file. Tests go in
  `machines/link/tests/<subject>.rs` (`admit`, `calls`, `container`, `presence`, `socket`,
  `updates`, `wire`); `tests/mod.rs` holds the shared fixtures `test_links`, `hello` and
  `connect` over `Session::fake`.
- `machine_add`, `machine_update` and `machines` are served from
  `crates/hub/src/machines/manage.rs` (layout rule 7: the domain serves its tools).
- Join templates stay in `crates/hub/src/machines/join/`, with their tests in
  `join/tests.rs`. `join/compose.rs` and `machines/update.rs` keep their inline tests while
  under the cap (G10).

## Adding a frame or a hello field

1. Contracts: the variant or field (rule 2), any constants (rule 1), and its golden lines
   (Mechanical check 1). Golden lines are appended, never edited.
2. Hub: read the capability in `admit` and keep it on `Session` and `Session::fake`. Add the
   dispatch arm (rule 5), the gate (rule 3) and the audit name (rule 9).
3. Link: the dispatch arm, the sender, and the state the frame changes.
4. Tests: the layers below. CI runs them with `cargo nextest run --locked --profile ci` under
   G13: the `core` job, and the `hub-postgres` job (`rdeps(devagotchi-store)`) for the hub
   tests that need Postgres.

## Adding a lifecycle step

- Name its trigger: hello, a call, a control frame, a timer or a CLI verb.
- Give it an idempotent second run (rule 10), an audit name (rule 9), and a refusal code with a
  reason (G4).
- Add its line to `tests/e2e.py`'s link steps. A step about installing or joining goes in
  `tests/compose_join.py` or `deploy/*/ci-install-check.*` instead.

## The canonical test

1. **Wire golden** (`crates/contracts/src/link/tests.rs`).
   - Every shape a released peer sends parses (`SEEN`, append-only).
   - Every variant has a line, and naming a new variant is a compile error until it is
     added (`hub_name`, `link_name`).
   - A None optional added after v1 is absent from the JSON.
   - `link_signing_message` and `LINK_PROTOCOL_VERSION` are pinned byte for byte.
2. **Hub unit** (`machines/link/tests/`).
   - Runs on `Links` over a fresh Postgres database, taken as `or_skip!(test_links())` (G13).
   - Builds the session with `connect(..)` over `Session::fake(..)`, which returns the
     `calls` and `control` receivers.
   - Asserts the code, and what the receiver did or did not get.
   - Exemplars: `tests/container.rs`; `tests/updates.rs`;
     `replays_other_hubs_other_keys_and_revoked_devices_are_refused` (`tests/admit.rs`).
3. **Hub socket**, for a loop arm (`machines/link/tests/socket.rs`).
   - The whole router on a real port with ConnectInfo: the
     `tenancy::tests::tenants::hosted_with` harness.
   - Plays the device with `tokio-tungstenite` (a hub dev-dependency) and signs with
     `link_signing_message`.
   - Asserts what reached the device and the `link.disconnect` audit row's outcome.
4. **Link unit** (`crates/link/src/<step>/tests.rs`).
   - Exercises the step with a fake broker (`update/tests.rs`) or none.
   - A dispatch arm or a deadline gets a fake hub on `127.0.0.1:0` with
     `tokio_tungstenite::accept_async` (`session/tests.rs`), driven through `connect_with`
     with a short `Liveness`. `state::endpoint` allows loopback `http`.
5. **Real binaries.**
   - `tests/e2e.py` link steps, run by the `e2e` job in `.github/workflows/ci.yml` on
     ubuntu-24.04, ubuntu-24.04-arm, windows-2025 and windows-2022. windows-11-arm runs
     `e2e.py --no-hub`, which skips them. A step is a `steps.append(run_probe(..))` or
     `steps.append(run_cmd(..)[0])` line with its expected code or exit.
   - `tests/compose_join.py` (`image.yml`): the join lines exactly as `machine_add` gives them.
   - `deploy/windows/ci-install-check.ps1` and `deploy/linux/ci-install-check.sh`: the ps1 and
     sh lines through the real installers, the code in the environment.
   - `tests/machine_update_probe.py`: a real update between two builds (`none` from e2e on
     Linux; `windows` and `linux` from the install checks).

## Anti-patterns

- An `Ok(_)` or `_` arm in a frame dispatch (G5). On the link a new variant then becomes a
  reconnect loop instead of a compile error.
- Sending a new `HubFrame` to every session, or checking a gate on one side only.
- A word or rule both crates use, written in each (G3): `"container"`, `"unavailable"`, the
  update states, the build-name rule.
- A field added to an existing frame with only `#[serde(default)]` (G2). It puts `null` on the
  wire, which an older strict peer refuses.
- Bumping `LINK_PROTOCOL_VERSION` to ship a feature.
- A second door that binds a key without a signature.
- An enroll code in argv, in a help example or in a message that tells a person to pass it.
- A socket read with no deadline, or a write with no timeout (G6).
- Building `Session { .. }` by hand in a test.
- A personal path or default in a join template or an installer.
- A line number in a doc reference: name the file and the function.

## The mechanical check

Three checks hold rules 1, 2, 5 and 11. None needs a database. CI runs them through its
existing `cargo nextest run`.

**1. Wire golden**, `crates/contracts/src/link/tests.rs`:
`every_frame_a_released_peer_sends_still_parses_and_every_variant_has_a_line`,
`the_signed_bytes_and_the_protocol_never_change`,
`fields_added_after_v1_stay_off_the_wire_when_none`. Beside the constants, `const`
assertions hold the liveness trio (G6). It catches:

- a new required field (the old hello lines fail);
- a renamed tag or field (its line fails);
- a new variant without a line (the set comparison fails);
- a new variant not named in `hub_name` or `link_name` (does not compile);
- a change to the signed bytes or the protocol version.

**2. Source scan**, `crates/devagotchi/tests/link_protocol.rs`, walking the workspace with
`crates/devagotchi/tests/support/mod.rs` (shared with `layout.rs`; G12). It needs no
allow-list. Test files (`tests.rs`, `*_tests.rs`, anything under a `tests/` folder) are not
scanned, and neither is text after a file's first `#[cfg(test)]`. It fails when:

- `crates/link/src/session/mod.rs` or `crates/hub/src/machines/link/serve.rs` contains
  `Ok(_) =>` or `Ok(_) |` (rule 5);
- a Rust file under `crates/link/src`, `crates/hub/src/machines/link` or
  `crates/hub/src/machines/join` holds the literal `"container"`, `"unavailable"`,
  `"waiting_for_jobs"`, `"downloading"` or `"installing"` (rule 1). `done` and `failed` are
  too common to scan for; review holds them;
- `join.sh`, `join.ps1`, `compose.yml`, `deploy/windows/install-devagotchi.ps1` or
  `deploy/linux/install-devagotchi.sh` contains `localdev` or a drive-letter path such as
  `D:\` (rule 11). These five are named, not walked.

**3. Clippy, once CI runs it.** `clippy::wildcard_enum_match_arm` does not apply today. It
ignores `Result` and `Option` scrutinees, and no workflow runs clippy. To use it, decode first
and then `match frame { .. }` on the enum itself, and put
`#[deny(clippy::wildcard_enum_match_arm)]` on those functions. Until then check 2 holds rule 5.

Held by review, with the tests rule 12 requires: rule 3 (per-gate receiver tests), rule 4,
rule 6 (the link's idle and timeout socket tests), rule 8 (one signed door), rule 9 (audit
names and outcomes), rule 10 (idempotent steps).

## Upstream sources

- axum, testing WebSockets:
  https://github.com/tokio-rs/axum/blob/main/examples/testing-websockets/src/main.rs. It
  shows the integration form: bind port 0, `axum::serve`, then
  `tokio_tungstenite::connect_async`. The hub's `hosted_with` harness is that form, and
  `machines/link/tests/socket.rs` uses it.
- axum, WebSockets with ConnectInfo:
  https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs. The hub uses
  `into_make_service_with_connect_info` (`serve/mod.rs`). The example splits send and
  receive into two tasks. The repo uses one `select!` loop with mpsc for answers and control,
  so idle, revocation and send-timeout checks live in one place. That departure is deliberate.
- serde container attributes: https://serde.rs/container-attrs.html. `deny_unknown_fields`
  errors on any unknown field, even when its value is null. That is why an added field must be
  skipped when None (rule 2).
- serde field attributes: https://serde.rs/field-attrs.html. `default` plus
  `skip_serializing_if = "Option::is_none"` lets old peers read the frame and keeps the field
  off their wire.
- serde variant attributes: https://serde.rs/variant-attrs.html. `#[serde(other)]` is allowed
  only on a unit variant of an internally or adjacently tagged enum. An `Unknown` fallback
  would help only links built after it was added; released links have none. So the repo gates
  sends on the hello instead (rule 3).
- tungstenite `WebSocketConfig`:
  https://docs.rs/tungstenite/latest/tungstenite/protocol/struct.WebSocketConfig.html. The
  defaults are 64 MiB per message and 16 MiB per frame. Both sides set 1 MiB from the shared
  constant, because the input is unauthenticated and a call carries one broker request.
- RFC 6455, section 5.5.2: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.2. A Ping serves
  as a keepalive or to verify the peer is still responsive. The hub counts Ping and Pong as
  activity; the link Pings on its heartbeat and requires inbound traffic within the idle
  window. tungstenite answers pings automatically, so no protocol change was needed.
- ed25519-dalek `VerifyingKey`:
  https://docs.rs/ed25519-dalek/latest/ed25519_dalek/struct.VerifyingKey.html. `verify_strict`
  performs the malleability checks, and `is_weak` flags low-order keys. `admit` uses both,
  tested in `weak_keys_are_refused_even_with_a_valid_code`.
- tokio `Semaphore`: https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html.
  `try_acquire_owned` gives a permit that moves into a task: the handshake cap and the link's
  call and wait pools.
- Backoff and jitter: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/.
  The link adds up to 25% on top of a capped doubling (`run/mod.rs` `run` and `jitter`), a
  floor-keeping variant. Enough for a one-user hub with a few machines; revisit for a fleet.
- Docker Compose services: https://docs.docker.com/reference/compose-file/services/.
  `service_completed_successfully` and `restart: "no"` (`compose.yml`), together with
  `already_enrolled`, make a second `up -d` harmless.
- clippy `match_wild_enum`:
  https://github.com/rust-lang/rust-clippy/blob/master/clippy_lints/src/matches/match_wild_enum.rs.
  It returns early on `Option` and `Result` scrutinees, so it cannot see an `Ok(_)` arm
  (Mechanical check 3).

## Known violations

None open. Every violation this pattern listed at main 2b3feec was fixed in B2 (increment 271,
[plan](../dev/plans/link-and-join-b2.md)); the plan says what each became. A new one goes
here, with its file, function and fix.
