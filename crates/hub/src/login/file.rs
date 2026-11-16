//! The account file (`account.json`, owner-only, written whole and renamed into place): reading, writing and removing it, and the account on file, refreshed when due.

use std::{io::Write, path::Path};

use super::{
    Standing, Stored,
    hub::{HUB_CLIENT, hub_refresh},
    issuer::Issuer,
    unix_now,
};

/// Read what is on file; `None` when nothing is. A file this binary cannot read is an
/// error naming the file, never its contents.
pub fn read(path: &Path) -> Result<Option<Stored>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            format!(
                "{} is not an account file; `devagotchi logout` removes it",
                path.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Write the file whole, owner-only, and rename it into place.
pub fn write(path: &Path, stored: &Stored) -> Result<(), String> {
    let text = serde_json::to_string_pretty(stored).map_err(|e| e.to_string())? + "\n";
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("{}: {e}", tmp.display()))?;
    }
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn remove(path: &Path) -> Result<bool, String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Whether an account (not `--local`, not past its offline grace) is on file.
pub fn signed_in(path: &Path) -> bool {
    matches!(read(path), Ok(Some(Stored::Account(account))) if account.standing(unix_now()) != Standing::Expired)
}

/// The account on file, refreshed when due and Nomen answers; otherwise as it stands.
/// What comes back is written to the file only when the refresh succeeded.
pub async fn current(path: &Path) -> Result<Option<Stored>, String> {
    let stored = read(path)?;
    let Some(Stored::Account(account)) = &stored else {
        return Ok(stored);
    };
    if matches!(
        account.standing(unix_now()),
        Standing::Fresh | Standing::Expired
    ) {
        return Ok(stored);
    }
    // A hub login records `hub`. A login from before that field existed is still a hub
    // login when its client is the hub's built-in one. A direct issuer login uses another.
    if account.hub.is_some() || account.client_id == HUB_CLIENT {
        if let Ok(refreshed) = hub_refresh(account).await {
            write(path, &Stored::Account(refreshed.clone()))?;
            return Ok(Some(Stored::Account(refreshed)));
        }
        return Ok(stored);
    }
    let Ok(nomen) = Issuer::new(&account.issuer, &account.client_id) else {
        return Ok(stored);
    };
    match nomen.refresh(account).await {
        Ok(refreshed) => {
            write(path, &Stored::Account(refreshed.clone()))?;
            Ok(Some(Stored::Account(refreshed)))
        }
        Err(_) => Ok(stored),
    }
}
