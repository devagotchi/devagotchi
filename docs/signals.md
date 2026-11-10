# Signals: the hub's event contract, version 1

Status: implemented in increment 117 (`crates/hub/src/signals.rs`). The JSON Schema is
[`signals/v1.schema.json`](signals/v1.schema.json), generated from the Rust type and held
equal to it by a test.

A hub produces one stream of what happens on it, in one shape, for whatever its operator
points it at: an alerting service, a dataset builder, a policy checker, a second hub. The
hub never depends on a receiver. A receiver attaches as a **sink** and is sent signals;
nothing about boards, accounts or machines waits for it.

## The signal

Every signal is one JSON object. These keys are always there:

| Key | Meaning |
|---|---|
| `schema` | `devagotchi.signal/v1`. A change that breaks a receiver gets a new version. |
| `id` | Stable and unique. Sent again, it is the same. A receiver deduplicates by it. |
| `source` | `board`, `tenancy` or `audit` (below). |
| `kind` | What happened: a board action (`ticket.claimed`), a tenant event (`org.member_set`), or `tool.<name>` for a tool call. |
| `at_ms` | When, in milliseconds since the Unix epoch. |

These are there when they apply:

| Key | Meaning |
|---|---|
| `tenant` | The tenant it happened in (`t-<uuid>`). Absent for the audit chain, which is the hub's. |
| `owner` | Whose board: an account id, `org-<tenant>` for an organisation, `@shared` for a configured board. |
| `project` | The board's project name. |
| `actor` | Who did it: an account, an agent seat (`<account>/<seat>`), or `hub`. |
| `ticket` | The ticket it concerns. |
| `target` | What a tool call was aimed at: a broker, or a board's project name. |
| `operation` | The operation a tool call named. |
| `outcome` | How a tool call ended: `ok`, or the refusal's code (`read_only`, `not_granted`, `stale_fence` …). |
| `data` | Everything else, and only for a sink that takes `full`: a board event's data (ticket text, notes, issue context), a tenant event's detail, a tool call's `via`. |

New optional keys may appear within version 1. A receiver ignores keys it does not know.

## The sources

| Source | What it carries | Order | `id` |
|---|---|---|---|
| `board` | Every board's events: tickets created, claimed, heartbeated, noted, completed, handed off, released, force-released; dependencies; approvals requested and decided; issue context and checkpoints | By `seq` within one board | `board:<tenant>:<board>:<seq>:<hash16>` |
| `tenancy` | Each tenant's own chain: organisations made, members set and removed, teams, project roles and visibility, invites made, taken and withdrawn, accounts | By `seq` within one tenant | `tenancy:<tenant>:<seq>:<hash16>` |
| `audit` | Every tool call the hub served and how it ended, including refusals | By `seq` | `audit:<seq>:<hash16>` |

`<hash16>` is the first 16 hex digits of the entry's hash in its own chain, so an id never
repeats across a reinstalled hub whose sequences start again.

## Delivery

A sink is an HTTPS endpoint. The hub posts batches of up to 500 signals:

```http
POST <url>
Content-Type: application/x-ndjson
Devagotchi-Signal-Schema: devagotchi.signal/v1
Devagotchi-Sink: <name>
Authorization: Bearer <token>                 (when the sink names a token)
Devagotchi-Signature: sha256=<hex HMAC-SHA256 of the body>   (when it names a secret)

{"schema":"devagotchi.signal/v1","id":"audit:41:9c1f…","source":"audit","kind":"tool.board_create","at_ms":1794350000123,"actor":"acct_…","target":"acme/site","outcome":"read_only"}
…
```

Header names are case-insensitive; the hub sends them in lowercase. Any 2xx answer takes the batch. Anything else, or no answer within 15 seconds, and the same
signals are sent again, after a wait that doubles from one second to a minute. Delivery is
at least once and in order within a scope (one board, one tenant, the audit chain); there
is no order between scopes. Each sink keeps its own position per scope in the hub's
Postgres (`signal_cursors`), so one sink that is down falls behind alone: it holds back
neither the hub nor any other sink, and nothing is dropped, because every source is
append-only. A new sink starts from the beginning of every source.

A sink that cannot verify the signature, or sees a token it does not expect, should answer
401 and keep nothing.

## Configuring a sink

In the hub's config:

```json
"signals": {
  "sinks": [
    {
      "name": "watcher",
      "url": "https://signals.example.com/in",
      "token_env": "DEVAGOTCHI_SIGNAL_TOKEN",
      "secret_env": "DEVAGOTCHI_SIGNAL_SECRET",
      "fields": "metadata",
      "sources": ["board", "tenancy", "audit"]
    }
  ]
}
```

- `url` is https, or http to this machine or to a service on the same private network (a
  name without a dot, as Compose names services). Anything else is refused at start: no
  signal crosses a network in the clear.
- `token_env` and `secret_env` name environment variables; the values never go in the
  config. A sink that names one that is not set stops the start, naming it.
- `fields` is `metadata` by default: what happened, to what, by whom and how it ended,
  never `data`. `full` adds `data`, which holds ticket text, notes and issue bodies; choose
  it only for a receiver allowed to keep those.
- `sources` limits what it takes; empty takes all three.

In the Compose stack, `DEVAGOTCHI_HUB_SIGNAL_SINKS` in `.env` is that JSON list, and
`DEVAGOTCHI_SIGNAL_TOKEN` and `DEVAGOTCHI_SIGNAL_SECRET` are passed to the hub for one sink
to name.

A sink is the operator's: on a hosted hub it receives every tenant's signals. Per-tenant
sinks that an organisation configures for itself are not part of version 1.

## Health

`/healthz` lists each sink under `signals`: `name`, `ready`, `degraded`, `sent`, `waiting`
(the signals its last attempt found and could not deliver), `oldest_waiting_secs`,
`last_sent_at`, `last_error` and `fields`. It never shows a token, a secret or a signal.
A sink is reported, never counted: a sink that is down does not make the hub unhealthy.

## The same signals from ClickHouse

The hub's ClickHouse holds the board source as `board_events` (`warehouse.rs`). Each row
carries `tenant_id`, `board_id`, `seq`, `hash`, `action`, `actor`, `ticket` and `at`, from
which a board signal follows: its `id` is `board:<tenant_id>:<board_id>:<seq>:<first 16 of
hash>`, its `kind` the `action`. A dataset built from ClickHouse and one built from a sink
therefore name each board event the same way and agree on what happened.
