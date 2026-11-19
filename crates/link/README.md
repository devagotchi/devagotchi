# devagotchi link

The machine side of the link (`.ai/patterns/link.md`). It lets a Devagotchi hub reach the broker
on this machine without opening any port here: the link dials out to the hub over one
WebSocket on 443, proves this machine's key, and hands each routed request to the local
broker over private IPC. Jobs stay with the broker and outlive the link, the hub and the
client.

```text
AI client ──▶ hub (/mcp) ──call──▶ /link ◀──outbound WSS── devagotchi link ──IPC──▶ broker
```

## Enroll once, then run

On the hub host, issue a one-time code for the broker name (valid 15 minutes, one use):

```bash
devagotchi hub enroll --state-dir ~/.local/state/devagotchi-hub --config ~/.config/devagotchi/hub.json --device gus
# dvg-3f0c…   (stdout, shown once)
```

The hub config lists the machine as a linked broker and grants it:

```json
"brokers": [ { "name": "gus", "link": true } ],
"grants":  [ { "subject": "<Nomen sub>", "brokers": ["gus"] } ]
```

On the machine, as the same user the broker runs as. The code goes in the environment (or
on stdin), never on the command line where other users could read it in the process list:

```bash
DEVAGOTCHI_ENROLL_CODE=dvg-3f0c… devagotchi link enroll --state-dir ~/.local/state/devagotchi-link \
  --hub https://hub.example --device gus --broker "<the broker's ready-line endpoint>"
devagotchi link run --state-dir ~/.local/state/devagotchi-link
# devagotchi link ready: gus linked to https://hub.example (broker …)
```

`run` reconnects by itself with backoff (1 s doubling to 60 s, up to 25% jitter). Every
`LINK_HEARTBEAT` (20 s) it sends a heartbeat and a WebSocket Ping. Either side gives the
other up after `LINK_IDLE_TIMEOUT` (60 s) without hearing from it: the hub marks the machine
offline, and the link reconnects. The hub answers every Ping, so a hub that went away
without closing is noticed within a minute, not when TCP gives up. Every write times out
after `LINK_SEND_TIMEOUT` (10 s). All three are in `devagotchi_contracts`. It handles at most
16 calls and 8 waits at once and answers `link_busy` beyond that. The installers start it at
login (`deploy/linux/install-devagotchi.sh`, `deploy/windows/install-devagotchi.ps1`), and
the hub's one-line join runs them. Exit code 2 means the hub refused this machine: a supervisor should not restart
it blindly. The hub tells a refused peer only `refused`; the reason (revoked, not enrolled,
wrong key, unknown name) is in the hub's audit log. A hub that briefly cannot check
(`unavailable`) is retried, not treated as a refusal.

## Update

When the hub asks (`machine_update`, or its hourly round when `machines.auto_update` is on),
the link waits for the broker's jobs to end (unless forced), downloads the installer and this
machine's archive from the hub, checks both against the hub's `SHA256SUMS`, and hands off to
the platform's updater, which outlives the link it replaces. It reports each step as an
`update_status` (`UPDATE_STATES` in the contracts); the new link reports how the handoff went.
A machine in a container (`DEVAGOTCHI_MACHINE_KIND=container`) is never asked: it moves with
its image. The code is in `src/update/`; the design in `docs/spec-machine-update.md`.

## Layout

| Path | What it does |
|---|---|
| `src/lib.rs` | The command line: `USAGE`, the arguments, `cli` |
| `src/run/` | `enroll` once, then `run` with its backoff |
| `src/session/` | One connection: the handshake, the serve loop, its deadlines |
| `src/update/` | Updating this machine: the idle wait, the downloads, the handoff |
| `src/state.rs` | The state directory, the key and the saved config |

## Security

| Property | How |
|---|---|
| No inbound port | The link only dials out. Nothing on the machine listens for the hub |
| The key never leaves | Ed25519 seed generated here into `device.key`. The state directory follows the broker's rule (Unix: 0700, ancestors real directories not writable by others; Windows: a protected user-only DACL, no link in its ancestry). The key is opened once without following links, that handle is checked (0600 and one link on Unix, user-only DACL on Windows) and read |
| No replay | The device signs `devagotchi-link/1`, the hub's public URL, the device name and a fresh 32-byte nonce per connection; the hub verifies strictly and refuses weak (small-order) keys |
| Enrollment | Only inside the signed hello; no other door binds a key. A one-time code bound to one device name; the hub stores only its SHA-256, spends it once, expires it after 15 minutes. A newer code supersedes older unused ones; binding a key or revoking cancels every unused code for that name |
| Revocation | `devagotchi hub revoke --device NAME` closes the link within 2 s and is checked again before each queued call is forwarded; new calls answer `device_revoked`; the link exits 2 |
| TLS | `https` hubs only, verified by the operating system's trust store; plain `ws` only for a loopback hub. Hub URLs are parsed: userinfo, queries and fragments are refused |
| Bounded on both sides | The hub's `/link` takes at most 32 handshakes at once, a hello of at most 4 KiB within 10 s, 64 queued calls per device; refusals are audited at most once a second with a count of the rest. On both sides every write times out after 10 s, and 60 s of silence ends the connection |
| The broker still decides | Each call carries one typed broker request; executable grants, workspace roots and deadlines are enforced by the broker as for a local caller |
| Audit | The hub records code issued and revoke (with the operator), enroll, connect and disconnect (with the key fingerprint, peer address and reason), refusals, and every routed call on its hash chain. The code itself is never logged |

## Check

`cargo test -p devagotchi-link` plays a hub on a loopback port: a hub that names another
hub, an update the link cannot take, a silent hub given up, and a hub that only answers
Pings kept. `python3 tests/e2e.py --bin-dir target/debug` runs the whole life of a link against a real
broker and hub: not enrolled, a code that works once, calls through the link, offline when
it stops, back when it restarts, revoked and refused. CI runs it on Linux and both Windows
runners.
