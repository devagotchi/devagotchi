# Pattern: the machine link

How a machine and its hub talk, and how each step of a machine's life is added. Follow it for
every new frame, hello field, gate or lifecycle step.

This pattern states only what is particular to the link. The shared rules G1 to G14 are in
[README.md](README.md) and hold here too. Where a rule below says "G<n>", the shared
rule is the base and this file adds the link's specifics. Layout comes from
[layout.md](layout.md). The moves still pending are step S10 of
[the structure plan](../dev/plans/structure-cleanup.md).

A self-hosted, one-user hub on Linux reaches the user's own Windows and Linux machines, and
machines in Docker, through one outbound WebSocket. Each machine's `devagotchi link` holds it
open.

```text
AI client -> hub /mcp -> Links::call -> HubFrame::Call -> /link <-wss- devagotchi link -IPC-> broker
```

## The parts

| Part | Home today |
|---|---|
| The wire: frames, sizes, versions, signed bytes, words both sides compare | `crates/contracts/src/lib.rs` (the block from the doc comment above `LINK_PROTOCOL_VERSION` to `link_signing_message` and its test module, lines 402-494) |
| Machine side: CLI, enroll, run and backoff, session, update, handoff | `crates/link/src/` |
| Hub side: door, handshake and admit, session loop, calls, updates, presence, registry | `crates/hub/src/machines/link/`, `crates/hub/src/machines/routes.rs`, `crates/hub/src/machines/update.rs` |
| The AI's verbs `machine_add` and `machine_update` | tools in `crates/hub/src/mcp/tools/machines.rs`, logic in `crates/hub/src/mcp/machines.rs` (`update_machine`). Both move to `crates/hub/src/machines/manage.rs` (see Where files go). |
| Join: the one line (sh, ps1, compose) and the installers it downloads | `crates/hub/src/machines/join/`, `deploy/linux/install-devagotchi.sh`, `deploy/windows/install-devagotchi.ps1` |
| Build name | `crates/devagotchi/build.rs` (`DEVAGOTCHI_BUILD`). The hub offers its downloads' version (`crates/hub/src/serve/mod.rs`, the `.offering(..)` call). |

Versions are on three axes, each with one job:

- `LINK_PROTOCOL_VERSION` (1) must match exactly in the hello (`enroll.rs` `admit`, lines
  70-75). It is part of the signed bytes (`link_signing_message`).
  - A hub that refuses the version never holds a session for that machine. `machine_update`
    then cannot reach it (`updates.rs` `ask_update`, lines 54-60), and the link exits 2
    (`crates/link/src/lib.rs` `run` and `dispatch`).
  - **Bumping the version strands every machine until someone reinstalls it by hand.**
- `API_VERSION` (1) is checked by the link on every `call` (`session.rs` `handle`, lines
  230-238).
- The build (`main-<sha7>`, `ci-<sha7>`, `X.Y.Z`, `dev`) is a capability signal. Only a session
  whose hello carried one may be sent `update` (`updates.rs` `ask_update`, lines 64-71).

## Exemplar

Two exemplars, both on main at 2b3feec.

