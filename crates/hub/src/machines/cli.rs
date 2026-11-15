//! `devagotchi hub enroll | revoke | devices`: the operator's machine commands, run on the
//! hub's host against the hub's Postgres (the storage its config names, or
//! `DEVAGOTCHI_HUB_STORAGE_URL`). Each is recorded on the hub's audit chain.

use std::path::Path;

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::devices::Devices;
use crate::{config, operator_event, prepare_state_dir, read_config, store};

/// The machine registry in the hub's Postgres for `enroll`, `revoke` and `devices`: the
/// storage `config` names, or `DEVAGOTCHI_HUB_STORAGE_URL`. Run `work` on it to the end.
fn with_registry<T>(
    state_dir: &Path,
    config: Option<&Path>,
    work: impl AsyncFnOnce(&Devices) -> BridgeResult<T>,
) -> BridgeResult<T> {
    prepare_state_dir(state_dir)?;
    let storage = match config {
        Some(path) => read_config(path)?.storage,
        None => None,
    };
    let url = store::url_for(storage.as_ref().and_then(|s| s.url.as_deref()), state_dir);
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err(BridgeError::new(
            "invalid_config",
            "machines are kept in the hub's Postgres: pass --config with the hub's config, or set DEVAGOTCHI_HUB_STORAGE_URL",
        ));
    }
    let schema = store::schema_for(storage.as_ref().and_then(|s| s.schema.as_deref()));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| BridgeError::new("runtime", error.to_string()))?
        .block_on(async {
            let store = store::Store::open_in(&url, schema.as_deref()).await?;
            work(&Devices::new(store)).await
        })
}

/// A one-time code that binds one machine's key to `device`, printed once.
pub(crate) fn enroll(state: &Path, config: Option<&Path>, device: &str) -> BridgeResult<()> {
    if !config::valid_name(device) {
        return Err(BridgeError::new(
            "invalid_cli",
            "device must be [a-z0-9][a-z0-9-]{0,62}, the broker name in the hub config",
        ));
    }
    let code = with_registry(state, config, async |devices| {
        devices.issue_code(device, super::random::<16>()).await
    })?;
    // The code itself is never audited or logged; only that one was issued.
    operator_event(state, "link.code_issued", device, "ok")?;
    // The code is the one secret this prints; it goes to stdout only, once.
    println!("{code}");
    eprintln!(
        "One-time code for {device}, valid {} minutes; any earlier unused code for {device} no longer works.\nOn that machine, with the code in DEVAGOTCHI_ENROLL_CODE (or on stdin):\n  devagotchi link enroll --state-dir ... --hub ... --device {device} --broker ...",
        super::devices::CODE_TTL_SECS / 60
    );
    Ok(())
}

/// Revoke `device` and cancel its unused codes; its link closes within seconds.
pub(crate) fn revoke(state: &Path, config: Option<&Path>, device: &str) -> BridgeResult<()> {
    let (revoked, cancelled) =
        with_registry(state, config, async |devices| devices.revoke(device).await)?;
    operator_event(
        state,
        "link.revoke",
        device,
        &format!("revoked={revoked} codes_cancelled={cancelled}"),
    )?;
    match (revoked, cancelled) {
        (false, 0) => {
            return Err(BridgeError::new(
                "not_enrolled",
                format!("no active device or pending code named {device}"),
            ));
        }
        (true, _) => eprintln!(
            "{device} revoked; its link closes within seconds ({cancelled} pending codes cancelled)"
        ),
        (false, _) => eprintln!("{cancelled} pending codes for {device} cancelled"),
    }
    Ok(())
}

/// Every enrolled machine, as JSON.
pub(crate) fn devices(state: &Path, config: Option<&Path>) -> BridgeResult<()> {
    let list = with_registry(state, config, async |devices| devices.list().await)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&list).unwrap_or_default()
    );
    Ok(())
}
