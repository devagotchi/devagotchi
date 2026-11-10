//! The signal contract, version 1 (increment 117, `docs/signals.md`): one stream of what
//! happens on the hub, in one shape, for whatever the operator points it at.
//!
//! Three sources, each append-only and ordered by its own sequence as it commits:
//! - `board`: every board's events (`board_events`: tickets, notes, claims, approvals,
//!   issue context), one board at a time by `seq`;
//! - `tenancy`: each tenant's own chain (`tenancy_events`: organisations, members, roles,
//!   teams, invites, accounts), by `seq`;
//! - `audit`: the hub's audit chain of every tool call and its outcome, by `seq`.
//!
//! A sink (`signals.sinks` in the config) is an HTTPS endpoint that takes batches of up to
//! `BATCH` signals as NDJSON. Each sink keeps its own position per source and scope
//! (`signal_cursors`), so it is sent each signal at least once, in order within a scope, and
//! a sink that is down falls behind without dropping anything and without holding back the
//! hub or any other sink. Every signal carries a stable `id`; a receiver deduplicates by it.
//! A sink takes `metadata` (the default: no `data`) or `full`. The hub never needs a sink: it
//! runs each one on its own task, backs off up to a minute while it refuses, and reports each
//! under `/healthz` `signals`, never counted against the hub's own health.

use std::{sync::Arc, sync::Mutex, time::Duration};

use devagotchi_contracts::{BridgeError, BridgeResult};
use hmac::{Hmac, KeyInit, Mac};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::{
    audit::Audit,
    config::{SinkConfig, SinkFields},
    store::{Backend, Store, transaction},
};

#[cfg(test)]
mod tests;

/// The contract a signal follows: what `schema` says and what `docs/signals.md` describes.
pub const SCHEMA: &str = "devagotchi.signal/v1";
/// At most this many signals in one batch.
pub const BATCH: usize = 500;
const IDLE: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Where a signal comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Board,
    Tenancy,
    Audit,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Board => "board",
            Source::Tenancy => "tenancy",
            Source::Audit => "audit",
        }
    }
}

/// One thing that happened on the hub (contract `devagotchi.signal/v1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Signal {
    /// Always `devagotchi.signal/v1` for this shape.
    pub schema: String,
    /// Stable and unique: `board:<tenant>:<board>:<seq>:<hash>`, `tenancy:<tenant>:<seq>:<hash>`
    /// or `audit:<seq>:<hash>` (the hash's first 16 hex digits). Sent again, it is the same.
    pub id: String,
    pub source: Source,
    /// What happened: a board action (`ticket.claimed`, `approval.decided` …), a tenant
    /// event (`org.member_set` …), or `tool.<name>` for a tool call.
    pub kind: String,
    /// When, in milliseconds since the Unix epoch.
    pub at_ms: i64,
    /// The tenant it happened in (`t-<uuid>`); absent for the hub's audit chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// Whose board: an account, `org-<tenant>` for an organisation, `@shared` for a
    /// configured board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The board's project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Who did it: an account, an agent seat (`<account>/<seat>`), or `hub`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// The ticket it concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// What a tool call was aimed at: a broker or a board's project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The operation a tool call named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// How a tool call ended: `ok`, or the refusal's code (`read_only`, `not_granted` …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// The rest, only to a sink that takes `full`: a board event's data, a tenant event's
    /// detail, a tool call's `via`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Signal {
    fn new(id: String, source: Source, kind: String, at_ms: i64) -> Self {
        Self {
            schema: SCHEMA.into(),
            id,
            source,
            kind,
            at_ms,
            tenant: None,
            owner: None,
            project: None,
            actor: None,
            ticket: None,
            target: None,
            operation: None,
            outcome: None,
            data: None,
        }
    }
}

/// The contract as JSON Schema, generated from `Signal` (`docs/signals/v1.schema.json`
/// holds the same; a test keeps them equal).
pub fn json_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Signal)).expect("signal schema")
}

/// Milliseconds since the epoch of the audit chain's `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn rfc3339_ms(at: &str) -> Option<i64> {
    let b = at.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || !at.ends_with('Z') {
        return None;
    }
    let n = |range: std::ops::Range<usize>| at.get(range)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    let ms = if b.get(19) == Some(&b'.') {
        n(20..23).unwrap_or(0)
    } else {
        0
    };
    // Days from the civil date (Howard Hinnant's algorithm), as audit.rs writes it.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + hh * 3600 + mm * 60 + ss) * 1000) + ms)
}