**Change shape: the container `kind` slice** (increment 209, merged in #258). It is the one
protocol change that touches every layer with its own test, and the one that uses serde's
forward-compatible field idiom.

- Contracts: `LinkFrame::Hello.kind` (`crates/contracts/src/lib.rs:458-461`). Test
  `hello_carries_its_build_and_an_old_hello_without_one_still_parses`
  (`crates/contracts/src/link_frame_tests.rs:3-35`): absent when None, parsed when present,
  the old shape still parses.
- Link: `KIND_ENV` (`crates/link/src/lib.rs:26-28`), `run` (line 206), `Updater::kind` and
  `Updater::request` (`crates/link/src/update.rs:77-80`, `97-103`). Test
  `a_container_refuses_every_update_and_says_so` (`crates/link/src/update_tests.rs:126-149`).
- Hub: `admit` (`crates/hub/src/machines/link/enroll.rs:69`), `Session.container`
  (`crates/hub/src/machines/link/mod.rs:50-51`), `ask_update` and `auto_update_now`
  (`crates/hub/src/machines/link/updates.rs:61-63`, `168-176`). Test
  `a_container_is_never_asked_to_update_and_is_told_how_to_move`
  (`crates/hub/src/machines/link/tests/container.rs:30-89`), which asserts on the device's
  receiver that nothing was sent.
- Join: `crates/hub/src/machines/join/compose.yml:46` and its test in `compose.rs` (line 187).
- Real OS: `tests/compose_join.py` `docker_leg`: `machine_update` on the Docker machine is
  refused as a container's, and a second `docker compose up -d` leaves it online.

**Loop shape: the hub session**, `Links::serve` (`crates/hub/src/machines/link/serve.rs:83-274`).

- Bounded before trust: the handshake permit (`routes.rs` `link_upgrade`, lines 31-37), a hello
  of at most 4 KiB within 10 s (`serve.rs:18-21`, `98-103`), every write within 10 s (`send`,
  lines 60-74).
- Exhaustive dispatch with no wildcard (lines 232-243).
- Each exit is a named reason, audited as `link.disconnect` (lines 199-273).
- Redacted wire refusals (`wire_refusal`, lines 33-49) with an itemized, rate-limited audit
  (`refused`, lines 323-354).
- The link's loop (`crates/link/src/session.rs` `connect`, lines 156-211) has the same shape
  with two defects: a wildcard arm, and no read deadline or write timeout (Known violations).

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

On the hub, the hello field becomes a `Session` field, the gate refuses with a code and a
reason, and the send goes through `control` (`enroll.rs:69`, `mod.rs:43-54`,
`updates.rs:47-93`):

```rust
let session = self.sessions.lock().expect("links").get(device).cloned().ok_or_else(|| offline(device))?;
if !session.new_field {
    return Err(BridgeError::new("<x>_unsupported", "<why, and what the person can do>"));
}
session.control.try_send(HubFrame::NewThing { .. }).map_err(|_| offline(device))?;
self.event(subject, via, "machine.<x>", device, .., &outcome).await;
```

A dispatch decodes, then names every variant of the frame enum (`serve.rs:232-243`):

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
   - For the link that means: message sizes, the heartbeat and idle-timeout pair, the update
     states, the refusal word `unavailable`, machine kinds and the build-name rule.
   - Precedents: `MAX_LINK_MESSAGE_BYTES`, `UPDATE_STATES`.
   - The refusal words only the hub writes (`refused`, `protocol_version`, `device_revoked`) may
     stay hub literals.
2. **Evolve within protocol 1, additively (G2).**
   - Never bump `LINK_PROTOCOL_VERSION` or change `link_signing_message` without an operator
     decision recorded in a plan.
   - Every shape a released peer sends keeps parsing, including today's `null`s. The golden list
     (Mechanical check 1) is append-only.
   - Why additive in alpha, when `.ai/README.md` rules out compatibility layers: an enrolled
     machine cannot be fixed from the hub. A machine that stops parsing its hub needs a person
     at it to reinstall. This is the only reason, and it covers only the link wire.
3. **Hubs update first, and the hub sends a post-v1 `HubFrame` only to a session that said it
   understands it.**
   - The hub learns this from the hello and keeps it on `Session`
     (`docs/spec-machine-update.md`, section U2).
   - The gate refuses with a code and a reason the person can act on (`update_unsupported`,
     `updates.rs:64-71`).
   - Without the gate, an old link cannot parse the unknown frame, so it drops the connection and
     reconnects (`session.rs:185`).
4. **The link sends a post-v1 `LinkFrame` only as an answer to a hub frame, or to a hub that
   updates first.** `UpdateStatus` answers `Update` (`session.rs:179-183`). The handoff outcome
   follows an `Update` the previous session got (`session.rs:149-153`).
5. **Dispatch is exhaustive (G5).**
   - The link's dispatches are the two handshake matches and the serve loop in `session.rs`
     `connect`. The hub's is the session loop in `serve.rs` `Links::serve`.
   - Extracting one variant (`if let`, `let ... else`, as in `serve.rs:104-111` and
     `enroll.rs:57-68`) is a decoder, not a dispatch, and may ignore the rest (G5).
6. **Bound everything before trust, and every read and write after it, on both sides (G6).**
   - Hub, before trust: 32 handshakes, 503 at the door (`mod.rs:36`, `routes.rs:31-33`). A hello
     of at most 4 KiB within 10 s (`serve.rs:18-21`). 1 MiB per message on both sides
     (`MAX_LINK_MESSAGE_BYTES`, `routes.rs:35-36`, `session.rs:73-75`).
   - Hub, after trust: every write within 10 s (`serve.rs` `send`), 60 s idle (`serve.rs:24`),
     64 queued calls per device (`mod.rs:39`).
   - Link: 16 calls and 8 waits at once, `link_busy` beyond them (`session.rs:27-32`).
   - A new step that reads or writes the socket uses its side's helpers (`serve::send`,
     `session::send` and `next_frame`), never its own.
7. **Refusals say little on the wire and everything in the audit (G4).**
   - A handshake refusal says only `refused`, `unavailable` (retry) or `protocol_version`
     (`serve.rs` `wire_refusal`). The reason is audited at most once a second, with a count of
     the rest (`serve.rs` `refused`).
   - A device that already proved its key and is then revoked is told `device_revoked`
     (`serve.rs` `revoked_frame`).
   - The link treats `unavailable` as Retry. Any other refusal is permanent: exit 2, so
     supervisors stop (`session.rs` `refusal`, `lib.rs` `dispatch`).
8. **A key is bound only by proving it (G8).**
   - Binding happens in a hello signed over `link_signing_message(hub, device, nonce)`. Weak keys
     are refused, then the signature is checked with `verify_strict` (`enroll.rs:94-107`).
   - The enroll code is a secret under G8. Every program on the join path reads it from the
     environment or stdin, never from argv (`crates/link/src/lib.rs` `USAGE` and
     `enrollment_code`, `join.sh:33`, `join.ps1:37`, `compose.yml:36`).
9. **Every lifecycle transition is audited (G7)** under a dotted name:
   - `link.connect`, `link.disconnect` (with its reason), `link.refused`, `link.enroll`,
     `link.code_issued`, `link.join_script`, `link.revoke`, `machine.update`,
     `machine.update_status`.
   - A new step adds its name. It never records the code, and records a key only as its
     fingerprint.
