//! `devagotchi hub enroll | revoke | devices`: the operator's machine commands, run on the
//! hub's host against the hub's Postgres (the storage its config names, or
//! `DEVAGOTCHI_HUB_STORAGE_URL`). Each is recorded on the hub's audit chain.

use std::path::Path;

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::devices::Devices;
use crate::{
    config,
    ops::audit::{Audit, operator_event},
    store,
};

/// A one-time code that binds one machine's key to `device`, printed once.
pub(crate) fn enroll(state: &Path, config: Option<&Path>, device: &str) -> BridgeResult<()> {
    if !config::valid_name(device) {
        return Err(BridgeError::new(
            "invalid_cli",
            "device must be [a-z0-9][a-z0-9-]{0,62}, the broker name in the hub config",
        ));
    }
    let code = store::block_on(async {
        let store = store::for_command(state, config).await?;
        let code = Devices::new(store.clone())
            .issue_code(device, super::random::<16>())
            .await?;
        // The code itself is never audited or logged; only that one was issued.
        operator_event(&Audit::new(store), "link.code_issued", device, "ok").await?;
        Ok(code)
    })?;
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
    let (revoked, cancelled) = store::block_on(async {
        let store = store::for_command(state, config).await?;
        let (revoked, cancelled) = Devices::new(store.clone()).revoke(device).await?;
        let outcome = format!("revoked={revoked} codes_cancelled={cancelled}");
        operator_event(&Audit::new(store), "link.revoke", device, &outcome).await?;
        Ok((revoked, cancelled))
    })?;
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
    let list = store::block_on(async {
        Devices::new(store::for_command(state, config).await?)
            .list()
            .await
    })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&list).unwrap_or_default()
    );
    Ok(())
}
