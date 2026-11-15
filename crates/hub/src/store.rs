//! The hub's store is `devagotchi-store` (`crates/store`): the database the hub and AgentHub
//! share, its migrations and its tenant isolation. Re-exported here so the hub's code names it
//! `crate::store` as it always has.

use std::path::Path;

use devagotchi_contracts::{BridgeError, BridgeResult};
pub use devagotchi_store::*;

/// The hub's Postgres for an operator command run on its host (`hub enroll`, `owner`,
/// `connect-nomen` …): the storage `config` names, or `DEVAGOTCHI_HUB_STORAGE_URL`. The hub
/// keeps nothing in files but its keys, so there is no other place to look.
pub(crate) async fn for_command(state_dir: &Path, config: Option<&Path>) -> BridgeResult<Store> {
    crate::prepare_state_dir(state_dir)?;
    let storage = match config {
        Some(path) => crate::read_config(path)?.storage,
        None => None,
    };
    let url = url_for(storage.as_ref().and_then(|s| s.url.as_deref()), state_dir);
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err(BridgeError::new(
            "invalid_config",
            "the hub keeps its machines and its audit record in Postgres: pass --config with the hub's config, or set DEVAGOTCHI_HUB_STORAGE_URL",
        ));
    }
    let schema = schema_for(storage.as_ref().and_then(|s| s.schema.as_deref()));
    Store::open_in(&url, schema.as_deref()).await
}

/// Run `work` to the end on a runtime of its own: what an operator command does.
pub(crate) fn block_on<T>(work: impl Future<Output = BridgeResult<T>>) -> BridgeResult<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| BridgeError::new("runtime", error.to_string()))?
        .block_on(work)
}