10. **Lifecycle steps are idempotent and say what happened.**
    - `enroll` again with the same hub, device and key succeeds with no network call
      (`lib.rs` `already_enrolled`).
    - A reconnect replaces the old session (`serve.rs:140-151`).
    - A second update request while one runs returns the current status (`update.rs`
      `Updater::request`).
    - A step that hands off to something that outlives the link records it in the state dir,
      and the next link reports it once (`handoff.rs` `pending` and `outcome`).
11. **Join artifacts are templates.**
    - Each value is filled only after a strict validator passes. It is quoted for its format:
      single quotes in sh and ps1 (`join/mod.rs:9-11`). In compose, every value passes a strict
      charset check first, so it is safe in YAML whether or not the template quotes it
      (`compose.rs` `file`).
    - Every placeholder is filled. The code appears exactly once and never on a command line.
    - Neither a template nor an installer it downloads carries a machine- or person-specific
      path or default.
    - A compose refusal is never a 200 (`compose.rs` `serve`). A dead or unknown code on the sh
      or ps1 line gets a 200 script that only fails with the reason (`join/mod.rs` `refusal`).
12. **Every change lands with its test layers** (The canonical test).
    - A gate's test asserts on the device's receiver that nothing was sent.
    - A refusal's test asserts the exact code.

## Where files go

G10 holds. The moves below are the S10 step of the structure plan.

- `crates/contracts/src/link/mod.rs`: the frames, constants and signing bytes. `lib.rs`
  re-exports them flat with `mod link; pub use link::*;`, as it does for `files` (G10's
  contracts exception). Tests go in `link/tests.rs`.
- `crates/link/src/<step>/mod.rs`:
  - `session/` (`session.rs` becomes `session/mod.rs`).
  - `update/`, with `handoff.rs`, `idle.rs`, `update-windows.ps1` and `tests.rs`.
  - `run/`, for enroll, run and backoff.
  - `lib.rs` keeps `USAGE`, `parse_args` and `cli`, and the `parse_args` tests.
- `crates/hub/src/machines/link/<job>.rs`, one job per file: `serve`, `enroll`, `calls`,
  `updates`, `presence`, `registry`.
  - Tests go in `machines/link/tests/<subject>.rs`.
  - `tests/mod.rs` holds the shared fixtures `test_links`, `hello` and `Session::fake`.
- `machine_add`, `machine_update` and `machines` go to `crates/hub/src/machines/manage.rs`
  (layout rule 7: the domain serves its tools). `crates/hub/src/mcp/` keeps the `Hub` surface.
- Join templates stay in `crates/hub/src/machines/join/`. Its tests move to
  `machines/join/tests.rs`, because `join/mod.rs` is a front door near the cap.
  `join/compose.rs` and `machines/update.rs` keep their inline tests while under the cap (G10).

## Adding a frame or a hello field

1. Contracts: the variant or field (rule 2), any constants (rule 1), and its golden lines
   (Mechanical check 1). Golden lines are appended, never edited.
2. Hub: read the capability in `admit` and keep it on `Session`. Add the dispatch arm
   (rule 5), the gate (rule 3) and the audit name (rule 9).
3. Link: the dispatch arm, the sender, and the state the frame changes.
4. Tests: the layers below. CI runs them with `cargo nextest run --locked --profile ci` under
   G13: the `core` job (`.github/workflows/ci.yml:67`), and the `hub-postgres` job's
   `cargo nextest run` step for the hub tests that need Postgres.

## Adding a lifecycle step

- Name its trigger: hello, a call, a control frame, a timer or a CLI verb.
- Give it an idempotent second run (rule 10), an audit name (rule 9), and a refusal code with a
  reason (G4).
- Add its line to `tests/e2e.py`'s link steps. A step about installing or joining goes in
  `tests/compose_join.py` or `deploy/*/ci-install-check.*` instead.

## The canonical test

1. **Wire golden** (`crates/contracts`).
   - Every shape a released peer sends parses.
   - A None optional added after v1 is absent from the JSON.
   - `link_signing_message` is pinned byte for byte.
   - Exemplar: `link_frame_tests.rs:3-35`.
2. **Hub unit** (`machines/link/tests/`).
   - Runs on `Links` over a fresh Postgres database, taken as `or_skip!(test_links())` (G13).
   - Builds the session with `Session::fake(..)`, which returns the `calls` and `control`
     receivers.
   - Asserts the code, and what the receiver did or did not get.
   - Exemplars: `tests/container.rs:30-89`;
     `replays_other_hubs_other_keys_and_revoked_devices_are_refused` (`tests.rs:193-230`).
3. **Hub socket**, for a new loop arm.
   - The whole router on a real port with ConnectInfo: the
     `tenancy::tests::tenants::hosted_with` harness
     (`crates/hub/src/tenancy/tests/tenants.rs`, `into_make_service_with_connect_info` at
     line 162).
   - Plays the device with `tokio-tungstenite` (to be added as a hub dev-dependency) and signs
     with `link_signing_message`.
4. **Link unit** (`crates/link/src/<step>/tests.rs`).
   - Exercises the step with a fake broker (`update_tests.rs:14-72`) or none
     (`update_tests.rs:126-149`). No link test fakes the hub yet.
   - A dispatch arm gets a fake hub on `127.0.0.1:0` with `tokio_tungstenite::accept_async`.
     `state::endpoint` allows loopback `http`.