fn hash16(hash: &str) -> &str {
    hash.get(..16).unwrap_or(hash)
}

#[derive(Debug, Default)]
struct Status {
    ready: bool,
    last_sent_at: Option<i64>,
    last_error: Option<String>,
    failures: u32,
    sent: u64,
    /// Signals the last attempt found waiting and could not deliver.
    waiting: u64,
    oldest_waiting_ms: Option<i64>,
}

/// One sink: where its batches go and where it is up to.
pub struct Sink {
    name: String,
    url: String,
    fields: SinkFields,
    sources: Vec<Source>,
    token: Option<String>,
    secret: Option<String>,
    store: Store,
    audit: Arc<Audit>,
    http: reqwest::Client,
    status: Mutex<Status>,
}

/// A batch on its way: the signals, and where each scope it came from stands once sent.
#[derive(Default)]
struct Batch {
    signals: Vec<Signal>,
    moves: Vec<(Source, String, String)>,
}

impl Sink {
    /// `env` answers the token and secret variables the config names.
    pub fn new(
        config: &SinkConfig,
        store: Store,
        audit: Arc<Audit>,
        env: impl Fn(&str) -> Option<String>,
    ) -> BridgeResult<Self> {
        if store.backend() != Backend::Postgres {
            return Err(BridgeError::new(
                "invalid_config",
                "signal sinks read the hub's Postgres; this store is not Postgres",
            ));
        }
        let read = |name: &Option<String>| -> BridgeResult<Option<String>> {
            match name {
                None => Ok(None),
                Some(name) => env(name)
                    .filter(|v| !v.is_empty())
                    .map(Some)
                    .ok_or_else(|| {
                        BridgeError::new(
                            "invalid_config",
                            format!("signals.sinks {}: {name} is not set", config.name),
                        )
                    }),
            }
        };
        let sources = if config.sources.is_empty() {
            vec![Source::Board, Source::Tenancy, Source::Audit]
        } else {
            config
                .sources
                .iter()
                .filter_map(|s| match s.as_str() {
                    "board" => Some(Source::Board),
                    "tenancy" => Some(Source::Tenancy),
                    "audit" => Some(Source::Audit),
                    _ => None,
                })
                .collect()
        };
        Ok(Self {
            name: config.name.clone(),
            url: config.url.clone(),
            fields: config.fields,
            sources,
            token: read(&config.token_env)?,
            secret: read(&config.secret_env)?,
            store,
            audit,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|error| BridgeError::new("invalid_config", error.to_string()))?,
            status: Mutex::new(Status::default()),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Send what waits, forever, backing off while the sink refuses.
    pub async fn run(self: Arc<Self>) {
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
                    eprintln!(
                        "devagotchi hub: signal sink {}: {}",
                        self.name, error.message
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }

    /// One round over every source this sink takes; how many signals it sent.
    pub async fn round(&self) -> BridgeResult<usize> {
        let mut batch = Batch::default();
        let mut sent = 0;
        let outcome: BridgeResult<()> = async {
            for source in self.sources.clone() {
                match source {
                    Source::Board => self.board(&mut batch, &mut sent).await?,
                    Source::Tenancy => self.tenancy(&mut batch, &mut sent).await?,
                    Source::Audit => self.audit_chain(&mut batch, &mut sent).await?,
                }
            }
            self.flush(&mut batch, &mut sent).await
        }
        .await;
        let mut status = self.status.lock().expect("sink status");
        match &outcome {
            Ok(()) => {
                status.ready = true;
                status.failures = 0;
                status.last_error = None;
                status.waiting = 0;
                status.oldest_waiting_ms = None;
                if sent > 0 {
                    status.last_sent_at = Some(crate::board::system_clock()() / 1000);
                }
                status.sent += sent as u64;
            }
            Err(error) => {
                status.ready = false;
                status.failures = status.failures.saturating_add(1);
                status.last_error = Some(error.message.clone());
                status.waiting = batch.signals.len() as u64;
                status.oldest_waiting_ms = batch.signals.iter().map(|s| s.at_ms).min();
                status.sent += sent as u64;
            }
        }
        outcome.map(|()| sent)
    }

    async fn tenants(&self) -> BridgeResult<Vec<String>> {
        let mut connection = self.store.read().await?;
        sqlx::query_scalar("SELECT id FROM tenancy_tenants ORDER BY id")
            .fetch_all(&mut *connection)
            .await
            .map_err(crate::store::store_error)
    }

    async fn position(&self, source: Source, scope: &str) -> BridgeResult<i64> {
        let mut connection = self.store.read().await?;
        let position: Option<String> = sqlx::query_scalar(
            "SELECT position FROM signal_cursors WHERE sink = $1 AND source = $2 AND scope = $3",
        )
        .bind(&self.name)
        .bind(source.as_str())
        .bind(scope)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::store::store_error)?;
        Ok(position.and_then(|p| p.parse().ok()).unwrap_or(0))
    }

    /// Send what the batch holds, then record where each scope stands. A send that fails
    /// moves nothing: the next round sends the same signals again.
    async fn flush(&self, batch: &mut Batch, sent: &mut usize) -> BridgeResult<()> {
        if batch.signals.is_empty() {
            return Ok(());
        }
        self.push(&batch.signals).await?;
        let now = crate::board::system_clock()();
        let moves = std::mem::take(&mut batch.moves);
        transaction!(&self.store, |connection| {
            for (source, scope, position) in &moves {
                sqlx::query(
                    "INSERT INTO signal_cursors (sink, source, scope, position, updated_at)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (sink, source, scope)
                     DO UPDATE SET position = excluded.position, updated_at = excluded.updated_at",
                )
                .bind(&self.name)
                .bind(source.as_str())
                .bind(scope)
                .bind(position)
                .bind(now)
                .execute(&mut *connection)
                .await?;
            }
            Ok(())
        })?;
        *sent += batch.signals.len();
        batch.signals.clear();
        Ok(())
    }

    fn keep(&self, mut signal: Signal) -> Signal {
        if self.fields == SinkFields::Metadata {
            signal.data = None;
        }
        signal
    }

    async fn board(&self, batch: &mut Batch, sent: &mut usize) -> BridgeResult<()> {
        type Row = (
            i64,
            i64,
            String,
            String,
            Option<String>,
            String,
            Option<i64>,
            String,
        );
        for tenant in self.tenants().await? {
            let boards: Vec<(String, Option<String>, Option<String>)> =
                transaction!(&self.store, |connection| {
                    self.store.scope(connection, &tenant).await?;
                    Ok(sqlx::query_as(
                        "SELECT id, owner_id, project_name FROM board_projects
                         WHERE tenant_id = $1 ORDER BY id",
                    )
                    .bind(&tenant)
                    .fetch_all(&mut *connection)
                    .await?)
                })?;
            for (board, owner, project) in boards {
                let scope = format!("{tenant}/{board}");
                let after = self.position(Source::Board, &scope).await?;
                let room = (BATCH - batch.signals.len()).max(1) as i64;
                let rows: Vec<Row> = transaction!(&self.store, |connection| {
                    self.store.scope(connection, &tenant).await?;
                    Ok(sqlx::query_as(
                        "SELECT seq, at, actor, action, ticket, data, fence, hash FROM board_events
                         WHERE tenant_id = $1 AND board_id = $2 AND seq > $3 ORDER BY seq LIMIT $4",
                    )
                    .bind(&tenant)
                    .bind(&board)
                    .bind(after)
                    .bind(room)
                    .fetch_all(&mut *connection)
                    .await?)
                })?;
                let Some(last) = rows.last().map(|row| row.0) else {
                    continue;
                };
                for (seq, at, actor, action, ticket, data, fence, hash) in rows {
                    let mut signal = Signal::new(
                        format!("board:{tenant}:{board}:{seq}:{}", hash16(&hash)),
                        Source::Board,
                        action,
                        at,
                    );
                    signal.tenant = Some(tenant.clone());
                    signal.owner = owner.clone();
                    signal.project = Some(project.clone().unwrap_or_else(|| board.clone()));
                    signal.actor = Some(actor);
                    signal.ticket = ticket;
                    let mut data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
                    if let Some(fence) = fence
                        && let Some(object) = data.as_object_mut()
                    {
                        object.insert("fence".into(), json!(fence));
                    }
                    signal.data = Some(data);
                    batch.signals.push(self.keep(signal));
                }
                batch.moves.push((Source::Board, scope, last.to_string()));
                if batch.signals.len() >= BATCH {
                    self.flush(batch, sent).await?;
                }
            }
        }
        Ok(())
    }

    async fn tenancy(&self, batch: &mut Batch, sent: &mut usize) -> BridgeResult<()> {
        for tenant in self.tenants().await? {
            let after = self.position(Source::Tenancy, &tenant).await?;
            let room = (BATCH - batch.signals.len()).max(1) as i64;
            let rows: Vec<(i64, i64, String, String, String, String)> =
                transaction!(&self.store, |connection| {
                    self.store.scope(connection, &tenant).await?;
                    Ok(sqlx::query_as(
                        "SELECT seq, at, actor, event, detail, hash FROM tenancy_events
                         WHERE tenant_id = $1 AND seq > $2 ORDER BY seq LIMIT $3",
                    )
                    .bind(&tenant)
                    .bind(after)
                    .bind(room)
                    .fetch_all(&mut *connection)
                    .await?)
                })?;
            let Some(last) = rows.last().map(|row| row.0) else {
                continue;
            };
            for (seq, at, actor, event, detail, hash) in rows {
                let mut signal = Signal::new(
                    format!("tenancy:{tenant}:{seq}:{}", hash16(&hash)),
                    Source::Tenancy,
                    event,
                    at.saturating_mul(1000),
                );
                signal.tenant = Some(tenant.clone());
                signal.actor = Some(actor);
                signal.data = Some(json!({ "detail": detail }));
                batch.signals.push(self.keep(signal));
            }
            batch
                .moves
                .push((Source::Tenancy, tenant, last.to_string()));
            if batch.signals.len() >= BATCH {
                self.flush(batch, sent).await?;
            }
        }
        Ok(())
    }

    async fn audit_chain(&self, batch: &mut Batch, sent: &mut usize) -> BridgeResult<()> {
        loop {
            let after = self.position(Source::Audit, "").await?.max(0) as u64;
            let room = (BATCH - batch.signals.len()).max(1);
            let audit = self.audit.clone();
            let rows = tokio::task::spawn_blocking(move || audit.after(after, room))
                .await
                .map_err(|join| BridgeError::new("audit_read", join.to_string()))??;
            let Some(last) = rows.last().map(|row| row.0) else {
                return Ok(());
            };
            let full = rows.len() == room;
            for (seq, at, subject, via, tool, broker, operation, outcome, hash) in rows {
                let mut signal = Signal::new(
                    format!("audit:{seq}:{}", hash16(&hash)),
                    Source::Audit,
                    format!("tool.{tool}"),
                    rfc3339_ms(&at).unwrap_or(0),
                );
                signal.actor = Some(subject);
                signal.target = Some(broker).filter(|b| !b.is_empty());
                signal.operation = Some(operation).filter(|o| !o.is_empty());
                signal.outcome = Some(outcome);
                signal.data = Some(json!({ "via": via }));
                batch.signals.push(self.keep(signal));
            }
            batch
                .moves
                .push((Source::Audit, String::new(), last.to_string()));
            // The audit position moves only when this batch is sent; send it before reading on.
            self.flush(batch, sent).await?;
            if !full {
                return Ok(());
            }
        }
    }

    async fn push(&self, signals: &[Signal]) -> BridgeResult<()> {
        let mut body = String::new();
        for signal in signals {
            body.push_str(&serde_json::to_string(signal).expect("signal json"));
            body.push('\n');
        }
        let mut request = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/x-ndjson")
            .header("devagotchi-signal-schema", SCHEMA)
            .header("devagotchi-sink", &self.name);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(secret) = &self.secret {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
            mac.update(body.as_bytes());
            request = request.header(
                "devagotchi-signature",
                format!("sha256={}", hex::encode(mac.finalize().into_bytes())),
            );
        }
        let refused = |why: String| {
            BridgeError::new(
                "sink_unavailable",
                format!("sink {} at {}: {why}", self.name, self.url),
            )
        };
        let response = request
            .body(body)
            .send()
            .await
            .map_err(|error| refused(error.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(refused(format!("answered {}", response.status().as_u16())));
        }
        Ok(())
    }

    /// What `/healthz` reports for this sink. Never the token, the secret or a signal.
    pub fn health(&self) -> Value {
        let status = self.status.lock().expect("sink status");
        let now = crate::board::system_clock()();
        let oldest = status.oldest_waiting_ms.map(|ms| (now - ms).max(0) / 1000);
        json!({
            "name": self.name,
            "ready": status.ready,
            "degraded": !status.ready || status.failures > 0,
            "sent": status.sent,
            "waiting": status.waiting,
            "oldest_waiting_secs": oldest,
            "last_sent_at": status.last_sent_at,
            "last_error": status.last_error,
            "fields": match self.fields {
                SinkFields::Metadata => "metadata",
                SinkFields::Full => "full",
            },
        })
    }
}
