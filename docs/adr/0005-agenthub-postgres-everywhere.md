# ADR 0005: AgentHub runs on Postgres everywhere, as a library, a binary or a container

Status: accepted. Date: 2026-11-15. Supersedes the standalone SQLite board in
[ADR 0002](0002-bound-board-offline-cache-and-queued-drafts.md) and in AgentHub's README.

## Authority and scope

The operator ruled on 2026-11-15:
- standalone AgentHub "should be postgres, should be deployable as a docker container, or even
  ran as a self-contained binary";
- it is separate because it is also offered "as an included service for Shippin, the
  commercial cloud panel";
- "if included with devagotchi then we keep it in devagotchi with clear prefix tables, we have
  it use shippin, and then stand alone is postgres".

The operator also asked whether a better database exists for this, naming FoundationDB and
rrflow. The review is below.

## Decision

1. **One storage engine: Postgres.** AgentHub's board of record is Postgres in every form. The
   standalone SQLite board (`.devagotchi/board.sqlite3`) is deleted. ADR 0002's offline queue,
   when it is built, is a pending-operation queue in the client, never a second board.
2. **Three ways to run it, one crate** (`devagotchi/agenthub`):

   | Form | Who gives it Postgres | Where its tables are |
   |---|---|---|
   | Inside Devagotchi (library) | the hub's `storage` | schema `devagotchi`, every table `board_*`, history `_agenthub_migrations` (as today) |
   | Inside Shippin (library or container) | Shippin's configuration | the schema Shippin names |
   | Standalone (binary or container) | its own config: `AGENTHUB_DATABASE_URL`, schema `agenthub` unless set | the schema it is given |

   The schema is the host's setting, so the migrations name no schema of their own (they run
   with the host's `search_path`). Devagotchi passes `devagotchi`, so its database does not
   change.
3. **Standalone AgentHub is a server.** The `agenthub` binary serves the board's MCP tools and
   HTTP API on one port, signs people in through a host-chosen issuer (Nomen by default, so
   Shippin's accounts work), and is published as a release archive per OS and an image
   `ghcr.io/devagotchi/agenthub`. Devagotchi keeps serving boards through its own hub, so a
   Devagotchi user runs nothing extra.
4. **Events leave through the outbox.** The Postgres outbox that feeds ClickHouse also feeds
   other sinks, a signed webhook first. That is how Shippin and rrflow react to board changes
   (triggers and workflows) without reading AgentHub's tables.

## Database review (2026-11-15)

What a board needs from its store:
- serializable transactions, used for leases and fences;
- row-level security per tenant;
- change notification (`LISTEN`);
- an outbox committed in the same transaction as the change;
- managed offerings on every cloud the panel deploys to.

| Option | Verdict |
|---|---|
| **Postgres** | Has all of the above, on every cloud as a managed service (RDS, Cloud SQL, AlloyDB, Oracle, Supabase, Neon). For AI-shaped data, `pgvector` adds semantic search over tickets and notes in the same transactions when a feature needs it. **Kept.** |
| FoundationDB | A strongly consistent ordered key-value store. Its transactions are good, but it has no SQL, no row-level security and no change feed of its own, so AgentHub would build all three as a layer above it. It also has few managed offerings. Not chosen. |
| rrflowDB (rrflow) | The AI-oriented multi-model database of rrflow, Jesse's own engine. Its README states it is pre-alpha and that PostgreSQL compatibility is a target with nothing implemented yet (its gate H-08). **The path: AgentHub's Postgres test suite becomes one of rrflowDB's qualification suites.** When rrflowDB passes it, AgentHub runs on it unchanged. Until then rrflow connects through the outbox (point 4) and AgentHub's API. |
| Other Postgres-wire distributed databases (CockroachDB, YugabyteDB) | Their row-level security, `LISTEN` and SERIALIZABLE behaviour each differ from Postgres. Reconsider only if a single cell outgrows one Postgres, which is far off with cells ([ADR 0003](0003-hosting-pooled-free-per-instance-paid.md)). |
| Dragonfly | A cache, not a store ([ADR 0004](0004-optional-cache-dragonfly.md)). |

## Consequences

- AgentHub's `src/lib.rs` SQLite board and its file-based tests move to Postgres. Its tests
  already run on Postgres in its own CI.
- `devagotchi board` (the command line) talks to a hub or an AgentHub server, never to a file.
- A new image and release archives come from the agenthub repository's own release workflow.