5. **Real binaries.**
   - `tests/e2e.py` link steps (lines 259-296), run by the `e2e` job in
     `.github/workflows/ci.yml` on ubuntu-24.04, ubuntu-24.04-arm, windows-2025 and
     windows-2022. windows-11-arm runs `e2e.py --no-hub`, which skips them. A step is a
     `steps.append(run_probe(..))` or `steps.append(run_cmd(..)[0])` line with its expected
     code or exit.
   - `tests/compose_join.py` (`image.yml:148`): the join lines exactly as `machine_add` gives
     them.
   - `deploy/windows/ci-install-check.ps1` and `deploy/linux/ci-install-check.sh`: the ps1 and
     sh lines through the real installers.
   - `tests/machine_update_probe.py`: a real update between two builds (`none` from e2e on
     Linux; `windows` and `linux` from the install checks).

## Anti-patterns

- An `Ok(_)` or `_` arm in a frame dispatch (G5). On the link a new variant then becomes a
  reconnect loop instead of a compile error.
- Sending a new `HubFrame` to every session, or checking a gate on one side only.
- A word or rule both crates use, written in each (G3): `"container"`, `"unavailable"`, the
  update states, `valid_version`.
- A field added to an existing frame with only `#[serde(default)]` (G2). It puts `null` on the
  wire, which an older strict peer refuses.
- Bumping `LINK_PROTOCOL_VERSION` to ship a feature.
- A second door that binds a key without a signature.
- An enroll code in argv, in a help example or in a message that tells a person to pass it.
- A socket read with no deadline, or a write with no timeout (G6).
- Building `Session { .. }` by hand in a test.
- A personal path or default in a join template or an installer.

## The mechanical check

Three checks hold rules 1, 2, 5 and 11. None needs a database. CI runs them through its existing
`cargo nextest run`. Each lands green together with the fixes it guards (G12). They are proposals:
none was compiled or run.

**1. Wire golden**, `crates/contracts/src/link/tests.rs` (today `link_frame_tests.rs`).

```rust
/// Every frame shape a released peer sends, oldest first. APPEND ONLY: a peer that sends an
/// old line may still be out there (.ai/patterns/link.md rule 2). Never edit or delete a line.
const SEEN: &[&str] = &[
    r#"{"type":"challenge","nonce":"n","hub":"https://h"}"#,
    r#"{"type":"welcome","device":"d"}"#,
    r#"{"type":"refused","code":"refused","message":"m"}"#,
    r#"{"type":"call","id":1,"request":{"api_version":1,"request":{"method":"broker_capabilities","params":{}}}}"#,
    r#"{"type":"update","version":"main-abc1234","force":false}"#,
    // v1 links (before builds had names)
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":null}"#,
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":"dvg-x"}"#,
    // U2 links: enroll sends build null, serve sends a build
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":"dvg-x","build":null}"#,
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":null,"build":"main-abc1234"}"#,
    // increment 209: a container says its kind
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":null,"build":"main-abc1234","kind":"container"}"#,
    r#"{"type":"result","id":1,"response":{"status":"ok","value":{"kind":"revision","value":{"revision":7}}}}"#,
    r#"{"type":"result","id":1,"response":{"status":"error","value":{"code":"link_busy","message":"m"}}}"#,
    r#"{"type":"heartbeat"}"#,
    r#"{"type":"update_status","state":"downloading","detail":null}"#,
    r#"{"type":"update_status","state":"failed","detail":"busy"}"#,
];
// No wildcard arms: a new variant does not compile until it is named here, and the test then
// fails until SEEN carries a line for it.
fn hub_name(f: &HubFrame) -> &'static str { match f { HubFrame::Challenge { .. } => "challenge", HubFrame::Welcome { .. } => "welcome", HubFrame::Refused { .. } => "refused", HubFrame::Call { .. } => "call", HubFrame::Update { .. } => "update" } }
fn link_name(f: &LinkFrame) -> &'static str { match f { LinkFrame::Hello { .. } => "hello", LinkFrame::Result { .. } => "result", LinkFrame::Heartbeat => "heartbeat", LinkFrame::UpdateStatus { .. } => "update_status" } }
const ALL: [&str; 9] = ["call", "challenge", "heartbeat", "hello", "refused", "result", "update", "update_status", "welcome"];

#[test]
fn every_frame_a_released_peer_sends_still_parses_and_every_variant_has_a_line() {
    let mut seen = std::collections::BTreeSet::new();
    for line in SEEN {
        let name = serde_json::from_str::<HubFrame>(line).map(|f| hub_name(&f))
            .or_else(|_| serde_json::from_str::<LinkFrame>(line).map(|f| link_name(&f)))
            .unwrap_or_else(|e| panic!("a released peer's frame no longer parses: {line}: {e}"));
        seen.insert(name);
    }
    assert_eq!(seen, ALL.into_iter().collect(), "add a SEEN line for every new variant");
}

#[test]
fn the_signed_bytes_and_the_protocol_never_change() {
    assert_eq!(LINK_PROTOCOL_VERSION, 1, "bumping strands every machine: .ai/patterns/link.md rule 2");
    assert_eq!(link_signing_message("https://h", "d", "n"), b"devagotchi-link/1\nhttps://h\nd\nn".to_vec());
}

#[test]
fn fields_added_after_v1_stay_off_the_wire_when_none() {
    let hello = LinkFrame::Hello {
        version: 1, device: "d".into(), public_key: "ab".into(), signature: "cd".into(),
        enroll_code: None, build: None, kind: None,
    };
    let text = serde_json::to_string(&hello).unwrap();
    assert!(!text.contains("\"build\"") && !text.contains("\"kind\""), "{text}");
}
```

