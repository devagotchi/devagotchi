//! Read the existing audit chain; no mutation or independent storage.
use super::*;
use futures_util::TryStreamExt;

impl Audit {
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
    pub async fn last_seq(&self) -> BridgeResult<u64> {
        let mut connection = self.store.read().await?;
        let seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM audit_entries")
            .fetch_one(&mut *connection)
            .await
            .map_err(read_error)?;
        Ok(u64::try_from(seq).unwrap_or(0))
    }

    /// Stream the whole chain from one database snapshot, without collecting the log.
    pub async fn verify(&self) -> BridgeResult<ChainStatus> {
        let mut connection = self.store.read().await?;
        let mut rows = sqlx::query(
            "SELECT at, subject, via, tool, broker, operation_id, outcome, prev_hash, hash
             FROM audit_entries ORDER BY seq",
        )
        .fetch(&mut *connection);
        let mut prev = "0".repeat(64);
        let mut entries = 0u64;
        let mut ok = true;
        while let Some(row) = rows.try_next().await.map_err(read_error)? {
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
