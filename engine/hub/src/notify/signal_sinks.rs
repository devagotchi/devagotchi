//! Signal sinks set on `/admin` (hub-settings M5): kept in `signal_sinks` (migration 0036) with
//! the token and secret sealed, audited on the hub's home chain as `signals.sink_saved` and
//! `signals.sink_removed` (name, address, fields, sources, on or off; never a secret).

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use crate::config::{SinkConfig, SinkFields};
use crate::identity::accounts::{Accounts, tenants};
use crate::machines::now_secs;
use crate::store::{secs, transaction};

/// The names the token and the secret are sealed under, with the sink's name.
pub fn sealed_as(sink: &str, what: &str) -> String {
    format!("signals.{sink}.{what}")
}

/// A sink as `/admin` keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSink {
    pub name: String,
    pub url: String,
    /// Empty: the sink takes no bearer token.
    pub sealed_token: String,
    /// Empty: batches are not signed.
    pub sealed_secret: String,
    /// `metadata` or `full`.
    pub fields: String,
    /// `board`, `tenancy`, `audit`; empty: all of them.
    pub sources: Vec<String>,
    pub enabled: bool,
}

impl Accounts {
    /// Every sink set on `/admin`, by name.
    pub async fn stored_sinks(&self) -> BridgeResult<Vec<StoredSink>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query(
                "SELECT name, url, sealed_token, sealed_secret, fields, sources, enabled
                 FROM signal_sinks ORDER BY name",
            )
            .fetch_all(&mut *connection)
            .await?;
            let mut sinks = Vec::new();
            for row in rows {
                let sources: String = row.try_get("sources")?;
                sinks.push(StoredSink {
                    name: row.try_get("name")?,
                    url: row.try_get("url")?,
                    sealed_token: row.try_get("sealed_token")?,
                    sealed_secret: row.try_get("sealed_secret")?,
                    fields: row.try_get("fields")?,
                    sources: sources
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect(),
                    enabled: row.try_get::<i32, _>("enabled")? != 0,
                });
            }
            Ok(sinks)
        })
    }

    /// Save a sink (made or changed), audited without its secrets.
    pub async fn save_sink(&self, by: &str, sink: &StoredSink) -> BridgeResult<()> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        transaction!(&self.store, |connection| {
            sqlx::query(
                "INSERT INTO signal_sinks
                   (name, url, sealed_token, sealed_secret, fields, sources, enabled, updated_at,
                    updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (name) DO UPDATE SET url = excluded.url,
                   sealed_token = excluded.sealed_token, sealed_secret = excluded.sealed_secret,
                   fields = excluded.fields, sources = excluded.sources,
                   enabled = excluded.enabled, updated_at = excluded.updated_at,
                   updated_by = excluded.updated_by",
            )
            .bind(&sink.name)
            .bind(&sink.url)
            .bind(&sink.sealed_token)
            .bind(&sink.sealed_secret)
            .bind(&sink.fields)
            .bind(sink.sources.join(","))
            .bind(i32::from(sink.enabled))
            .bind(secs(now))
            .bind(by)
            .execute(&mut *connection)
            .await?;
            self.store.scope(connection, &home).await?;
            let sources = if sink.sources.is_empty() {
                "all".to_string()
            } else {
                sink.sources.join(",")
            };
            tenants::record_in(
                connection,
                &home,
                by,
                "signals.sink_saved",
                &format!(
                    "{} {} {} {}: {}",
                    sink.name,
                    sink.url,
                    sink.fields,
                    sources,
                    if sink.enabled { "on" } else { "off" }
                ),
                now,
            )
            .await?;
            Ok(())
        })
    }

    /// Remove a sink set on `/admin`; whether there was one. Its position stays in
    /// `signal_cursors`, so a sink saved again under the name carries on where it was.
    pub async fn remove_sink(&self, by: &str, name: &str) -> BridgeResult<bool> {
        let now = now_secs();
        let home = self.home_tenant().await?;
        transaction!(&self.store, |connection| {
            let gone = sqlx::query("DELETE FROM signal_sinks WHERE name = $1")
                .bind(name)
                .execute(&mut *connection)
                .await?
                .rows_affected()
                > 0;
            if gone {
                self.store.scope(connection, &home).await?;
                tenants::record_in(connection, &home, by, "signals.sink_removed", name, now)
                    .await?;
            }
            Ok(gone)
        })
    }
}

/// The running sink for one saved on `/admin`, its token and secret opened with `sealer`.
pub fn sink_of(
    stored: &StoredSink,
    sealer: &crate::identity::Sealer,
    store: crate::store::Store,
    audit: std::sync::Arc<crate::ops::audit::Audit>,
) -> BridgeResult<super::signals::Sink> {
    let open = |sealed: &str, what: &str| {
        (!sealed.is_empty())
            .then(|| sealer.open_value(&sealed_as(&stored.name, what), sealed))
            .flatten()
    };
    let (token, secret) = (
        open(&stored.sealed_token, "token"),
        open(&stored.sealed_secret, "secret"),
    );
    let config = SinkConfig {
        name: stored.name.clone(),
        url: stored.url.clone(),
        token_env: token.as_ref().map(|_| "TOKEN".to_string()),
        secret_env: secret.as_ref().map(|_| "SECRET".to_string()),
        fields: if stored.fields == "full" {
            SinkFields::Full
        } else {
            SinkFields::Metadata
        },
        sources: stored.sources.clone(),
    };
    super::signals::Sink::new(&config, store, audit, |name| match name {
        "TOKEN" => token.clone(),
        "SECRET" => secret.clone(),
        _ => None,
    })
}