Beside the constants, a const assertion holds the liveness pair (G6), once
`LINK_HEARTBEAT` and `LINK_IDLE_TIMEOUT` exist:
`const _: () = assert!(LINK_HEARTBEAT.as_secs() * 3 <= LINK_IDLE_TIMEOUT.as_secs());`

What check 1 catches:
- A new required field fails the old hello lines.
- A renamed tag or field fails its line.
- A new variant without a line fails the set comparison.
- A new variant not named in `hub_name` or `link_name` does not compile.
- A change to the signed bytes fails outright.

**2. Source scan**, `crates/devagotchi/tests/link_protocol.rs`, sharing the walker of
`crates/devagotchi/tests/layout.rs` (`rust_files`, lifted into a shared module; G12). It needs
no allow-list. Test files (`tests.rs`, `*_tests.rs`, anything under a `tests/` folder) are not
scanned, and neither is text after a file's first `#[cfg(test)]` line. Without the first
exclusion the scan would flag `update_tests.rs:16` and `135` and `machines/link/tests.rs:319`,
which have no `#[cfg(test)]` line of their own. It fails when:
- `crates/link/src/session.rs` (or `session/mod.rs`) or
  `crates/hub/src/machines/link/serve.rs` contains `Ok(_) =>` or `Ok(_) |` (rule 5). Today
  that is exactly `session.rs:98`, `126` and `185`. `session.rs:268` `Ok(Some(Ok(_)))` matches
  a `Message`, not a frame, and the pattern does not catch it.
- A Rust file under `crates/link/src`, `crates/hub/src/machines/link` or
  `crates/hub/src/machines/join` holds the literal `"container"`, `"unavailable"`,
  `"waiting_for_jobs"`, `"downloading"` or `"installing"` (rule 1). `done` and `failed` are too
  common to scan for; review holds them.
- `join.sh`, `join.ps1`, `compose.yml`, `deploy/windows/install-devagotchi.ps1` or
  `deploy/linux/install-devagotchi.sh` contains `localdev` or a drive-letter path such as
  `D:\` (rule 11). These five are named, not walked.

**3. Clippy, once CI runs it.** `clippy::wildcard_enum_match_arm` does not apply today. It
ignores `Result` and `Option` scrutinees, and no workflow runs clippy. To use it, decode first and
then `match frame { .. }` on the enum itself, and put `#[deny(clippy::wildcard_enum_match_arm)]` on
those functions. Until then check 2 holds rule 5.

Held by review, with the tests rule 12 requires: rule 3 (per-gate receiver tests), rule 4,
rule 6 (the link's idle and timeout socket tests), rule 8 (one signed door), rule 9 (audit
names), rule 10 (idempotent steps).

## Upstream sources

- axum, testing WebSockets:
  https://github.com/tokio-rs/axum/blob/main/examples/testing-websockets/src/main.rs. It
  shows the integration form: bind port 0, `axum::serve`, then
  `tokio_tungstenite::connect_async`. The hub already has that harness (`hosted_with`), so the
  missing socket test is a gap, not a choice.
- axum, WebSockets with ConnectInfo:
  https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs. The hub uses
  `into_make_service_with_connect_info` (`serve/mod.rs:344`). The example splits send and
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
  activity. The link should Ping on its heartbeat and require inbound traffic within the idle
  window. tungstenite answers pings automatically, so no protocol change is needed.
- ed25519-dalek `VerifyingKey`:
  https://docs.rs/ed25519-dalek/latest/ed25519_dalek/struct.VerifyingKey.html. `verify_strict`
  performs the malleability checks, and `is_weak` flags low-order keys. `admit` uses both,
  tested in `weak_keys_are_refused_even_with_a_valid_code`.
- tokio `Semaphore`: https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html.
  `try_acquire_owned` gives a permit that moves into a task: the handshake cap and the link's
  call and wait pools.
- Backoff and jitter: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/.
  The link adds up to 25% on top of a capped doubling (`lib.rs` `run` and `jitter`), a
  floor-keeping variant. Enough for a one-user hub with a few machines; revisit for a fleet.
- Docker Compose services: https://docs.docker.com/reference/compose-file/services/.
  `service_completed_successfully` and `restart: "no"` (`compose.yml:31-56`), together with
  `already_enrolled`, make a second `up -d` harmless.
- clippy `match_wild_enum`:
  https://github.com/rust-lang/rust-clippy/blob/master/clippy_lints/src/matches/match_wild_enum.rs.
  It returns early on `Option` and `Result` scrutinees, so it cannot see the link's `Ok(_)` arms
  (Mechanical check 3).

## Known violations

Each was re-read at main 2b3feec.

### Rule 1: compared values live in contracts (G3)

