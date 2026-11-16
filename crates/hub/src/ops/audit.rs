//! Append-only, hash-chained audit of every routed call, in the hub's Postgres
//! (`audit_entries`, migration 0027). The chain is verified on health checks so a truncated
//! or edited log is visible, not silent. Appends take one advisory lock, so every hub
//! instance and an operator command writing at the same moment extend one chain.

use devagotchi_contracts::{BridgeError, BridgeResult};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::store::{Store, transaction};

/// The advisory lock every append holds: "devagotchi audit" as a number.
const CHAIN_LOCK: i64 = 0x6476_6761_7564_6974;

pub struct Audit {
    store: Store,
}

#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    pub subject: &'a str,
    pub via: &'a str,
    pub tool: &'a str,
    pub broker: &'a str,
    pub operation_id: &'a str,
    pub outcome: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChainStatus {
    pub entries: u64,
    pub ok: bool,
    pub head: String,
}

/// One recorded call, as the signal sinks read it (`notify::signals`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub seq: u64,
    pub at: String,
    pub subject: String,
    pub via: String,
    pub tool: String,
    pub broker: String,
    pub operation_id: String,
    pub outcome: String,
    pub hash: String,
}

/// One subject's calls of one tool to one machine with one outcome (`Audit::usage_of`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRow {
    pub tool: String,
    pub broker: String,
    pub outcome: String,
    pub calls: u64,
    pub first_at: String,
    pub last_at: String,
}

fn recorded(row: &sqlx::any::AnyRow) -> Result<Recorded, sqlx::Error> {
    Ok(Recorded {
        seq: u64::try_from(row.try_get::<i64, _>("seq")?).unwrap_or(0),
        at: row.try_get("at")?,
        subject: row.try_get("subject")?,
        via: row.try_get("via")?,
        tool: row.try_get("tool")?,
        broker: row.try_get("broker")?,
        operation_id: row.try_get("operation_id")?,
        outcome: row.try_get("outcome")?,
        hash: row.try_get("hash")?,
    })
}

fn read_error(error: impl std::fmt::Display) -> BridgeError {
    BridgeError::new("audit_read", error.to_string())
}

impl Audit {
    /// The chain in the hub's store.
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// The store the chain is kept in: the hub's Postgres, which the lexicon shares.
    pub fn store(&self) -> &Store {
        &self.store
    }

    pub async fn record(&self, entry: &AuditEntry<'_>) -> BridgeResult<u64> {
        let at = now_rfc3339();
        transaction!(&self.store, |connection| {
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(CHAIN_LOCK)
                .execute(&mut *connection)
                .await?;
            let prev: Option<String> =
                sqlx::query_scalar("SELECT hash FROM audit_entries ORDER BY seq DESC LIMIT 1")
                    .fetch_optional(&mut *connection)
                    .await?;
            let prev = prev.unwrap_or_else(|| "0".repeat(64));
            let hash = row_hash(&prev, &at, entry);
            let seq: i64 = sqlx::query_scalar(
                "INSERT INTO audit_entries
                   (at, subject, via, tool, broker, operation_id, outcome, prev_hash, hash)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING seq",
            )
            .bind(&at)
            .bind(entry.subject)
            .bind(entry.via)
            .bind(entry.tool)
            .bind(entry.broker)
            .bind(entry.operation_id)
            .bind(entry.outcome)
            .bind(&prev)
            .bind(&hash)
            .fetch_one(&mut *connection)
            .await?;
            Ok(u64::try_from(seq).unwrap_or(0))
        })
        .map_err(|error| BridgeError::new("audit_write", error.message))
    }

