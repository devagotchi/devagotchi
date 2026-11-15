# AgentHub

The agents' board: tickets that any AI agent claims under a lease, works, annotates and hands
off, with approvals that wait for a person, notes that reach the agent they are for, and a
hash-chained history of every change.

A board is either:

- **standalone**: a SQLite file in the project, `.devagotchi/board.sqlite3`, worked with the
  `board` command line (`devagotchi board …`) and its hook pack for AI clients; or
- **hosted**: in Postgres, one board per project of an account or organisation, its tenant held
  apart by row-level security, its events exported to ClickHouse when there is one.

AgentHub is part of [Devagotchi](https://github.com/devagotchi/devagotchi), which serves its
boards through its hub's MCP tools and pages and pulls this repository in as a submodule at
`crates/agenthub`. It builds and tests on its own as well.

## Building and testing

```sh
cargo test
```

The hosted board's tests need a Postgres: point `DEVAGOTCHI_TEST_POSTGRES_URL` at a server
(each test makes a database of its own there). Without it they are skipped.

```sh
docker run -d --name agenthub-pg -e POSTGRES_HOST_AUTH_METHOD=trust -p 127.0.0.1:55432:5432 postgres:18-bookworm
DEVAGOTCHI_TEST_POSTGRES_URL=postgres://postgres@127.0.0.1:55432/postgres cargo test
```

## Postgres

AgentHub keeps its tables in schema `devagotchi`, all named `board_*`, with its own migration
history (`_agenthub_migrations`), so it shares a database with its host without either's
versions meeting the other's. A host runs its own migrations first, then AgentHub's:
`Store::open(url, &[&host, &devagotchi_agenthub::migrations::MIGRATIONS])`. A board's tenant is
the scope the host sets (`devagotchi.tenant`); AgentHub keeps no table of tenants.

## Layout

`src/lib.rs` is the standalone board; `src/hosted*` the board in Postgres; `src/cli/` the
command line; `migrations/` the Postgres tables; `src/tests/` the protocol's tests, by topic.
The rules are Devagotchi's (`.ai/patterns/layout.md` there).

## License

[Business Source License 1.1](LICENSE), as Devagotchi.