- `crates/link/src/session.rs` (`HEARTBEAT`, line 26) and
  `crates/hub/src/machines/link/serve.rs` (`IDLE_TIMEOUT`, line 24): liveness needs the heartbeat
  well under the idle timeout, and the two are tied only by comments and `crates/link/README.md`
  -> add `LINK_HEARTBEAT` (20 s) and `LINK_IDLE_TIMEOUT` (60 s) to contracts with the const
  assertion from Mechanical check 1, and use them on both sides and for the link's own idle check
  (rule 6 below).
- `crates/hub/src/machines/link/enroll.rs` (`admit`, line 69), `crates/link/src/lib.rs` (`run`,
  line 206), `crates/link/src/update.rs` (`Updater::kind`, line 79): the machine kind
  `"container"` is a literal in both crates, and in `compose.yml:46`. A typo on one side
  compiles and turns off the container gate -> `pub const MACHINE_KIND_CONTAINER` in contracts
  at the three sites. The `compose.rs` test asserts
  `format!("DEVAGOTCHI_MACHINE_KIND: {MACHINE_KIND_CONTAINER}")`.
- `crates/link/src/session.rs` (`refusal`, line 216) and `crates/hub/src/machines/link/serve.rs`
  (`wire_refusal`, line 37): the link branches on the literal `"unavailable"` that the hub
  writes. If they drift, a store hiccup becomes exit 2 and supervisors stop the machine ->
  `pub const REFUSAL_UNAVAILABLE` in contracts, used at both sites.
- Update states written as literals: `crates/link/src/update.rs` (`Updater::request` and
  `Updater::run`, lines 100, 119, 124, 137, 146, 150, 152), `crates/link/src/handoff.rs`
  (`outcome`, lines 51, 71), `crates/hub/src/machines/link/updates.rs` (`update_reported` and
  `connected_on`, lines 120, 129, 157), `crates/hub/src/machines/update.rs` (`Update`, lines
  42, 52), `crates/hub/src/mcp/machines.rs` (`update_machine`, line 111). Nothing ties them to
  `UPDATE_STATES`, and the hub silently drops an unknown state (`update_reported`, lines
  112-114) -> `UPDATE_WAITING`, `UPDATE_DOWNLOADING`, `UPDATE_INSTALLING`, `UPDATE_DONE`,
  `UPDATE_FAILED` in contracts, with `UPDATE_STATES` built from them. `"requested"`
  (`machines/update.rs:34`, `mcp/machines.rs:118`) is hub-only and becomes one const in
  `machines/update.rs`.
- `crates/link/src/update.rs` (`valid_version`, lines 240-251) and
  `crates/hub/src/machines/downloads.rs` (`valid_version`, lines 72-82): one rule copied byte for
  byte, and `crates/hub/src/machines/join/compose.rs` (`image`, lines 32-36) re-derives part of
  it -> `pub fn valid_build_name(v: &str) -> bool` in contracts, called from all three; delete
  the copies.

### Rule 2: additive fields (G2)

- `crates/contracts/src/lib.rs` (`LinkFrame::Hello.build`, lines 456-457): added after v1 with
  only `#[serde(default)]`, so every enroll hello sends `"build":null` (`session.rs:111-114`).
  A hub from before U2 refuses it under `deny_unknown_fields` -> add
  `skip_serializing_if = "Option::is_none"`, and add
  `fields_added_after_v1_stay_off_the_wire_when_none`. Keep the golden lines with
  `"build":null` and `"enroll_code":null`, because links in the field send them.

### Rule 5: exhaustive dispatch (G5)

- `crates/link/src/session.rs` (`connect`, lines 98, 126, 185): the link's three dispatches end
  in `Ok(_)`. A new `HubFrame` variant compiles there and turns into a reconnect every 1 to 60 s,
  plus up to 25% jitter -> name the remaining variants:
  - 98: `Ok(HubFrame::Welcome { .. } | HubFrame::Refused { .. } | HubFrame::Call { .. } | HubFrame::Update { .. }) => return Ended::Retry("hub did not open with a challenge".into())`.
  - 126: `Ok(HubFrame::Challenge { .. } | HubFrame::Call { .. } | HubFrame::Update { .. }) => ...`.
  - 185: `Ok(HubFrame::Challenge { .. } | HubFrame::Welcome { .. }) | Err(_) => ...`.

### Rule 6: bounded reads and writes (G6)

- `crates/link/src/session.rs` (`connect` serve loop, lines 156-211; `send`, lines 276-284): no
  read deadline and no write timeout. The hub sends nothing when idle, and the link's heartbeat
  writes still land in the socket buffer. A hub that vanishes silently (NAT timeout, sleep,
  network change) is noticed only when TCP gives up, while the hub already shows the machine
  offline. Plausible from the code; the exact time depends on OS TCP settings -> add
  `let mut idle = Box::pin(tokio::time::sleep(LINK_IDLE_TIMEOUT))` and reset it on any inbound
  message, Pong included; send `Message::Ping` on each heartbeat tick; add the arm
  `_ = &mut idle => return Ended::Retry("hub silent for 60s".into())`; wrap `sink.send` in
  `tokio::time::timeout(Duration::from_secs(10), ..)` as `serve.rs` `send` does. Test with a
  fake hub that accepts and then never reads or writes, on tokio's paused time.

