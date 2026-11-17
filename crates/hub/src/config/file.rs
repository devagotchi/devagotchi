//! The two things the hub is started with on disk: its config file, read with its
//! `*_ENV` variables from the environment, and its state directory (keys, owner-only).

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::HubConfig;

pub(crate) fn read(path: &std::path::Path) -> BridgeResult<HubConfig> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        BridgeError::new("invalid_config", format!("{}: {error}", path.display()))
    })?;
    HubConfig::parse_with(&text, |name| std::env::var(name).ok())
}

pub(crate) fn prepare_state_dir(state_dir: &std::path::Path) -> BridgeResult<()> {
    std::fs::create_dir_all(state_dir)
        .map_err(|error| BridgeError::new("state_dir", error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(state_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| BridgeError::new("state_dir", error.to_string()))?;
    }
    Ok(())
}
