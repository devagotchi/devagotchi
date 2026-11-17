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
    if let Some(path) = config {
        named_link(&crate::read_config(path)?, device)?;
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

/// A code is only worth issuing for a device the hub will take: one its config names as a
/// linked broker. Any other name is refused at the link (`devices::enroll`), after the code
/// was printed and copied to the machine, so it is refused here instead, before a code exists.
/// A machine an account adds from the hub's pages takes no operator code at all.
fn named_link(config: &crate::config::HubConfig, device: &str) -> BridgeResult<()> {
    match config.brokers.iter().find(|broker| broker.name == device) {
        Some(broker) if broker.link => Ok(()),
        Some(_) => Err(BridgeError::new(
            "invalid_cli",
            format!(
                "{device} is a broker on this hub's host, not a linked machine: it needs no code"
            ),
        )),
        None => Err(BridgeError::new(
            "not_configured",
            format!(
                "the hub's config does not name {device}, so the hub would refuse its link: add {{\"name\": \"{device}\", \"link\": true}} to its brokers and restart the hub, or add the machine from the hub's Machines page"
            ),
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::named_link;
    use crate::config::HubConfig;

    // A broker on the hub's own host: an absolute socket path, or a named pipe on Windows.
    #[cfg(unix)]
    const SOCKET: &str = "/run/devagotchi-broker.sock";
    #[cfg(windows)]
    const SOCKET: &str = r"\\.\pipe\devagotchi-broker";

    fn config(brokers: &str) -> HubConfig {
        HubConfig::parse_with(
            &format!(
                r#"{{"listen": "127.0.0.1:8787", "public_url": "http://127.0.0.1:8787",
                    "auth": {{"mode": "hub", "rp_id": "127.0.0.1"}}, "brokers": {brokers}, "grants": []}}"#
            ),
            |_| None,
        )
        .expect("config")
    }

    #[test]
    fn a_code_is_issued_only_for_a_device_the_config_names_as_a_link() {
        let brokers = serde_json::json!([
            {"name": "box1", "link": true},
            {"name": "here", "socket": SOCKET},
        ]);
        let hub = config(&brokers.to_string());
        assert!(named_link(&hub, "box1").is_ok());
        assert_eq!(named_link(&hub, "here").unwrap_err().code, "invalid_cli");
        let missing = named_link(&hub, "box2").unwrap_err();
        assert_eq!(missing.code, "not_configured");
        assert!(
            missing
                .message
                .contains(r#"{"name": "box2", "link": true}"#),
            "{}",
            missing.message
        );
    }
}
