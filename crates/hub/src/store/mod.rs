//! The hub's store: the Postgres engine `devagotchi-store` (`crates/store`), re-exported so the
//! hub's code names it `crate::store`, with what the hub's database holds: the hub's own
//! tables (`crates/hub/migrations`, history in `_sqlx_migrations`), then AgentHub's
//! (`devagotchi_agenthub::migrations`, history of its own). [`open`] makes both.

use std::path::Path;

use devagotchi_contracts::{BridgeError, BridgeResult};
pub use devagotchi_store::*;

/// The hub's own tables.
pub static MIGRATIONS: Migrator = Migrator {
    ignore_missing: true,
    ..sqlx::migrate!("./migrations")
};

/// Everything a hub's database holds, in the order it is made: the hub's tables (they make
/// the schema and the role `devagotchi_hub`), then AgentHub's boards.
pub static DATABASE: [&Migrator; 2] = [&MIGRATIONS, &devagotchi_agenthub::migrations::MIGRATIONS];

/// The hub's database at `url`, brought up to date.
pub async fn open(url: &str) -> BridgeResult<Store> {
    Store::open(url, &DATABASE).await
}

/// [`open`], with the migration records in `schema` ([`Store::open_in`]).
pub async fn open_in(url: &str, schema: Option<&str>) -> BridgeResult<Store> {
    Store::open_in(url, schema, &DATABASE).await
}

/// The hub's Postgres: the environment's URL (`DEVAGOTCHI_HUB_STORAGE_URL`), else the one its
/// config names.
pub(crate) fn postgres_url(configured: Option<&str>) -> BridgeResult<String> {
    url_for(configured).ok_or_else(|| {
        BridgeError::new(
            "invalid_config",
            "the hub keeps everything in Postgres: set storage.url in its config or DEVAGOTCHI_HUB_STORAGE_URL",
        )
    })
}

/// The hub's Postgres for an operator command run on its host (`hub enroll`, `owner`,
/// `connect-nomen` …): the storage `config` names, or `DEVAGOTCHI_HUB_STORAGE_URL`. The hub
/// keeps nothing in files but its keys, so there is no other place to look.
pub(crate) async fn for_command(state_dir: &Path, config: Option<&Path>) -> BridgeResult<Store> {
    crate::config::prepare_state_dir(state_dir)?;
    let storage = match config {
        Some(path) => crate::config::read(path)?.storage,
        None => None,
    };
    let url = postgres_url(storage.as_ref().and_then(|s| s.url.as_deref())).map_err(|_| {
        BridgeError::new(
            "invalid_config",
            "the hub keeps its machines and its audit record in Postgres: pass --config with the hub's config, or set DEVAGOTCHI_HUB_STORAGE_URL",
        )
    })?;
    let schema = schema_for(storage.as_ref().and_then(|s| s.schema.as_deref()));
    open_in(&url, schema.as_deref()).await
}

/// Run `work` to the end on a runtime of its own: what an operator command does.
pub(crate) fn block_on<T>(work: impl Future<Output = BridgeResult<T>>) -> BridgeResult<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| BridgeError::new("runtime", error.to_string()))?
        .block_on(work)
}

/// Stores for the hub's tests: a new database on the test Postgres with everything a hub's
/// database holds (`DEVAGOTCHI_TEST_POSTGRES_URL`; none without it).
#[cfg(test)]
mod tests;

#[cfg(test)]
pub mod testing {
    pub use devagotchi_store::testing::{TestStore, empty_postgres};

    /// A new database made as a hub makes its own, or `None` without a test Postgres.
    pub async fn postgres() -> Option<TestStore> {
        devagotchi_store::testing::postgres_with(&super::DATABASE).await
    }

    /// The stores a test runs on: [`postgres`], or none.
    pub async fn stores() -> Vec<TestStore> {
        postgres().await.into_iter().collect()
    }

    /// The hub's tables at `url` brought to migration `version` and no further, as a hub from
    /// before a later migration left it.
    pub async fn migrated_to(url: &str, version: i64) {
        devagotchi_store::testing::migrated_to(url, &super::MIGRATIONS, version).await
    }
}
