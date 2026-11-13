//! Sign-in providers set from the hub's settings page, kept in the store so a person configures
//! Google, GitHub, Sign in with Shippin (Nomen) or any OpenID Connect provider without touching
//! a file or restarting. The client secret arrives here already sealed (`crate::sealed`). Each
//! change is on the hub's home tenant's chain, naming the provider and never its secret.

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use super::{Accounts, now_secs, secs, tenants};
use crate::store::transaction;

/// One provider as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSignIn {
    /// Its name in URLs and on linked sign-ins: `google`, `github`, `nomen`, or the name an
    /// OpenID Connect provider was given.
    pub name: String,
    /// `google`, `github` or `oidc`.
    pub kind: String,
    /// What its button says; empty for the kind's own.
    pub label: String,
    /// An OpenID Connect provider's issuer; empty for Google and GitHub.
    pub issuer: String,
    pub client_id: String,
    pub sealed_secret: String,
    pub enabled: bool,
    pub updated_at: u64,
    pub updated_by: String,
}

/// A provider as the settings page saves it.
pub struct NewSignIn<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub label: &'a str,
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub sealed_secret: &'a str,
    pub enabled: bool,
}

impl Accounts {
    pub async fn sign_in_settings(&self) -> BridgeResult<Vec<StoredSignIn>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query(
                "SELECT name, kind, label, issuer, client_id, sealed_secret, enabled, updated_at,
                        updated_by
                 FROM sign_in_settings ORDER BY name",
            )
            .fetch_all(&mut *connection)
            .await?;
            let mut all = Vec::with_capacity(rows.len());
            for row in &rows {
                all.push(StoredSignIn {
                    name: row.try_get("name")?,
                    kind: row.try_get("kind")?,
                    label: row.try_get("label")?,
                    issuer: row.try_get("issuer")?,
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

    /// Save one provider, by name: one of each name.
    pub async fn save_sign_in(&self, by: &str, new: &NewSignIn<'_>) -> BridgeResult<()> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        transaction!(&self.store, |connection| {
            sqlx::query(
                "INSERT INTO sign_in_settings
                   (name, kind, label, issuer, client_id, sealed_secret, enabled, updated_at,
                    updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (name) DO UPDATE SET kind = excluded.kind, label = excluded.label,
                   issuer = excluded.issuer, client_id = excluded.client_id,
                   sealed_secret = excluded.sealed_secret, enabled = excluded.enabled,
                   updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            )
            .bind(new.name)
            .bind(new.kind)
            .bind(new.label)
            .bind(new.issuer)
            .bind(new.client_id)
            .bind(new.sealed_secret)
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
                "sign_in.configured",
                &format!("{}: {}", new.name, if new.enabled { "on" } else { "off" }),
                now,
            )
            .await?;
            Ok(())
        })
    }
}
