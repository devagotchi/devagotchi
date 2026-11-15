# ADR 0004: an optional cache, Dragonfly by default, off unless configured

Status: accepted. Date: 2026-11-15. Amends [ADR 0001](0001-hosted-board-postgres-and-clickhouse.md)'s
"Do not add Dragonfly without measured need".

## Authority and scope

The operator ruled on 2026-11-15: "optional configuration for dragonfly", chosen as
"Optional, off by default". This is the separate ruling ADR 0001 and the release plan asked
for.

## Decision

1. The hub takes an optional cache: `cache.url`, or `DEVAGOTCHI_HUB_CACHE_URL`, a Redis-protocol
   URL (`redis://` or `rediss://`). Dragonfly is the recommended server; Valkey and Redis work
   the same. Without it the hub does everything it does today.
2. **Postgres stays the record and ClickHouse stays the statistics.** Nothing is stored only in
   the cache. A cache that is down or empty makes the hub degraded (shown in `/healthz` and on
   `/admin`), never wrong. Each use falls back to what the hub does without a cache.
3. The cache is used only where several hub processes must share short-lived state:
   - rate limits and abuse counters (sign-in, email sends, enrollment codes, API keys);
   - which process holds a machine's connection, when a pooled cell runs more than one
     process ([ADR 0003](0003-hosting-pooled-free-per-instance-paid.md)).
   Further uses each name the measurement that asks for them.
4. It is set like every other setting: a card on `/admin` (grouped with storage) with the
   address, a Check button and its state. The environment wins, and the card shows when it
   does. The password is sealed with `settings.key`. The compose stack has an opt-in Dragonfly
   service. One solo VM does not need it, and the installers do not add it unless asked.

## Consequences

- A solo developer's hub is unchanged and needs nothing new.
- A pooled cell can grow past one process without a code change in the core.
- The core gains a Redis-protocol client dependency, built into the default binary, idle when
  unconfigured.
