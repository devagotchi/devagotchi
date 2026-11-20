//! Bounded, filtered history for the hub owner's activity page.
use super::*;

#[derive(Default)]
pub struct HistoryFilter {
    pub subject: Option<String>,
    pub action_prefix: Option<String>,
    pub broker: Option<String>,
    pub outcome: Option<String>,
    pub before: Option<u64>,
    pub limit: usize,
}

impl Audit {
    /// All subjects' history. The caller must require the hub owner's authority.
    pub async fn history(&self, filter: &HistoryFilter) -> BridgeResult<Vec<Recorded>> {
        if filter.before == Some(0) {
            return Err(BridgeError::new("invalid", "before must be positive"));
        }
        let before = filter.before.map(|n| i64::try_from(n).unwrap_or(i64::MAX));
        let action = filter
            .action_prefix
            .as_deref()
            .map(|s| s.strip_suffix('*').unwrap_or(s));
        let mut connection = self.store.read().await?;
        let rows = sqlx::query(
            "SELECT seq, at, subject, via, tool, broker, operation_id, outcome, hash
             FROM audit_entries
             WHERE ($1::bigint IS NULL OR seq<$1)
               AND ($2::text IS NULL OR subject=$2)
               AND ($3::text IS NULL OR left(tool,char_length($3))=$3)
               AND ($4::text IS NULL OR broker=$4)
               AND ($5::text IS NULL OR outcome=$5)
             ORDER BY seq DESC LIMIT $6",
        )
        .bind(before)
        .bind(&filter.subject)
        .bind(action)
        .bind(&filter.broker)
        .bind(&filter.outcome)
        .bind(filter.limit.clamp(1, 1000) as i64)
        .fetch_all(&mut *connection)
        .await
        .map_err(read_error)?;
        rows.iter()
            .map(recorded)
            .collect::<Result<_, _>>()
            .map_err(read_error)
    }

    pub fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }
}
