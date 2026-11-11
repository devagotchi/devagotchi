//! Sign-in providers set from the hub's settings page, kept in the store so a person configures
//! Google or GitHub without touching a file or restarting. The client secret arrives here
//! already sealed (`crate::sealed`). Each change is on the hub's home tenant's chain, naming the
//! provider and never its secret.

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use super::{Accounts, now_secs, secs, tenants};
use crate::store::transaction;

/// One provider as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSignIn {
    /// `google` or `github`.
    pub name: String,
    pub client_id: String,
    pub sealed_secret: String,
    pub enabled: bool,
    pub updated_at: u64,
    pub updated_by: String,
}

impl Accounts {
    pub async fn sign_in_settings(&self) -> BridgeResult<Vec<StoredSignIn>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query(
                "SELECT name, client_id, sealed_secret, enabled, updated_at, updated_by
                 FROM sign_in_settings ORDER BY name",
            )
            .fetch_all(&mut *connection)
            .await?;
            let mut all = Vec::with_capacity(rows.len());
            for row in &rows {
                all.push(StoredSignIn {
                    name: row.try_get("name")?,
                    client_id: row.try_get("client_id")?,
                    sealed_secret: row.try_get("sealed_secret")?,
                    enabled: row.try_get::<i32, _>("enabled")? != 0,
                    updated_at: row.try_get::<i64, _>("updated_at")?.max(0) as u64,
                    updated_by: row.try_get("updated_by")?,
                });
            }
            Ok(all)
        })
    }

    /// Save one provider. `name` is the kind as well: one of each.
    pub async fn save_sign_in(
        &self,
        by: &str,
        name: &str,
        client_id: &str,
        sealed_secret: &str,
        enabled: bool,
    ) -> BridgeResult<()> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        transaction!(&self.store, |connection| {
            sqlx::query(
                "INSERT INTO sign_in_settings
                   (name, kind, client_id, sealed_secret, enabled, updated_at, updated_by)
                 VALUES ($1, $1, $2, $3, $4, $5, $6)
                 ON CONFLICT (name) DO UPDATE SET client_id = excluded.client_id,
                   sealed_secret = excluded.sealed_secret, enabled = excluded.enabled,
                   updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            )
            .bind(name)
            .bind(client_id)
            .bind(sealed_secret)
            .bind(i32::from(enabled))
            .bind(secs(now))
            .bind(by)
            .execute(&mut *connection)
            .await?;
            self.store.scope(connection, &home).await?;
            tenants::record_in(
                connection,
                &home,
                by,
                "sign_in.configured",
                &format!("{name}: {}", if enabled { "on" } else { "off" }),
                now,
            )
            .await?;
            Ok(())
        })
    }
}
