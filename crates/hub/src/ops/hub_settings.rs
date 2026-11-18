//! The `/admin` settings as Postgres keeps them (`hub_settings`, migration 0034): read when the
//! hub starts and after each save, written by the settings page. What each setting is and how
//! the values in force are worked out is `crate::config::live`. Each change is on the hub's
//! home tenant's chain, naming the setting and its new value.

use std::collections::BTreeMap;

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use crate::config::{HubConfig, LiveSettings};
use crate::identity::accounts::{Accounts, tenants};
use crate::machines::now_secs;
use crate::store::{secs, transaction};

impl Accounts {
    /// Every saved setting, by key.
    pub async fn hub_settings(&self) -> BridgeResult<BTreeMap<String, String>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query("SELECT key, value FROM hub_settings")
                .fetch_all(&mut *connection)
                .await?;
            let mut saved = BTreeMap::new();
            for row in rows {
                saved.insert(row.try_get("key")?, row.try_get("value")?);
            }
            Ok(saved)
        })
    }

    /// Save these settings (key, value as the page shows it), each one audited, in one
    /// transaction: all of them or none.
    pub async fn save_hub_settings(
        &self,
        by: &str,
        changes: &[(String, String)],
    ) -> BridgeResult<()> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        transaction!(&self.store, |connection| {
            for (key, value) in changes {
                sqlx::query(
                    "INSERT INTO hub_settings (key, value, updated_at, updated_by)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                       updated_at = excluded.updated_at, updated_by = excluded.updated_by",
                )
                .bind(key)
                .bind(value)
                .bind(secs(now))
                .bind(by)
                .execute(&mut *connection)
                .await?;
            }
            self.store.scope(connection, &home).await?;
            for (key, value) in changes {
                tenants::record_in(
                    connection,
                    &home,
                    by,
                    "settings.changed",
                    &format!("{key} = {value}"),
                    now,
                )
                .await?;
            }
            Ok(())
        })
    }
}

/// Put the saved settings in force for every holder of `config`, under what the file and the
/// environment pinned. Run when the hub starts and after each save.
pub async fn apply_saved(config: &HubConfig, accounts: &Accounts) -> BridgeResult<LiveSettings> {
    let saved = accounts.hub_settings().await?;
    let settings = LiveSettings::from_config(config).with_saved(&saved, config.pinned());
    config.live_handle().replace(settings);
    Ok(settings)
}
