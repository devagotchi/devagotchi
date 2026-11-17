//! A test's check that a tenant's event chain is whole.

use super::*;

impl Accounts {
    /// Whether `tenant`'s chain is whole: every row's hash is its own, over the row before.
    pub(crate) async fn chain_intact(&self, tenant: &str) -> BridgeResult<bool> {
        let rows = transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            Ok(sqlx::query(
                "SELECT seq, at, actor, event, detail, prev_hash, hash FROM tenancy_events
                 WHERE tenant_id = $1 ORDER BY seq",
            )
            .bind(tenant)
            .fetch_all(&mut *connection)
            .await?)
        })?;
        let mut prev = "0".repeat(64);
        let mut expected = 1i64;
        for row in rows {
            let read = |row: &AnyRow| -> Result<_, sqlx::Error> {
                Ok((
                    row.try_get::<i64, _>(0)?,
                    row.try_get::<i64, _>(1)?,
                    row.try_get::<String, _>(2)?,
                    row.try_get::<String, _>(3)?,
                    row.try_get::<String, _>(4)?,
                    row.try_get::<String, _>(5)?,
                    row.try_get::<String, _>(6)?,
                ))
            };
            let (seq, at, actor, event, detail, prev_hash, hash) =
                read(&row).map_err(store_error)?;
            if seq != expected
                || prev_hash != prev
                || hash != event_hash(&prev, tenant, seq, at, &actor, &event, &detail)
            {
                return Ok(false);
            }
            prev = hash;
            expected += 1;
        }
        Ok(true)
    }
}
