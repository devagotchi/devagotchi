//! Where AgentHub's board events go for statistics, set from the hub's settings page and kept in
//! the store, so a person connects their own ClickHouse without touching a file or restarting.
//! The password arrives here already sealed (`crate::sealed`). Each change is on the hub's home
//! tenant's chain, naming where it points and never the password.

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use super::{Accounts, now_secs, secs, tenants};
use crate::store::transaction;

/// The ClickHouse export as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClickHouse {
    /// Its HTTP interface, `http(s)://host:port`.
    pub url: String,
    pub database: String,
    /// Empty when ClickHouse takes no user (its `default`).
    pub user: String,
    /// Empty when there is no password.
    pub sealed_password: String,
    pub enabled: bool,
    pub updated_at: u64,
    pub updated_by: String,
}

impl Accounts {
    /// The ClickHouse set on the settings page, if one was.
    pub async fn clickhouse_setting(&self) -> BridgeResult<Option<StoredClickHouse>> {
        transaction!(&self.store, |connection| {
            let row = sqlx::query(
                "SELECT url, database_name, user_name, sealed_password, enabled, updated_at,
                        updated_by
                 FROM analytics_settings WHERE name = 'clickhouse'",
            )
            .fetch_optional(&mut *connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(StoredClickHouse {
                url: row.try_get("url")?,
                database: row.try_get("database_name")?,
                user: row.try_get("user_name")?,
                sealed_password: row.try_get("sealed_password")?,
                enabled: row.try_get::<i32, _>("enabled")? != 0,
                updated_at: row.try_get::<i64, _>("updated_at")?.max(0) as u64,
                updated_by: row.try_get("updated_by")?,
            }))
        })
    }

    /// Save the ClickHouse export (`updated_at` and `updated_by` are set here).
    pub async fn save_clickhouse(&self, by: &str, new: &StoredClickHouse) -> BridgeResult<()> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        transaction!(&self.store, |connection| {
            sqlx::query(
                "INSERT INTO analytics_settings
                   (name, url, database_name, user_name, sealed_password, enabled, updated_at,
                    updated_by)
                 VALUES ('clickhouse', $1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (name) DO UPDATE SET url = excluded.url,
                   database_name = excluded.database_name, user_name = excluded.user_name,
                   sealed_password = excluded.sealed_password, enabled = excluded.enabled,
                   updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            )
            .bind(&new.url)
            .bind(&new.database)
            .bind(&new.user)
            .bind(&new.sealed_password)
            .bind(i32::from(new.enabled))
            .bind(secs(now))
            .bind(by)
            .execute(&mut *connection)
            .await?;
            self.store.scope(connection, &home).await?;
            tenants::record_in(
                connection,
                &home,
                by,
                "analytics.configured",
                &format!(
                    "clickhouse {} {}: {}",
                    new.url,
                    new.database,
                    if new.enabled { "on" } else { "off" }
                ),
                now,
            )
            .await?;
            Ok(())
        })
    }
}
