//! The hub's board events in ClickHouse, for statistics (ADR 0001, increment 115).
//!
//! Postgres stays the board of record. Every board mutation commits a row to its
//! `board_outbox` in the same transaction (`agenthub::hosted`); this exporter sends the rows
//! that wait to ClickHouse over its HTTP interface and marks them delivered once
//! ClickHouse has taken them. Delivery is at least once: a send whose answer is lost is sent
//! again, and the table (`ReplacingMergeTree` ordered by tenant, board and event id) keeps
//! one row per event; every query counts `uniqExact(event_id)`, so a repeat is never counted
//! twice. Nothing about a board depends on ClickHouse: while it is down the outbox holds
//! the events in Postgres, `/healthz` reports `analytics` degraded with how many wait and
//! since when, and the export drains when it is back. Board writes are never refused for
//! analytics; the bound on the backlog is Postgres's own disk, and the export drains it at
//! most `BATCH` rows per tenant and `TENANTS` tenants a round.
//!
//! The password comes from `DEVAGOTCHI_CLICKHOUSE_PASSWORD` only and is never logged or
//! reported. A ClickHouse that lost its data is filled again from Postgres's history with
//! `devagotchi hub analytics-resend`, which marks every event waiting again.

use std::{sync::Mutex, time::Duration};

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde_json::{Value, json};

use crate::{
    config::ClickHouseConfig,
    store::{Backend, Store, transaction},
};

#[cfg(test)]
mod tests;

/// At most this many events per tenant in one send.
pub const BATCH: i64 = 500;
/// At most this many tenants a round.
pub const TENANTS: i64 = 50;
/// How long an export may lag before `/healthz` calls it degraded.
pub const LAG_DEGRADED_SECS: i64 = 300;
/// How long the exporter waits between rounds when nothing waits.
const IDLE: Duration = Duration::from_secs(2);
/// The longest wait between attempts while ClickHouse refuses.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct Status {
    /// The database and table exist and the last attempt worked.
    ready: bool,
    /// Unix seconds of the last round that sent everything it took.
    last_export_at: Option<i64>,
    /// The last refusal, without anything a request carried.
    last_error: Option<String>,
    /// Attempts that failed since the last one that worked.
    failures: u32,
    /// Events sent since the hub started (repeats included).
    sent: u64,
}

/// One hub's export to ClickHouse.
pub struct Warehouse {
    store: Store,
    http: reqwest::Client,
    url: String,
    database: String,
    user: Option<String>,
    password: Option<String>,
    status: Mutex<Status>,
}

fn now_secs() -> i64 {
    crate::agenthub::system_clock()() / 1000
}

fn unavailable(message: impl Into<String>) -> BridgeError {
    BridgeError::new("analytics_unavailable", message)
}