### Rules 7 and 12: refusal codes tested exactly

- `crates/hub/src/machines/link/tests.rs` (`the_wire_says_only_refused_unless_retrying_can_help`,
  lines 299-320) and `enroll.rs` (`admit`, lines 70-75): the table never checks that
  `protocol_version` passes through, that `audit_write` becomes `unavailable`, or that
  `early_access` becomes `refused`. No test sends a hello with another version -> extend the
  table in its existing destructuring form, and add an `admit` test whose hello has
  `version: LINK_PROTOCOL_VERSION + 1` and expects the code `protocol_version`.

### Rule 8: a key is bound only by proving it (G8)

- `crates/hub/src/machines/routes.rs` (`enroll_with_code`, `EnrollBody`, lines 40-80) and
  `crates/hub/src/machines/link/registry.rs` (`enroll_owned`): `POST /enroll` binds any
  `public_key` to a live code with no signature, a second door beside the signed hello. Harm is
  limited: a later hello must still sign with the bound key, so the damage is a burned code. Its
  callers are 7 calls in 6 hub test files (`agenthub/mcp/tests/tickets.rs:31`,
  `machines/tests/inspect.rs:80`, `ops/tests.rs:318`, `tenancy/tests/access.rs:153` and `199`,
  `tenancy/tests/tenants.rs:499`, `web/app/tests.rs:92`) -> operator decision D3, default
  (b): remove the route, its `PRIVATE` entry (`crates/hub/src/site/discovery.rs:43`) and the
  `tenancy/access/mod.rs:326` doc mention, note it in `CHANGELOG.md` (which lists it at line
  1082 as shared by every mode), and have the tests call `links.enroll_owned(..)`. The
  alternative (a) is to verify a signature over `link_signing_message(public_url, name, code)`
  as `admit` does.
- `deploy/windows/install-devagotchi.ps1` (`-EnrollCode`: help line 21, example line 35, param
  line 51, message line 348): the installer takes the code on its command line and tells people
  to pass it there, so it shows in the process list. `join.ps1:37` already uses the environment,
  which the installer reads at lines 62-63 -> rewrite the example to set
  `$env:DEVAGOTCHI_ENROLL_CODE` first, make the message name the variable, mark `-EnrollCode`
  deprecated, and remove it once nothing passes it.

### Rule 9: audit (G7)

- `crates/hub/src/machines/link/serve.rs` (`Links::event`): a failed audit write is only printed
  to stderr. G7 also raises an `audit.write_failed` operator signal -> raise it from `event`
  once that signal exists (B1, `Hub::recorded`).
- Outcomes carry detail: `enroll.rs` (`admit`: `ok owner=<owner>`), `updates.rs`
  (`request_update`: `sent force=<bool>`, `already_on`; `update_reported`:
  `<state>: <detail>`), `serve.rs` (`refused`: `<code> (<n> more refusals not itemized)`),
  `registry.rs` (`revoke_owned`: `revoked=<bool> codes_cancelled=<n>`; `join_script`: the way,
  `sh`, `powershell` or `compose`). G7's outcome is `ok`, `running` or a code -> outcome `ok`
  (`done` and `already_on` included) or the code (`update_failed` for a failed update, the word
  `push_machine` already uses), with the owner, force, detail, suppressed count, revoke counts
  and way moved into the target.

### Rule 11: join templates

- `crates/hub/src/machines/join/join.ps1` (lines 33-34): the Windows join script every user gets
  installs into `D:\localdev\devagotchi` whenever `D:\localdev` exists, with a comment naming the
  maintainer -> `$root = if ($env:DEVAGOTCHI_ROOT) { $env:DEVAGOTCHI_ROOT } else { Join-Path $env:LOCALAPPDATA 'Devagotchi' }`,
  drop the comment, and add `assert!(!ps.contains("localdev"))` to the `join/mod.rs` script
  test.
- `deploy/windows/install-devagotchi.ps1` (`$Root` default line 42 and help line 7;
  `$Workspace` default line 270; example line 33): a direct install defaults to
  `D:\localdev\devagotchi`, and every install whose caller passes no `-Workspace` (including
  `join.ps1`) uses `D:\localdev` as the workspace when it exists -> default `$Root` to
  `Join-Path $env:LOCALAPPDATA 'Devagotchi'` and `$Workspace` to `Join-Path $Root 'workspace'`,
  and use a neutral path in the example. `ci-install-check.ps1` already passes `-Root` and
  `-Workspace`.

### Rule 12: test layers

- `crates/contracts/src/link_frame_tests.rs` (whole file): only Hello, Update and UpdateStatus
  have wire tests. Nothing pins `link_signing_message`, and every hub test signs through the same
  function, so an accidental edit passes every Rust test and strands every machine -> Mechanical
  check 1.
- `crates/hub/src/machines/link/updates.rs` (`ask_update`, lines 47-74): only the container gate
  is tested. `update_unsupported` for a hello without a build, `AlreadyOn` and `no_build` are
  not -> three tests in `tests/updates.rs`, each asserting the code (or `AlreadyOn`) and
  `control.try_recv().is_err()`.
