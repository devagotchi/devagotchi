//! The phones and browsers a person turned notifications on from (Web Push), in
//! `push_subscriptions` (migration 0026). Kept with the machine registry until the notify
//! domain takes them (structure-cleanup S4).

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use super::{Devices, now_secs};
use crate::store::{secs, transaction};

/// A phone or browser a person turned notifications on from (Web Push). The keys are the
/// browser's public half; the hub cannot read anything with them, only encrypt to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSubscription {
    pub endpoint: String,
    pub subject: String,
    pub p256dh: String,
    pub auth: String,
}

impl Devices {
    /// Remember a push subscription for `subject`; the same endpoint again replaces it.
    pub async fn add_push(
        &self,
        subject: &str,
        endpoint: &str,
        p256dh: &str,
        auth: &str,
    ) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            sqlx::query(
                "INSERT INTO push_subscriptions (endpoint, subject, p256dh, auth, created_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (endpoint) DO UPDATE SET subject = $2, p256dh = $3, auth = $4",
            )
            .bind(endpoint)
            .bind(subject)
            .bind(p256dh)
            .bind(auth)
            .bind(secs(now_secs()))
            .execute(&mut *connection)
            .await?;
            Ok(())
        })
    }

    pub async fn push_for(&self, subject: &str) -> BridgeResult<Vec<PushSubscription>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query(
                "SELECT endpoint, subject, p256dh, auth FROM push_subscriptions
                 WHERE subject = $1 ORDER BY created_at, endpoint",
            )
            .bind(subject)
            .fetch_all(&mut *connection)
            .await?;
            let mut out = Vec::with_capacity(rows.len());
            for row in &rows {
                out.push(PushSubscription {
                    endpoint: row.try_get("endpoint")?,
                    subject: row.try_get("subject")?,
                    p256dh: row.try_get("p256dh")?,
                    auth: row.try_get("auth")?,
                });
            }
            Ok(out)
        })
    }

    pub async fn drop_push(&self, endpoint: &str) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = $1")
                .bind(endpoint)
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
    }
}
