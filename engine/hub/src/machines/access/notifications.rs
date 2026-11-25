//! Bounded at-least-once delivery. The dispatcher performs broker I/O after claiming.
//! A broker must apply (enrollment, grant, version) idempotently, then acknowledge it.
use super::*;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notification {
    pub grant_id: String,
    pub version: i64,
    pub machine: String,
    pub generation: String,
    pub reason: String,
    pub attempt: i64,
}
impl Access {
    /// Internal dispatcher API: tenant comes from the hub's environment enumeration,
    /// never browser/tool arguments. Dispatch remains available after a person is disabled.
    pub(crate) async fn take_notifications(
        &self,
        tenant: &str,
        limit: u32,
    ) -> BridgeResult<Vec<Notification>> {
        if !(1..=100).contains(&limit) {
            return Err(invalid("notification batch must contain 1 to 100 rows"));
        }
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            Self::lock_in(c, tenant).await?;
            let now = Self::now_in(c).await?;
            let rows=sqlx::query("SELECT grant_id,version,machine,generation,reason,attempts FROM machine_access_notifications
                WHERE tenant_id=$1 AND delivered_at IS NULL AND next_attempt_at<=$2
                ORDER BY next_attempt_at,grant_id,version LIMIT $3 FOR UPDATE SKIP LOCKED")
                .bind(tenant).bind(now).bind(i64::from(limit)).fetch_all(&mut *c).await?;
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let attempt = row
                    .try_get::<i64, _>("attempts")?
                    .checked_add(1)
                    .ok_or_else(|| {
                        refused("access_damaged", "notification attempt counter overflow")
                    })?;
                let notification = Notification {
                    grant_id: row.try_get("grant_id")?,
                    version: row.try_get("version")?,
                    machine: row.try_get("machine")?,
                    generation: row.try_get("generation")?,
                    reason: row.try_get("reason")?,
                    attempt,
                };
                sqlx::query("UPDATE machine_access_notifications SET attempts=$4,next_attempt_at=$5 WHERE tenant_id=$1 AND grant_id=$2 AND version=$3")
                    .bind(tenant).bind(&notification.grant_id).bind(notification.version).bind(attempt).bind(now+30_000).execute(&mut *c).await?;
                out.push(notification);
            }
            Ok(out)
        })
    }
    /// None means the exact broker enrollment acknowledged application. Failure is a
    /// bounded diagnostic code, never a transport error string that may contain secrets.
    /// A late completion cannot overwrite a newer claim's result.
    pub(crate) async fn finish_notification(
        &self,
        tenant: &str,
        notification: &Notification,
        failure: Option<&str>,
    ) -> BridgeResult<bool> {
        if failure.is_some_and(|code| {
            code.is_empty()
                || code.len() > 64
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        }) {
            return Err(invalid(
                "notification failure must be a bounded diagnostic code",
            ));
        }
        if notification.attempt < 1 {
            return Err(invalid("notification attempt must be positive"));
        }
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            Self::lock_in(c, tenant).await?;
            let now = Self::now_in(c).await?;
            let backoff = (1000i64 << (notification.attempt - 1).min(6)).min(60_000);
            let changed=sqlx::query("UPDATE machine_access_notifications SET delivered_at=$7,last_error=$8,next_attempt_at=$9
                WHERE tenant_id=$1 AND grant_id=$2 AND version=$3 AND machine=$4 AND generation=$5 AND attempts=$6 AND delivered_at IS NULL")
                .bind(tenant).bind(&notification.grant_id).bind(notification.version).bind(&notification.machine).bind(&notification.generation)
                .bind(notification.attempt).bind(if failure.is_none(){Some(now)}else{None}).bind(failure).bind(now+backoff)
                .execute(c).await?.rows_affected();
            Ok(changed == 1)
        })
    }
}
