# Dev build on the phone, dev stats, and one-step diagnostics export (spec)

Status: **proposed, waiting on hades's review. Nothing is built until it is approved.**
Issue: not_created. PR: not_created (no PR ceremony on this work, by the user's instruction).

Asked by hades on 2026-11-19: install and test the dev build on the phone, switch on dev
stats, and export everything in one step, in a form an AI agent can process. Written beside
[write_pdf](write-pdf.md), whose shared font pack (§4 there) this spec's UI also uses.

## 1. What exists today (read from main at c9f6271, not from memory)

| Need | Today | Where |
|---|---|---|
| The phone app | The hub's installable web app (PWA): manifest, icons, service worker, web push. Installable on Android since increments 245 and 246. No native app, no APK, no Android build target | `crates/hub/src/site/pwa.rs`, `crates/hub/src/notify/push/`, `CHANGELOG.md` |
| Phone tests | Playwright emulating a Pixel 7 against a one-user hub, in CI | `tests/android_browser.py`, `.github/workflows/ci.yml` |
| Real-device test | Planned only (increment 253: adb forward, connect over CDP, check the WebAPK) | `.ai/dev/plans/one-user-edition.md` (phase 4, milestone M1) |
| A dev build | Builds are named `main-<sha7>` or `ci-<sha7>`, `dev` locally (`devagotchi version`). A hub can follow `--channel main`. `scripts/fetch-main-build.sh` installs a verified main build on a machine | `crates/devagotchi/build.rs`, `deploy/compose/install.sh`, `release.yml` |
| Build shown in the app | Planned (increment 236: build and channel in the UI and `/healthz`) | `one-user-edition.md` |
| Tracing | No `tracing` crate. Logging is `eprintln!`. The broker writes one plain line per operation to `broker.log` when in the background, with no rotation and no request id | `crates/machine-engine/src/trace.rs`, `service/background.rs` |
| Stats | Recent tool calls and usage: in memory (last 1,000) for local `devagotchi mcp`; the Postgres audit table through a hub | `crates/mcp/src/adapter/history.rs`, `crates/hub/src/machines/history.rs`, `ops/audit.rs` |
| Dev mode | None, in the UI or in config | |
| Export | None. The nearest is `devagotchi ai capture export` (AI-session JSONL, off by default, no redaction) | `crates/kernel/src/capture/mod.rs` |
| Android as a machine (a broker on the phone) | No plan; an open operator question | `.ai/dev/plans/path.md` |

So the phone half mostly exists (the PWA), and the diagnostics half does not exist at all.

## 2. The target

1. **Install the dev build on the phone, beside the release one.** A hub on the `main`
   channel serves a PWA named "Devagotchi Dev", with its own manifest `id`, `start_url`
   scope and a "DEV" badge on the icon, so Android keeps both apps installed side by side.
   Settings > About shows the build (`main-<sha7>`), the channel and the service worker's
   version (increment 236, pulled forward).
2. **Switch on dev stats.** One toggle, per account, in Settings (see `hub-settings.md`).
   When it is on:
   - The app shows a small drawer with the build, the last 50 API calls (route, status,
     milliseconds, request id), tool calls and errors per machine, the push and service
     worker state, Web Vitals (LCP, INP, CLS), and memory where the browser reports it.
   - The hub records that account's requests at debug level, under their request ids.
   - The machines that account reaches log their operations at debug level too, for as long
     as the toggle is on (the hub tells each link, and the link tells its broker).
   - It turns itself off after 24 hours unless it is switched on again, so a forgotten
     toggle doesn't fill a disk.
3. **Export in one step.** One button, "Export diagnostics", in the drawer, and the same
   from a shell: `devagotchi diag export`. Both make one bundle (§4).
4. **Check a machine from a shell.** `devagotchi diag check` prints the build and channel,
   the config it loaded, where its logs go and their size, whether the hub and link answer,
   and what the PDF spec (§10 there) adds: the font pack and the converters found.
5. **Hand it to an agent without copying files.** The bundle is kept on the hub for 7 days,
   and a Devagotchi MCP tool, `diagnostics_get`, reads it. An agent like Claude can then
   read the bundle directly through the Devagotchi connector. On the phone the button also
   offers Android's share sheet (the Web Share API with a file), or a plain download.

## 3. The tracing it needs first

Without this the export has little to export. It is the foundation and lands first.

- Adopt `tracing` with `tracing-subscriber` (MIT) in the hub, the link, the broker and
  `devagotchi mcp`, replacing `eprintln!` as each file is touched.
- One request id per call, made at the hub (or by `devagotchi mcp` when there is no hub), and
  carried hub to link to broker in the request envelope (`contracts`). Every log line and
  every audit row carries it, so one tool call can be followed across three processes.
- Output as JSON lines, one object per event, with stable field names (`ts`, `level`,
  `target`, `request_id`, `machine`, `tool`, `ms`, `outcome`, `message`). The terminal
  keeps a readable format in the foreground.
- Files rotate daily with `tracing-appender` and keep 7 days by default. `broker.log` keeps
  its name for today's file, so nothing that reads it now breaks.
- The level comes from config, `DEVAGOTCHI_LOG` (the same syntax as `RUST_LOG`), or the dev
  stats toggle, in that order of strength (toggle strongest).
- The browser side keeps a ring buffer of its own: console errors, failed fetches with their
  request ids, and timings, in memory, cleared on reload.

## 4. The bundle

One file, `devagotchi-diag-<build>-<UTC time>.zip`. Zip, because Android and every desktop
open it without extra apps. Inside:

| File | What |
|---|---|
| `README.md` | What each file is, its fields, and the time window. Written for a person or an agent reading the bundle cold |
| `manifest.json` | Bundle schema version, build, channel, hub and machine versions, OS, the phone's user agent, the time window, what was redacted and how many times |
| `hub.jsonl` | The hub's log lines in the window |
| `machines/<name>/broker.jsonl` | Each selected machine's broker log in the window, fetched through its link |
| `calls.jsonl` | Tool calls with request id, tool, machine, outcome and ms (from the audit table) |
| `usage.json` | Usage stats per tool and per machine |
| `client.jsonl` | The phone's ring buffer, and the dev stats drawer's snapshot |
| `config.json` | Hub and broker settings, with secrets replaced by `"<redacted>"` |
| `capture.jsonl` | Optional, off by default: the AI-session capture export |

Defaults: the last 24 hours, the machines the account reached in that window, and at most
50 MB (oldest lines dropped first, and `manifest.json` says so).

Redaction runs on everything before it is written:
- Known secret fields from the config structs (client secrets, webhook secrets, storage
  URLs with passwords, API keys, claim codes).
- Patterns: bearer tokens, `ghp_` and similar token prefixes, private key blocks, and URLs
  with a password in them.
- The count per rule goes in `manifest.json`. Lines are redacted in place, never dropped
  silently.
- `capture.jsonl` is redacted the same way, which fixes its `"redaction":"not_performed"`.

## 5. Where AgentHub comes in

The full AgentHub build is the main chat's task and is not built here. Where this spec
touches it:
- The hub depends on `devagotchi-agenthub` already, with no feature gate
  (`crates/hub/Cargo.toml`), and 10 hub files import it.
- The export takes AgentHub's workflow and run logs through one small trait,
  `DiagnosticsSource`, that AgentHub implements on its side. Until it does, the bundle has
  no AgentHub file and the manifest says so.
- The request id from §3 is in the contracts envelope, so AgentHub's runs can carry it too.
- For the solo founder first: one account, one hub, the toggle and the export work with no
  team, no tenancy and no ClickHouse.

## 6. Milestones

| Step | What | Accepted when |
|---|---|---|
| D1 | Tracing foundation (§3) in the broker and `devagotchi mcp` | One call through `devagotchi mcp` shows the same request id in its log line and its history row; the log rotates; `DEVAGOTCHI_LOG=debug` raises the level |
| D2 | The same in the hub and the link, with the id carried across | One tool call through the hub shows one id in `hub.jsonl` and in that machine's broker log |
| D3 | Dev PWA beside release: manifest id, name and badge by channel; About shows build and channel | On a real Android phone both apps install side by side and show different builds |
| D4 | Dev stats toggle and drawer, with the 24-hour auto-off | Toggle on, make a few calls, the drawer shows them with ms and ids; the broker logs debug lines while it is on and stops after it is off |
| D5 | Export: the button, `devagotchi diag export`, redaction and the bundle | The zip holds every file in §4; a planted fake secret is redacted and counted; Android's share sheet offers the file |
| D5b | `devagotchi diag check` | It reports a missing font pack, a stopped link and an unreadable log as three separate lines |
| D6 | `diagnostics_get` through the Devagotchi MCP tools | An agent lists and reads a bundle through the hub |
| D7 | Real-device check (increment 253, `tests/android_device.py`) runs the D3 to D5 steps over adb | The script passes on hades's phone |

No CI work until the working alpha, as the user said. Each step lands on main when it works.
