//! `storage` and `clickhouse`: the hub's Postgres (its URL, never with a password, and its
//! schema) and the ClickHouse its board events are exported to, with the environment
//! variables that set or override them at deploy.

use serde::Deserialize;

use super::HubConfig;

pub use devagotchi_agenthub::clickhouse::ClickHouseConfig;

pub const CLICKHOUSE_URL_ENV: &str = "DEVAGOTCHI_CLICKHOUSE_URL";
pub const CLICKHOUSE_DATABASE_ENV: &str = "DEVAGOTCHI_CLICKHOUSE_DATABASE";
pub const CLICKHOUSE_USER_ENV: &str = "DEVAGOTCHI_CLICKHOUSE_USER";
pub const CLICKHOUSE_PASSWORD_ENV: &str = "DEVAGOTCHI_CLICKHOUSE_PASSWORD";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// A Postgres URL without a password.
    #[serde(default)]
    pub url: Option<String>,
    /// The Postgres schema the hub's tables live in, made if missing; `public` when absent.
    /// Give one on any database something else also uses.
    #[serde(default)]
    pub schema: Option<String>,
}

impl StorageConfig {
    /// A URL the store accepts and a schema name it can make.
    pub(super) fn check(&self) -> Result<(), String> {
        if let Some(url) = &self.url {
            crate::store::check_configured_url(url)?;
        }
        if let Some(schema) = &self.schema
            && !crate::store::valid_schema(schema)
        {
            return Err(format!(
                "storage.schema {schema:?}: 1 to 63 of [a-z0-9_], not starting with a digit or pg_"
            ));
        }
        Ok(())
    }
}

/// `clickhouse` with `DEVAGOTCHI_CLICKHOUSE_URL`, `_DATABASE` and `_USER` from `env` over it,
/// checked: the HTTP interface, and a database name the hub can make.
pub(super) fn clickhouse_from_env(
    mut clickhouse: Option<ClickHouseConfig>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<ClickHouseConfig>, String> {
    if let Some(url) = env(CLICKHOUSE_URL_ENV).filter(|url| !url.trim().is_empty()) {
        let url = url.trim().to_string();
        match &mut clickhouse {
            Some(clickhouse) => clickhouse.url = url,
            None => {
                clickhouse = Some(ClickHouseConfig {
                    url,
                    database: devagotchi_agenthub::clickhouse::default_database(),
                    user: None,
                })
            }
        }
    }
    if let Some(clickhouse) = &mut clickhouse {
        if let Some(database) = env(CLICKHOUSE_DATABASE_ENV).filter(|v| !v.trim().is_empty()) {
            clickhouse.database = database.trim().to_string();
        }
        if let Some(user) = env(CLICKHOUSE_USER_ENV).filter(|v| !v.trim().is_empty()) {
            clickhouse.user = Some(user.trim().to_string());
        }
        if !(clickhouse.url.starts_with("http://") || clickhouse.url.starts_with("https://")) {
            return Err(format!(
                "clickhouse.url {:?}: the HTTP interface, http(s)://host:port",
                clickhouse.url
            ));
        }
        if !crate::store::valid_schema(&clickhouse.database) {
            return Err(format!(
                "clickhouse.database {:?}: 1 to 63 of [a-z0-9_], not starting with a digit",
                clickhouse.database
            ));
        }
    }
    Ok(clickhouse)
}

impl HubConfig {
    /// The store the config names, if any (the environment can override it).
    pub fn storage_url(&self) -> Option<&str> {
        self.storage
            .as_ref()
            .and_then(|storage| storage.url.as_deref())
    }

    /// The Postgres schema the config names for the store, if any (the environment can
    /// override it).
    pub fn storage_schema(&self) -> Option<&str> {
        self.storage
            .as_ref()
            .and_then(|storage| storage.schema.as_deref())
    }
}
