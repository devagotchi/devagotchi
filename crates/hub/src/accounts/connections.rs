//! A tenant's connections to outside services, kept in the store under its row-level security
//! (`crate::connections` makes the calls). The key arrives sealed (`crate::sealed`) and is
//! never read back out to a person; each add and removal is on the tenant's chain, naming the
//! connection and its service, never the key.

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use super::{Accounts, now_secs, secs, tenants};
use crate::store::transaction;

/// A connection as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConnection {
    pub name: String,
    /// Its service in the catalog (`devagotchi_agenthub::connections::CATALOG`).
    pub service: String,
    pub sealed_key: String,
    /// The key's last four characters.
    pub hint: String,
    pub created_by: String,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
}

fn stored(row: &sqlx::any::AnyRow) -> Result<StoredConnection, sqlx::Error> {
    Ok(StoredConnection {
        name: row.try_get("name")?,
        service: row.try_get("service")?,
        sealed_key: row.try_get("sealed_key")?,
        hint: row.try_get("hint")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get::<i64, _>("created_at")?.max(0) as u64,
        last_used_at: row
            .try_get::<Option<i64>, _>("last_used_at")?
            .map(|at| at.max(0) as u64),
    })
}

const COLUMNS: &str = "name, service, sealed_key, hint, created_by, created_at, last_used_at";

impl Accounts {
    /// The tenant's connections, by name.
    pub async fn connections(&self, tenant: &str) -> BridgeResult<Vec<StoredConnection>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {COLUMNS} FROM connector_keys WHERE tenant_id = $1 ORDER BY name"
            )))
            .bind(tenant)
            .fetch_all(&mut *connection)
            .await?;
            Ok(rows.iter().map(stored).collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// The tenant's connection called `name`.
    pub async fn connection(
        &self,
        tenant: &str,
        name: &str,
    ) -> BridgeResult<Option<StoredConnection>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {COLUMNS} FROM connector_keys WHERE tenant_id = $1 AND name = $2"
            )))
            .bind(tenant)
            .bind(name)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(row.as_ref().map(stored).transpose()?)
        })
    }

    /// Add a connection, or give one of the same name a new key.
    pub async fn save_connection(
        &self,
        tenant: &str,
        by: &str,
        name: &str,
        service: &str,
        sealed_key: &str,
        hint: &str,
    ) -> BridgeResult<()> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            sqlx::query(
                "INSERT INTO connector_keys
                   (tenant_id, name, service, sealed_key, hint, created_by, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
                 ON CONFLICT (tenant_id, name) DO UPDATE SET service = excluded.service,
                   sealed_key = excluded.sealed_key, hint = excluded.hint,
                   updated_at = excluded.updated_at",
            )
            .bind(tenant)
            .bind(name)
            .bind(service)
            .bind(sealed_key)
            .bind(hint)
            .bind(by)
            .bind(secs(now))
            .execute(&mut *connection)
            .await?;
            tenants::record_in(
                connection,
                tenant,
                by,
                "connection.saved",
                &format!("{name}: {service}"),
                now,
            )
            .await?;
            Ok(())
        })
    }

    /// Remove a connection; whether there was one.
    pub async fn remove_connection(
        &self,
        tenant: &str,
        by: &str,
        name: &str,
    ) -> BridgeResult<bool> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let gone = sqlx::query("DELETE FROM connector_keys WHERE tenant_id = $1 AND name = $2")
                .bind(tenant)
                .bind(name)
                .execute(&mut *connection)
                .await?
                .rows_affected()
                > 0;
            if gone {
                tenants::record_in(connection, tenant, by, "connection.removed", name, now).await?;
            }
            Ok(gone)
        })
    }

    /// Note that a connection was just used.
    pub async fn touch_connection(&self, tenant: &str, name: &str) -> BridgeResult<()> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            sqlx::query(
                "UPDATE connector_keys SET last_used_at = $3 WHERE tenant_id = $1 AND name = $2",
            )
            .bind(tenant)
            .bind(name)
            .bind(secs(now))
            .execute(&mut *connection)
            .await?;
            Ok(())
        })
    }
}
