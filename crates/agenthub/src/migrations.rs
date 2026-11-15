//! AgentHub's tables in Postgres (`migrations/`), all `board_*` in schema `devagotchi`, with a
//! version history of their own, `_agenthub_migrations`, so they never meet the versions of
//! the host's migrations in the same database. A host runs its own migrations first (they may
//! make the schema and the role `devagotchi_hub`), then these: `Store::open(url, &[&host,
//! &MIGRATIONS])`.

use std::borrow::Cow;

use devagotchi_store::Migrator;

pub static MIGRATIONS: Migrator = Migrator {
    table_name: Cow::Borrowed("_agenthub_migrations"),
    ignore_missing: true,
    ..sqlx::migrate!("./migrations")
};

#[cfg(test)]
mod tests;