impl Warehouse {
    pub fn new(
        config: &ClickHouseConfig,
        store: Store,
        password: Option<String>,
    ) -> BridgeResult<Self> {
        if store.backend() != Backend::Postgres {
            return Err(BridgeError::new(
                "invalid_config",
                "the ClickHouse export reads the Postgres outbox; this store is not Postgres",
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| BridgeError::new("invalid_config", error.to_string()))?;
        Ok(Self {
            store,
            http,
            url: config.url.trim_end_matches('/').to_string(),
            database: config.database.clone(),
            user: config.user.clone(),
            password: password.filter(|p| !p.is_empty()),
            status: Mutex::new(Status::default()),
        })
    }

    /// Send the rows that wait, forever, backing off while ClickHouse refuses.
    pub async fn run(self: std::sync::Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            match self.round().await {
                Ok(sent) => {
                    backoff = Duration::from_secs(1);
                    if sent == 0 {
                        tokio::time::sleep(IDLE).await;
                    }
                }
                Err(error) => {
                    eprintln!("devagotchi hub: analytics export: {}", error.message);
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }

    /// One round: the schema if it is not known to be there, then up to `BATCH` waiting
    /// events of each of up to `TENANTS` tenants, oldest first. How many were sent.
    pub async fn round(&self) -> BridgeResult<usize> {
        let ready = self.status.lock().expect("warehouse status").ready;
        let outcome = async {
            if !ready {
                self.ensure_schema().await?;
            }
            self.export_waiting().await
        }
        .await;
        let mut status = self.status.lock().expect("warehouse status");
        match &outcome {
            Ok(sent) => {
                status.ready = true;
                status.failures = 0;
                status.last_error = None;
                status.last_export_at = Some(now_secs());
                status.sent += *sent as u64;
            }
            Err(error) => {
                status.ready = false;
                status.failures = status.failures.saturating_add(1);
                status.last_error = Some(error.message.clone());
            }
        }
        outcome
    }

    async fn ensure_schema(&self) -> BridgeResult<()> {
        let db = &self.database;
        self.execute(&format!("CREATE DATABASE IF NOT EXISTS {db}"), None, &[])
            .await?;
        self.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {db}.board_events (
                   tenant_id String,
                   board_id String,
                   owner_id String,
                   project String,
                   event_id String,
                   kind LowCardinality(String),
                   seq Int64,
                   at_ms Int64,
                   at DateTime64(3, 'UTC') DEFAULT fromUnixTimestamp64Milli(at_ms),
                   actor String,
                   action LowCardinality(String),
                   ticket Nullable(String),
                   fence Nullable(Int64),
                   data String,
                   hash String,
                   exported_at DateTime64(3, 'UTC') DEFAULT now64(3)
                 ) ENGINE = ReplacingMergeTree(exported_at)
                 PARTITION BY toYYYYMM(at)
                 ORDER BY (tenant_id, board_id, event_id)"
            ),
            None,
            &[],
        )
        .await
        .map(|_| ())
    }

    async fn export_waiting(&self) -> BridgeResult<usize> {
        let tenants: Vec<String> = {
            let mut connection = self.store.read().await?;
            sqlx::query_scalar("SELECT * FROM devagotchi.outbox_pending_tenants($1)")
                .bind(TENANTS)
                .fetch_all(&mut *connection)
                .await
                .map_err(crate::store::store_error)?
        };
        let mut sent = 0;
        for tenant in tenants {
            sent += self.export_tenant(&tenant).await?;
        }
        Ok(sent)
    }

    async fn export_tenant(&self, tenant: &str) -> BridgeResult<usize> {
        type Waiting = (String, String, String, String, String, i64, String, String);
        let rows: Vec<Waiting> = transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            Ok(sqlx::query_as(
                "SELECT o.tenant_id, o.board_id, o.event_id, o.kind, o.payload, o.created_at,
                        coalesce(p.owner_id, ''), coalesce(p.project_name, o.board_id)
                   FROM board_outbox o
                   LEFT JOIN board_projects p ON p.tenant_id = o.tenant_id AND p.id = o.board_id
                  WHERE o.tenant_id = $1 AND o.delivered_at IS NULL
                  ORDER BY o.created_at, o.board_id, o.event_id
                  LIMIT $2",
            )
            .bind(tenant)
            .bind(BATCH)
            .fetch_all(&mut *connection)
            .await?)
        })?;
        if rows.is_empty() {
            return Ok(0);
        }
        let mut body = String::new();
        for (tenant_id, board_id, event_id, kind, payload, created_at, owner, project) in &rows {
            body.push_str(&row(
                tenant_id,
                board_id,
                event_id,
                kind,
                payload,
                *created_at,
                owner,
                project,
            ));
            body.push('\n');
        }
        let insert = format!(
            "INSERT INTO {}.board_events (tenant_id, board_id, owner_id, project, event_id, kind, seq, at_ms, actor, action, ticket, fence, data, hash) FORMAT JSONEachRow",
            self.database
        );
        self.execute(&insert, Some(body), &[]).await?;
        // Taken by ClickHouse: marked delivered. A crash before this line sends them again,
        // and the table keeps one row per event id.
        let delivered_at = crate::agenthub::system_clock()();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            for chunk in rows.chunks(100) {
                let mut sql = String::from(
                    "UPDATE board_outbox SET delivered_at = $2
                      WHERE tenant_id = $1 AND delivered_at IS NULL AND (",
                );
                for (i, _) in chunk.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(" OR ");
                    }
                    sql.push_str(&format!(
                        "(board_id = ${} AND event_id = ${})",
                        3 + 2 * i,
                        4 + 2 * i
                    ));
                }
                sql.push(')');
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(tenant)
                    .bind(delivered_at);
                for (_, board_id, event_id, ..) in chunk {
                    query = query.bind(board_id).bind(event_id);
                }
                query.execute(&mut *connection).await?;
            }
            Ok(())
        })?;
        Ok(rows.len())
    }

    /// Run one statement on ClickHouse: `sql` in the URL with `body` as its data (an
    /// insert), or in the body alone. `params` bind `{name:Type}` placeholders.
    async fn execute(
        &self,
        sql: &str,
        body: Option<String>,
        params: &[(&str, String)],
    ) -> BridgeResult<String> {
        let mut query: Vec<(String, String)> = vec![
            ("output_format_json_quote_64bit_integers".into(), "0".into()),
            ("date_time_output_format".into(), "iso".into()),
        ];
        for (name, value) in params {
            query.push((format!("param_{name}"), value.clone()));
        }
        let body = match body {
            Some(body) => {
                query.push(("query".into(), sql.to_string()));
                body
            }
            None => sql.to_string(),
        };
        let encode = |text: &str| {
            percent_encoding::utf8_percent_encode(text, percent_encoding::NON_ALPHANUMERIC)
                .to_string()
        };
        let query: Vec<String> = query
            .iter()
            .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
            .collect();
        let mut request = self
            .http
            .post(format!("{}/?{}", self.url, query.join("&")))
            .body(body);
        if let Some(user) = &self.user {
            request = request.header("X-ClickHouse-User", user);
        }
        if let Some(password) = &self.password {
            request = request.header("X-ClickHouse-Key", password);
        }
        let response = request.send().await.map_err(|error| {
            unavailable(format!(
                "ClickHouse at {}: {}",
                self.url,
                error.without_url()
            ))
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|error| {
            unavailable(format!(
                "ClickHouse at {}: {}",
                self.url,
                error.without_url()
            ))
        })?;
        if !status.is_success() {
            let first: String = text
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(240)
                .collect();
            return Err(unavailable(format!(
                "ClickHouse at {} answered {}: {first}",
                self.url,
                status.as_u16()
            )));
        }
        Ok(text)
    }

    async fn select(&self, sql: &str, params: &[(&str, String)]) -> BridgeResult<Vec<Value>> {
        let text = self
            .execute(&format!("{sql} FORMAT JSON"), None, params)
            .await?;
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|error| unavailable(format!("ClickHouse answered something else: {error}")))?;
        Ok(parsed["data"].as_array().cloned().unwrap_or_default())
    }

    /// What `/healthz` reports as `analytics`: never counted against the hub's health.
    pub async fn health(&self) -> Value {
        let backlog: Result<(i64, Option<i64>), _> = async {
            let mut connection = self.store.read().await?;
            sqlx::query_as("SELECT pending, oldest FROM devagotchi.outbox_backlog()")
                .fetch_one(&mut *connection)
                .await
                .map_err(crate::store::store_error)
        }
        .await;
        let status = self.status.lock().expect("warehouse status");
        let (pending, oldest_secs, backlog_error) = match backlog {
            Ok((pending, oldest)) => (
                Some(pending),
                oldest.map(|ms| (crate::agenthub::system_clock()() - ms).max(0) / 1000),
                None,
            ),
            Err(error) => (None, None, Some(error.message)),
        };
        let degraded = !status.ready
            || status.failures > 0
            || oldest_secs.is_some_and(|secs| secs > LAG_DEGRADED_SECS)
            || backlog_error.is_some();
        json!({
            "configured": true,
            "ready": status.ready,
            "degraded": degraded,
            "pending": pending,
            "oldest_pending_secs": oldest_secs,
            "sent": status.sent,
            "last_export_at": status.last_export_at,
            "last_error": status.last_error.clone().or(backlog_error),
        })
    }

    /// The statistics of one board over the last `days`: events by day and action, who
    /// acts and how much, and how long tickets took from created to completed.
    pub async fn board_stats(&self, tenant: &str, board: &str, days: u32) -> BridgeResult<Value> {
        let db = &self.database;
        let params = [
            ("tenant", tenant.to_string()),
            ("board", board.to_string()),
            ("days", days.to_string()),
        ];
        let scope = "tenant_id = {tenant:String} AND board_id = {board:String}
                     AND at >= now64(3) - toIntervalDay({days:UInt32})";
        let throughput = self
            .select(
                &format!(
                    "SELECT toString(toDate(at)) AS day, action, uniqExact(event_id) AS events
                       FROM {db}.board_events WHERE {scope}
                      GROUP BY day, action ORDER BY day, action"
                ),
                &params,
            )
            .await?;
        let actors = self
            .select(
                &format!(
                    "SELECT actor, uniqExact(event_id) AS events, toString(max(at)) AS last_at
                       FROM {db}.board_events WHERE {scope}
                      GROUP BY actor ORDER BY events DESC, actor LIMIT 50"
                ),
                &params,
            )
            .await?;
        let completion = self
            .select(
                &format!(
                    "SELECT count() AS tickets,
                            toInt64(round(avg(secs))) AS mean_secs,
                            toInt64(quantileExact(0.5)(secs)) AS median_secs
                       FROM (SELECT ticket,
                                    dateDiff('second', minIf(at, action = 'ticket.created'),
                                                       maxIf(at, action = 'ticket.completed')) AS secs
                               FROM {db}.board_events
                              WHERE {scope} AND ticket IS NOT NULL
                              GROUP BY ticket
                             HAVING countIf(action = 'ticket.created') > 0
                                AND countIf(action = 'ticket.completed') > 0)"
                ),
                &params,
            )
            .await?;
        let events = self
            .select(
                &format!(
                    "SELECT uniqExact(event_id) AS events, count() AS rows
                       FROM {db}.board_events WHERE {scope}"
                ),
                &params,
            )
            .await?;
        Ok(json!({
            "days": days,
            "events": events.first().and_then(|row| row.get("events")).cloned().unwrap_or(json!(0)),
            "throughput": throughput,
            "actors": actors,
            "completion": completion.first().cloned().unwrap_or(json!({ "tickets": 0 })),
        }))
    }

    /// How many distinct events ClickHouse holds for `tenant` (tests and probes).
    pub async fn count(&self, tenant: &str) -> BridgeResult<(i64, i64)> {
        let rows = self
            .select(
                &format!(
                    "SELECT uniqExact(event_id) AS events, count() AS rows FROM {}.board_events FINAL
                      WHERE tenant_id = {{tenant:String}}",
                    self.database
                ),
                &[("tenant", tenant.to_string())],
            )
            .await?;
        let first = rows.first().cloned().unwrap_or_default();
        Ok((
            first["events"].as_i64().unwrap_or(0),
            first["rows"].as_i64().unwrap_or(0),
        ))
    }
}