    /// Up to `limit` entries after `seq`, oldest first: what the signal sinks read.
    pub async fn after(&self, seq: u64, limit: usize) -> BridgeResult<Vec<Recorded>> {
        let mut connection = self.store.read().await?;
        let rows = sqlx::query(
            "SELECT seq, at, subject, via, tool, broker, operation_id, outcome, hash
             FROM audit_entries WHERE seq > $1 ORDER BY seq LIMIT $2",
        )
        .bind(i64::try_from(seq).unwrap_or(i64::MAX))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&mut *connection)
        .await
        .map_err(read_error)?;
        rows.iter()
            .map(recorded)
            .collect::<Result<_, sqlx::Error>>()
            .map_err(read_error)
    }

    /// One subject's calls, newest first: up to `limit` of them, with how many matched.
    /// `tool` and `broker` narrow them. What `get_recent_tool_calls` shows.
    pub async fn calls_of(
        &self,
        subject: &str,
        tool: Option<&str>,
        broker: Option<&str>,
        limit: u32,
    ) -> BridgeResult<(Vec<Recorded>, u64)> {
        let mut connection = self.store.read().await?;
        let matched: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_entries
             WHERE subject = $1 AND ($2::TEXT IS NULL OR tool = $2)
               AND ($3::TEXT IS NULL OR broker = $3)",
        )
        .bind(subject)
        .bind(tool)
        .bind(broker)
        .fetch_one(&mut *connection)
        .await
        .map_err(read_error)?;
        let rows = sqlx::query(
            "SELECT seq, at, subject, via, tool, broker, operation_id, outcome, hash
             FROM audit_entries
             WHERE subject = $1 AND ($2::TEXT IS NULL OR tool = $2)
               AND ($3::TEXT IS NULL OR broker = $3)
             ORDER BY seq DESC LIMIT $4",
        )
        .bind(subject)
        .bind(tool)
        .bind(broker)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .map_err(read_error)?;
        let calls = rows
            .iter()
            .map(recorded)
            .collect::<Result<_, sqlx::Error>>();
        Ok((
            calls.map_err(read_error)?,
            u64::try_from(matched).unwrap_or(0),
        ))
    }

    /// One subject's calls counted by tool, machine and outcome: what `get_usage_stats`
    /// tallies. One row per group, with its count and its first and latest call.
    pub async fn usage_of(&self, subject: &str) -> BridgeResult<Vec<UsageRow>> {
        let mut connection = self.store.read().await?;
        let rows = sqlx::query(
            "SELECT tool, broker, outcome, COUNT(*) AS calls, MIN(at) AS first_at,
                    MAX(at) AS last_at
             FROM audit_entries WHERE subject = $1 GROUP BY tool, broker, outcome",
        )
        .bind(subject)
        .fetch_all(&mut *connection)
        .await
        .map_err(read_error)?;
        rows.iter()
            .map(|row| {
                Ok(UsageRow {
                    tool: row.try_get("tool")?,
                    broker: row.try_get("broker")?,
                    outcome: row.try_get("outcome")?,
                    calls: u64::try_from(row.try_get::<i64, _>("calls")?).unwrap_or(0),
                    first_at: row.try_get("first_at")?,
                    last_at: row.try_get("last_at")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(read_error)
    }

    /// The last entry's seq (0 for none): how far a signal sink can be behind.
    #[cfg(test)]
    pub async fn last_seq(&self) -> BridgeResult<u64> {
        let mut connection = self.store.read().await?;
        let seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM audit_entries")
            .fetch_one(&mut *connection)
            .await
            .map_err(read_error)?;
        Ok(u64::try_from(seq).unwrap_or(0))
    }

    /// Walk the whole chain. Small logs only; a paged walk arrives with the audit UI.
    pub async fn verify(&self) -> BridgeResult<ChainStatus> {
        let mut connection = self.store.read().await?;
        let rows = sqlx::query(
            "SELECT at, subject, via, tool, broker, operation_id, outcome, prev_hash, hash
             FROM audit_entries ORDER BY seq",
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(read_error)?;
        let mut prev = "0".repeat(64);
        let mut entries = 0u64;
        let mut ok = true;
        for row in &rows {
            let text = |column: &str| row.try_get::<String, _>(column).map_err(read_error);
            let (at, prev_hash, hash) = (text("at")?, text("prev_hash")?, text("hash")?);
            let (subject, via, tool) = (text("subject")?, text("via")?, text("tool")?);
            let (broker, operation_id, outcome) =
                (text("broker")?, text("operation_id")?, text("outcome")?);
            let entry = AuditEntry {
                subject: &subject,
                via: &via,
                tool: &tool,
                broker: &broker,
                operation_id: &operation_id,
                outcome: &outcome,
            };
            if prev_hash != prev || row_hash(&prev, &at, &entry) != hash {
                ok = false;
            }
            prev = hash;
            entries += 1;
        }
        Ok(ChainStatus {
            entries,
            ok,
            head: prev,
        })
    }
}

impl crate::mcp::Hub {
    /// Record one call in the audit chain. A call that cannot be audited is not a call
    /// this bastion made, so a failure here fails the call. An operation that started
    /// (`operation_start`, or `run`) is also its tenant's operation milestone
    /// (`analytics::funnel`): the first, and the latest a day at a time.
    pub(crate) async fn record(
        &self,
        subject: &crate::identity::Subject,
        tool: &str,
        broker: &str,
        operation_id: &str,
        outcome: &str,
    ) -> Result<(), rmcp::Json<BridgeError>> {
        if let (Some(analytics), Some(tenant)) = (&self.analytics, &subject.tenant)
            && matches!(
                (tool, outcome),
                ("operation_start", "ok") | ("run", "ok") | ("run", "running")
            )
        {
            analytics.tenant_event(tenant, crate::ops::analytics::funnel::OPERATION);
        }
        self.audit
            .record(&AuditEntry {
                subject: &subject.id,
                via: subject.via,
                tool,
                broker,
                operation_id,
                outcome,
            })
            .await
            .map(|_| ())
            .map_err(rmcp::Json)
    }
}

fn row_hash(prev: &str, at: &str, entry: &AuditEntry<'_>) -> String {
    let mut hasher = Sha256::new();
    for part in [
        prev,
        at,
        entry.subject,
        entry.via,
        entry.tool,
        entry.broker,
        entry.operation_id,
        entry.outcome,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(hasher.finalize())
}

fn now_rfc3339() -> String {
    devagotchi_contracts::utc_rfc3339(std::time::SystemTime::now())
}

/// A chain on a new database of the test server, or `None` without one.
#[cfg(test)]
pub(crate) async fn test_audit() -> Option<std::sync::Arc<Audit>> {
    let test = crate::store::testing::postgres().await?;
    Some(std::sync::Arc::new(Audit::new(test.store)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> AuditEntry<'_> {
        AuditEntry {
            subject: "jesse",
            via: "dev",
            tool: "operation_start",
            broker: "gold-ws",
            operation_id: id,
            outcome: "ok",
        }
    }

    #[tokio::test]
    async fn chain_links_rows_and_detects_tampering() {
        let audit = or_skip!(test_audit());
        for i in 0..3 {
            audit.record(&entry(&format!("op-{i}"))).await.unwrap();
        }
        let status = audit.verify().await.unwrap();
        assert_eq!(status.entries, 3);
        assert!(status.ok);
        // The hub's role may not change a row; the table's owner can, and verify sees it.
        sqlx::query("UPDATE devagotchi.audit_entries SET outcome = 'denied' WHERE seq = 2")
            .execute(audit.store.admin())
            .await
            .unwrap();
        let tampered = audit.verify().await.unwrap();
        assert_eq!(tampered.entries, 3);
        assert!(!tampered.ok);
    }

    #[tokio::test]
    async fn many_writers_at_once_keep_one_chain() {
        let audit = or_skip!(test_audit());
        let writers = (0..16).map(|i| {
            let audit = audit.clone();
            tokio::spawn(async move { audit.record(&entry(&format!("op-{i}"))).await.unwrap() })
        });
        for writer in writers {
            writer.await.unwrap();
        }
        let status = audit.verify().await.unwrap();
        assert_eq!((status.entries, status.ok), (16, true));
        assert_eq!(audit.last_seq().await.unwrap(), 16);
        assert_eq!(audit.after(14, 10).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_subjects_calls_are_its_own_newest_first() {
        let audit = or_skip!(test_audit());
        for i in 0..3 {
            audit.record(&entry(&format!("op-{i}"))).await.unwrap();
        }
        let other = AuditEntry {
            subject: "someone",
            ..entry("op-9")
        };
        audit.record(&other).await.unwrap();
        let failed = AuditEntry {
            tool: "read_file",
            outcome: "path_not_found",
            ..entry("/x")
        };
        audit.record(&failed).await.unwrap();
        let (calls, matched) = audit.calls_of("jesse", None, None, 2).await.unwrap();
        assert_eq!(matched, 4);
        assert_eq!(
            calls
                .iter()
                .map(|c| c.operation_id.as_str())
                .collect::<Vec<_>>(),
            ["/x", "op-2"]
        );
        let (calls, matched) = audit
            .calls_of("jesse", Some("read_file"), Some("gold-ws"), 9)
            .await
            .unwrap();
        assert_eq!((calls.len(), matched), (1, 1));
        let usage = audit.usage_of("jesse").await.unwrap();
        assert_eq!(usage.iter().map(|row| row.calls).sum::<u64>(), 4);
        assert!(
            usage
                .iter()
                .any(|row| row.outcome == "path_not_found" && row.calls == 1)
        );
    }

    #[test]
    fn timestamp_is_rfc3339_utc() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), 24);
        assert!(ts.ends_with('Z') && ts.chars().nth(10) == Some('T'));
    }
}
