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

/// The mail server saved on `/admin` → Email, as `hub_settings` keeps it (`mail.*` rows): the
/// address carries no password; the password is sealed under the hub's key (`mail.password`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMail {
    pub smtp_url: String,
    pub from: String,
    pub per_address_per_hour: u32,
    pub sealed_password: String,
    pub enabled: bool,
}

/// The name the mail password is sealed under.
pub const MAIL_SEALED_AS: &str = "mail.password";
const MAIL_KEYS: [&str; 5] = [
    "mail.smtp_url",
    "mail.from",
    "mail.per_address_per_hour",
    "mail.sealed_password",
    "mail.enabled",
];

impl Accounts {
    /// The mail server saved on `/admin`, if one was.
    pub async fn stored_mail(&self) -> BridgeResult<Option<StoredMail>> {
        let saved = self.hub_settings().await?;
        let Some(smtp_url) = saved.get("mail.smtp_url").cloned() else {
            return Ok(None);
        };
        let text = |key: &str| saved.get(key).cloned().unwrap_or_default();
        Ok(Some(StoredMail {
            smtp_url,
            from: text("mail.from"),
            per_address_per_hour: text("mail.per_address_per_hour").parse().unwrap_or(5),
            sealed_password: text("mail.sealed_password"),
            enabled: text("mail.enabled") == "on",
        }))
    }

    /// Save the mail server, audited as `mail.configured` with its address, sender and
    /// whether it is on: never the password, sealed or not.
    pub async fn save_mail(&self, by: &str, mail: &StoredMail) -> BridgeResult<()> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        let values = [
            mail.smtp_url.clone(),
            mail.from.clone(),
            mail.per_address_per_hour.to_string(),
            mail.sealed_password.clone(),
            if mail.enabled { "on" } else { "off" }.to_string(),
        ];
        transaction!(&self.store, |connection| {
            for (key, value) in MAIL_KEYS.iter().zip(values.iter()) {
                sqlx::query(
                    "INSERT INTO hub_settings (key, value, updated_at, updated_by)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                       updated_at = excluded.updated_at, updated_by = excluded.updated_by",
                )
                .bind(*key)
                .bind(value)
                .bind(secs(now))
                .bind(by)
                .execute(&mut *connection)
                .await?;
            }
            self.store.scope(connection, &home).await?;
            tenants::record_in(
                connection,
                &home,
                by,
                "mail.configured",
                &format!(
                    "{} from {}: {}",
                    mail.smtp_url,
                    mail.from,
                    if mail.enabled { "on" } else { "off" }
                ),
                now,
            )
            .await?;
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