- `crates/link/src/session.rs` (`connect`, `refusal`): no test reaches `hub_mismatch`.
  Refusal classification, serve-loop dispatch and updates are covered only by `tests/e2e.py`
  (updates through `tests/machine_update_probe.py`). `session/tests.rs` covers only
  `from_a_hub` -> a `#[test]` that `refusal("unavailable", ..)` is Retry and
  `refusal("refused", ..)` is Refused; a fake hub on `127.0.0.1:0` whose challenge names another
  hub gives `hub_mismatch`; a fake that sends Challenge, Welcome, then Update with an invalid
  version gets back an `update_status` `failed` frame.
- `crates/hub/src/machines/link/serve.rs` (`Links::serve`): no socket test. Result routing,
  Heartbeat resetting idle and the revocation tick are covered only by the `tests/e2e.py` link
  steps, and UpdateStatus only by `tests/machine_update_probe.py`. Nothing tests a mid-session
  Hello or Binary ending as `protocol_error` -> add `tokio-tungstenite` as a hub dev-dependency
  and a test on `hosted_with`: enroll with a code from `devices.issue_code`, sign the challenge,
  assert a mid-session Hello ends with a `link.disconnect` row whose outcome is
  `protocol_error`, and assert a revoke sends `device_revoked` and closes within
  `2 * REVOCATION_CHECK`.
- `Session { .. }` built by hand at 5 sites: `crates/hub/src/machines/link/tests.rs`
  (`connect`, line 74; lines 394, 432), `tests/container.rs` (`connect_as`, line 18),
  `crates/hub/src/machines/link/calls.rs` (`connect_for_test`, line 110). A new gate field
  means 5 edits, and a test can leave it unset -> `#[cfg(test)] impl Session { pub(super) fn fake(id: u64, build: Option<&str>, container: bool) -> (Self, mpsc::Receiver<(u64, WireRequest)>, mpsc::Receiver<HubFrame>) }`
  in `machines/link/mod.rs`, used at all 5.
- `crates/hub/src/machines/link/tests.rs` (`links!`, lines 16-24; `test_links`, lines 26-45):
  the macro re-implements `or_skip!`, and `test_links` ignores its directory, so every test makes
  a tempdir for nothing -> `async fn test_links() -> Option<Links>` taken as
  `or_skip!(test_links())` (G13); delete `links!` and the tempdir lines.

### Layout (G10, layout.md), tracked by S10

- `crates/link/src/lib.rs` (`#[path = "update_tests.rs"] mod update_tests;`, lines 251-253) and
  `crates/link/src/update_tests.rs`: a top-level `<x>_tests.rs` -> `crates/link/src/update/`
  with `mod.rs`, `handoff.rs`, `idle.rs`, `update-windows.ps1` and `tests.rs`.
- `crates/link/src/session.rs` beside `crates/link/src/session/`: a domain file beside its own
  folder -> `git mv crates/link/src/session.rs crates/link/src/session/mod.rs`.
- `crates/link/src/lib.rs` (`dispatch`, `enrollment_code`, `already_enrolled`, `enroll`, `run`,
  `jitter`, lines 113-249; inline tests 255-369): logic at the top level (layout rule 4) ->
  `crates/link/src/run/mod.rs` with `run/tests.rs`; `lib.rs` keeps `USAGE`, `parse_args`, `cli`
  and the `parse_args` tests.
- `crates/contracts/src/lib.rs` (lines 402-494) and `crates/contracts/src/link_frame_tests.rs`:
  the wire is inline in a 494-line `lib.rs`, so the next frame crosses the cap, and its tests
  are a top-level `<x>_tests.rs` -> `crates/contracts/src/link/mod.rs` and `link/tests.rs`,
  with `mod link; pub use link::*;` in `lib.rs` (G10's contracts exception).
- `crates/hub/src/machines/link/tests.rs` (475 lines) beside `crates/hub/src/machines/link/tests/`
  -> `tests/mod.rs` with the fixtures (`test_links`, `hello`, `Session::fake`), then one file per
  subject: `presence.rs`, `admit.rs`, `wire.rs`, `calls.rs`, and the existing `container.rs`.
- `crates/hub/src/machines/join/mod.rs` (inline tests, lines 290-482): a front door at 482 of
  500 lines -> `machines/join/tests.rs`.

### Docs name real things (G11)

- `crates/hub/src/machines/link/mod.rs:52`, `crates/hub/src/mcp/machines.rs:139` and
  `crates/hub/src/serve/mod.rs:146` name `machine_update.rs`, which does not exist ->
  `machines/link/updates.rs`.
- `crates/hub/src/machines/update.rs:5` says "`link.rs` sends and receives the frames" ->
  `machines/link/serve.rs` and `machines/link/updates.rs`.
- `crates/contracts/src/lib.rs:403` and `crates/link/README.md:33`, `35` name a
  `devagotchi-link` binary. `crates/link` has only `[lib]`, and the command is `devagotchi link`
  (`crates/devagotchi/src/main.rs:70`) -> `devagotchi link enroll|run`.
- `crates/link/README.md:41-42` says "installer support is the next increment", though both
  installers start the link -> say the installers start it at login, and add an Update section
  pointing to the update code and `docs/spec-machine-update.md`.
- `AGENTS.md:39` says "Contracts belong in `crates/contracts/src/lib.rs`" -> "Contracts belong
  in a family module of `crates/contracts/src/`, re-exported from lib.rs" (the B0 sentence).