/// Mark every tenant's board events waiting again, so the export sends them all
/// (`devagotchi hub analytics-resend`). How many.
pub async fn resend(store: &Store) -> BridgeResult<i64> {
    if store.backend() != Backend::Postgres {
        return Err(BridgeError::new(
            "invalid_config",
            "the ClickHouse export reads the Postgres outbox; this store is not Postgres",
        ));
    }
    transaction!(store, |connection| {
        Ok(sqlx::query_scalar("SELECT devagotchi.outbox_resend()")
            .fetch_one(&mut *connection)
            .await?)
    })
}

/// One outbox row as a JSONEachRow line. A board event's payload is the event itself
/// (`agenthub::Event`); anything else (an import's receipt) keeps its kind as the action.
#[allow(clippy::too_many_arguments)]
fn row(
    tenant_id: &str,
    board_id: &str,
    event_id: &str,
    kind: &str,
    payload: &str,
    created_at: i64,
    owner: &str,
    project: &str,
) -> String {
    let event: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    let text = |key: &str| event.get(key).and_then(Value::as_str).map(str::to_string);
    let number = |key: &str| event.get(key).and_then(Value::as_i64);
    json!({
        "tenant_id": tenant_id,
        "board_id": board_id,
        "owner_id": owner,
        "project": project,
        "event_id": event_id,
        "kind": kind,
        "seq": number("seq").unwrap_or(0),
        "at_ms": number("at").unwrap_or(created_at),
        "actor": text("actor").unwrap_or_default(),
        "action": text("action").unwrap_or_else(|| kind.to_string()),
        "ticket": text("ticket"),
        "fence": number("fence"),
        "data": event.get("data").map(Value::to_string).unwrap_or_else(|| payload.to_string()),
        "hash": text("hash").unwrap_or_default(),
    })
    .to_string()
}

