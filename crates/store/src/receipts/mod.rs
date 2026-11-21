//! Mutation receipts shared by the domains that use the PostgreSQL store.
//!
//! Owns: store_request_receipts and its separate _store_migrations history.
//! Serves: reserve/replay/complete/retire APIs inside the caller's serializable transaction.
//! Settings: explicit retention calls; identities never expire or become reusable.
//! Depends on: PostgreSQL and a caller that checks current authority BEFORE reading a
//! receipt. Callers must exclude secret issuance and external side effects from this API.

use crate::{BridgeError, BridgeResult, Failed, Migrator};
use serde::Serialize;
use serde_json::Value;
use sqlx::AnyConnection;
use std::borrow::Cow;

mod request;
#[cfg(test)]
mod tests;
pub use request::Request;

pub(crate) static MIGRATIONS: Migrator = Migrator {
    table_name: Cow::Borrowed("_store_migrations"),
    ignore_missing: true,
    ..sqlx::migrate!("./migrations")
};

const MAX_RESPONSE: usize = 1024 * 1024;
type DbResult<T> = Result<T, Failed>;

/// Fresh means the caller must commit its mutation and completion together. Replay
/// carries the original nonsecret response; no mutation or external effect runs again.
pub enum Outcome {
    Fresh(Reservation),
    Replay(Value),
}

/// Can only be obtained from a reservation in the current transaction.
pub struct Reservation(Request);

async fn scoped(connection: &mut AnyConnection, tenant: &str) -> DbResult<()> {
    let scope: Option<String> =
        sqlx::query_scalar("SELECT current_setting('devagotchi.tenant',true)")
            .fetch_one(connection)
            .await?;
    if scope.as_deref() != Some(tenant) {
        return Err(BridgeError::new(
            "not_granted",
            "request receipt requires the current environment scope",
        )
        .into());
    }
    Ok(())
}

/// Authentication, account/project grants and current policy must already be checked
/// in this transaction. Request identity includes the operation's resource identifiers.
pub async fn begin_in(connection: &mut AnyConnection, request: &Request) -> DbResult<Outcome> {
    scoped(connection, &request.tenant).await?;
    let inserted = sqlx::query("INSERT INTO store_request_receipts (tenant_id,principal,request_key,operation,request_hash) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (tenant_id,principal,request_key) DO NOTHING")
        .bind(&request.tenant).bind(&request.principal).bind(&request.key)
        .bind(&request.operation).bind(&request.digest).execute(&mut *connection).await?.rows_affected();
    if inserted == 1 {
        return Ok(Outcome::Fresh(Reservation(request.clone())));
    }
    let (operation, digest, response, retired): (String,String,Option<String>,Option<i64>) = sqlx::query_as(
        "SELECT operation,request_hash,response,retired_at FROM store_request_receipts WHERE tenant_id=$1 AND principal=$2 AND request_key=$3")
        .bind(&request.tenant).bind(&request.principal).bind(&request.key).fetch_one(connection).await?;
    if retired.is_some() {
        return Err(BridgeError::new(
            "request_retired",
            "this request key is permanently retired; reconcile its original result",
        )
        .into());
    }
    if operation != request.operation || digest != request.digest {
        return Err(BridgeError::new(
            "request_conflict",
            "request key was already used for another operation or content",
        )
        .into());
    }
    let response = response.ok_or_else(|| {
        BridgeError::new(
            "receipt_incomplete",
            "request is already reserved in this transaction",
        )
    })?;
    if response.len() > MAX_RESPONSE {
        return Err(BridgeError::new(
            "receipt_damaged",
            "stored request response exceeds its bound",
        )
        .into());
    }
    let value = serde_json::from_str(&response)
        .map_err(|_| BridgeError::new("receipt_damaged", "invalid stored request response"))?;
    Ok(Outcome::Replay(value))
}

impl Reservation {
    /// Save a nonsecret domain response on the same connection as its mutation and audit.
    /// Failure rolls everything back. Issued tokens, keys and passwords must never enter it.
    pub async fn complete_in<T: Serialize>(
        self,
        connection: &mut AnyConnection,
        response: &T,
    ) -> DbResult<()> {
        scoped(connection, &self.0.tenant).await?;
        let response = serde_json::to_string(response)
            .map_err(|_| BridgeError::new("invalid", "request response cannot be encoded"))?;
        if response.len() > MAX_RESPONSE {
            return Err(BridgeError::new("invalid", "request response exceeds its bound").into());
        }
        let changed = sqlx::query("UPDATE store_request_receipts SET response=$6,completed_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE tenant_id=$1 AND principal=$2 AND request_key=$3 AND operation=$4 AND request_hash=$5 AND completed_at IS NULL")
            .bind(&self.0.tenant).bind(&self.0.principal).bind(&self.0.key)
            .bind(&self.0.operation).bind(&self.0.digest).bind(response).execute(connection).await?.rows_affected();
        if changed != 1 {
            return Err(BridgeError::new(
                "receipt_incomplete",
                "request reservation is not pending in this transaction",
            )
            .into());
        }
        Ok(())
    }
}

/// Discard bounded pages of old responses while preserving every key forever. The caller
/// owns retention authority and any required archive-before-retire process. Cutoff is UTC
/// milliseconds; no automatic retention runs merely because the store was opened.
pub async fn retire_before_in(
    connection: &mut AnyConnection,
    tenant: &str,
    cutoff: i64,
    limit: u32,
) -> DbResult<u64> {
    if !crate::valid_tenant_id(tenant) || cutoff < 0 || !(1..=1000).contains(&limit) {
        return Err(BridgeError::new("invalid", "invalid receipt retirement bounds").into());
    }
    scoped(connection, tenant).await?;
    Ok(sqlx::query("WITH selected AS (SELECT tenant_id,principal,request_key FROM store_request_receipts WHERE tenant_id=$1 AND retired_at IS NULL AND completed_at<$2 ORDER BY completed_at,principal,request_key LIMIT $3 FOR UPDATE SKIP LOCKED) UPDATE store_request_receipts r SET response=NULL,retired_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM selected s WHERE r.tenant_id=s.tenant_id AND r.principal=s.principal AND r.request_key=s.request_key")
        .bind(tenant).bind(cutoff).bind(i64::from(limit)).execute(connection).await?.rows_affected())
}