// ---- statistics over HTTP ----------------------------------------------------------

#[derive(serde::Deserialize)]
struct StatsQuery {
    #[serde(default)]
    days: Option<u32>,
}

/// `GET /api/v1/projects/{project}/stats?days=N` and
/// `GET /api/v1/orgs/{org}/projects/{project}/stats?days=N`: a board's statistics from
/// ClickHouse, for anyone who reads the board (`Hub::board_access`), over the last N days
/// (30 by default, at most 366).
pub(crate) fn routes() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/api/v1/projects/{project}/stats", get(project_stats))
        .route(
            "/api/v1/orgs/{org}/projects/{project}/stats",
            get(org_project_stats),
        )
}

async fn project_stats(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::Extension(subject): axum::Extension<crate::auth::Subject>,
    axum::extract::Path(project): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<StatsQuery>,
) -> axum::response::Response {
    stats_for(&state, &subject, project, query.days).await
}

async fn org_project_stats(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::Extension(subject): axum::Extension<crate::auth::Subject>,
    axum::extract::Path((org, project)): axum::extract::Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<StatsQuery>,
) -> axum::response::Response {
    stats_for(&state, &subject, format!("{org}/{project}"), query.days).await
}

async fn stats_for(
    state: &crate::AppState,
    subject: &crate::auth::Subject,
    project: String,
    days: Option<u32>,
) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};
    let days = days.unwrap_or(30).clamp(1, 366);
    let result: BridgeResult<Value> = async {
        let warehouse = state.hub.boards.warehouse().cloned().ok_or_else(|| {
            BridgeError::new(
                "not_offered",
                "this hub exports no statistics: it has no ClickHouse (clickhouse.url)",
            )
        })?;
        let hub = state.hub.clone();
        let who = subject.clone();
        let name = project.clone();
        let (tenant, board) = tokio::task::spawn_blocking(move || {
            let (board, _, _) = hub.board_access(&who, &name)?;
            let hosted = board.hosted_backend()?;
            Ok::<_, BridgeError>((hosted.tenant().to_string(), hosted.board_id().to_string()))
        })
        .await
        .unwrap_or_else(|join| {
            Err(BridgeError::new(
                "board_store",
                format!("board call did not complete: {join}"),
            ))
        })?;
        let mut stats = warehouse.board_stats(&tenant, &board, days).await?;
        stats["project"] = json!(project);
        Ok(stats)
    }
    .await;
    match result {
        Ok(stats) => axum::Json(stats).into_response(),
        Err(error) => {
            let status = match error.code.as_str() {
                "unknown_board" | "not_offered" => StatusCode::NOT_FOUND,
                "not_granted" | "waiting" => StatusCode::FORBIDDEN,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            };
            (status, axum::Json(error)).into_response()
        }
    }
}
