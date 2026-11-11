//! API keys: OAuth is the handshake a person does once; a key is what they hand an agent, a
//! script or a client that cannot do that handshake. Each key belongs to an account, is limited
//! to the scopes, projects and machines the person chose, and ends at a set time. The hub keeps
//! a SHA-256 of it and shows the key once.
//!
//! A key acts for its account's AI, never as the person: it cannot approve anything, and it
//! reaches only the MCP endpoint. A person makes and ends keys from their own signed-in page.
//! `KeyLimits::check` runs in `Hub::call_tool`, the one door every tool passes through, and a
//! tool that has no scope here is refused, so a new tool is closed until it is given one.

use base64::Engine as _;
use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{AnyConnection, Row};

use super::{Accounts, account_columns_a, account_row, hash, now_secs, secs, tenants};
use crate::{
    auth::Subject,
    config::valid_name,
    store::{Backend, Failed, transaction},
};

#[cfg(test)]
mod tests;

const PREFIX: &str = "dgk_";
/// The keys one account may hold at once.
const MAX_LIVE: i64 = 25;
const MAX_NAME: usize = 60;
const MAX_NAMED: usize = 100;
/// How long a key may be made for, in hours.
pub const DURATIONS: [(u64, &str); 6] = [
    (1, "1 hour"),
    (24, "1 day"),
    (168, "7 days"),
    (720, "30 days"),
    (2160, "90 days"),
    (8760, "1 year"),
];

/// What a key may do. Each tool needs exactly one of these (`scope_for`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "machines.read")]
    MachinesRead,
    #[serde(rename = "machines.run")]
    MachinesRun,
    #[serde(rename = "machines.manage")]
    MachinesManage,
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "files.write")]
    FilesWrite,
    #[serde(rename = "boards.read")]
    BoardsRead,
    #[serde(rename = "boards.write")]
    BoardsWrite,
    #[serde(rename = "boards.admin")]
    BoardsAdmin,
    #[serde(rename = "approvals.request")]
    ApprovalsRequest,
}

impl Scope {
    pub const ALL: [Scope; 9] = [
        Scope::MachinesRead,
        Scope::MachinesRun,
        Scope::MachinesManage,
        Scope::FilesRead,
        Scope::FilesWrite,
        Scope::BoardsRead,
        Scope::BoardsWrite,
        Scope::BoardsAdmin,
        Scope::ApprovalsRequest,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::MachinesRead => "machines.read",
            Scope::MachinesRun => "machines.run",
            Scope::MachinesManage => "machines.manage",
            Scope::FilesRead => "files.read",
            Scope::FilesWrite => "files.write",
            Scope::BoardsRead => "boards.read",
            Scope::BoardsWrite => "boards.write",
            Scope::BoardsAdmin => "boards.admin",
            Scope::ApprovalsRequest => "approvals.request",
        }
    }

    /// What it allows, as a person reads it.
    pub fn title(self) -> &'static str {
        match self {
            Scope::MachinesRead => "See machines and running jobs",
            Scope::MachinesRun => "Run commands on machines",
            Scope::MachinesManage => "Add and update machines, start setup links",
            Scope::FilesRead => "Read files on machines",
            Scope::FilesWrite => "Write and move files on machines",
            Scope::BoardsRead => "Read boards and notes",
            Scope::BoardsWrite => "Work tickets and leave notes",
            Scope::BoardsAdmin => "Change board settings and webhooks",
            Scope::ApprovalsRequest => "Ask you for approvals",
        }
    }

    pub fn parse(text: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.as_str() == text)
    }
}

/// The scope a tool needs. `None` means no key may call it.
pub fn scope_for(tool: &str) -> Option<Scope> {
    Some(match tool {
        "machines" | "operation_get" | "operation_read" | "operation_wait" => Scope::MachinesRead,
        "run" | "operation_start" | "operation_cancel" => Scope::MachinesRun,
        "machine_add" | "machine_update" | "workflow_list" | "workflow_start"
        | "workflow_status" | "workflow_wait" | "workflow_cancel" => Scope::MachinesManage,
        "read_file" | "read_multiple_files" | "read_image" | "list_directory" | "get_file_info" => {
            Scope::FilesRead
        }
        "write_file" | "edit_file" | "create_directory" | "move_file" => Scope::FilesWrite,
        "approval_request" | "approval_wait" => Scope::ApprovalsRequest,
        "board_settings" | "board_wake" => Scope::BoardsAdmin,
        "board_attach"
        | "board_issue_context"
        | "board_prime"
        | "board_ready"
        | "board_list"
        | "board_get"
        | "board_events"
        | "board_wait"
        | "board_inbox"
        | "board_verify" => Scope::BoardsRead,
        tool if tool.starts_with("board_") => Scope::BoardsWrite,
        _ => return None,
    })
}

/// A tool every key may call: it says who the key is and touches nothing.
pub const ALWAYS: [&str; 1] = ["whoami"];

/// What a key was limited to, carried on its request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyLimits {
    pub id: String,
    pub scopes: Vec<Scope>,
    /// Empty: every project the account can reach.
    pub projects: Vec<String>,
    /// Empty: every machine the account can reach.
    pub machines: Vec<String>,
}

impl KeyLimits {
    /// Why this key may not make this call, or nothing when it may.
    pub fn check(
        &self,
        tool: &str,
        arguments: Option<&serde_json::Map<String, Value>>,
    ) -> Option<String> {
        if ALWAYS.contains(&tool) {
            return None;
        }
        let Some(scope) = scope_for(tool) else {
            return Some(format!("an API key cannot call {tool}"));
        };
        if !self.scopes.contains(&scope) {
            return Some(format!("this API key does not have {}", scope.as_str()));
        }
        let named = |key: &str| arguments.and_then(|a| a.get(key)).and_then(Value::as_str);
        if !self.projects.is_empty()
            && let Some(project) = named("project")
            && !self.projects.iter().any(|allowed| allowed == project)
        {
            return Some(format!(
                "this API key is not allowed on project {project:?}"
            ));
        }
        if !self.machines.is_empty() {
            for key in ["machine", "broker"] {
                if let Some(machine) = named(key)
                    && !self.machines.iter().any(|allowed| allowed == machine)
                {
                    return Some(format!(
                        "this API key is not allowed on machine {machine:?}"
                    ));
                }
            }
        }
        None
    }
}

/// A key as its owner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    /// The key's last four characters.
    pub hint: String,
    pub scopes: Vec<Scope>,
    pub projects: Vec<String>,
    pub machines: Vec<String>,
    /// Unix seconds.
    pub created_at: u64,
    pub expires_at: u64,
    pub last_used_at: Option<u64>,
    pub revoked_at: Option<u64>,
}

impl ApiKey {
    pub fn live(&self, now: u64) -> bool {
        self.revoked_at.is_none() && now < self.expires_at
    }
}

/// What a person asks for.
#[derive(Debug, Clone, Default)]
pub struct NewKey {
    pub name: String,
    pub scopes: Vec<Scope>,
    pub projects: Vec<String>,
    pub machines: Vec<String>,
    pub hours: u64,
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}

impl NewKey {
    fn check(&self) -> BridgeResult<()> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME || name.chars().any(char::is_control)
        {
            return Err(invalid(format!(
                "a key's name is 1 to {MAX_NAME} characters"
            )));
        }
        if self.scopes.is_empty() {
            return Err(invalid("choose at least one thing this key may do"));
        }
        if !DURATIONS.iter().any(|(hours, _)| *hours == self.hours) {
            return Err(invalid("that is not one of the durations"));
        }
        if self.projects.len() > MAX_NAMED || self.machines.len() > MAX_NAMED {
            return Err(invalid(format!(
                "name at most {MAX_NAMED} projects or machines"
            )));
        }
        for name in self.projects.iter().chain(&self.machines) {
            if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
                return Err(invalid(format!("{name:?} is not a name")));
            }
        }
        Ok(())
    }
}

fn fresh_secret() -> String {
    format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(crate::link::random::<32>())
    )
}

fn hint_of(secret: &str) -> String {
    secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn list(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn scopes_of(raw: &str) -> Vec<Scope> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn key_from_row(row: &sqlx::any::AnyRow) -> Result<ApiKey, sqlx::Error> {
    let at = |name: &str| -> Result<Option<u64>, sqlx::Error> {
        Ok(row
            .try_get::<Option<i64>, _>(name)?
            .map(|value| value.max(0) as u64))
    };
    Ok(ApiKey {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        hint: row.try_get("hint")?,
        scopes: scopes_of(&row.try_get::<String, _>("scopes")?),
        projects: list(&row.try_get::<String, _>("projects")?),
        machines: list(&row.try_get::<String, _>("machines")?),
        created_at: at("created_at")?.unwrap_or(0),
        expires_at: at("expires_at")?.unwrap_or(0),
        last_used_at: at("last_used_at")?,
        revoked_at: at("revoked_at")?,
    })
}

macro_rules! key_columns {
    () => {
        "id, name, hint, scopes, projects, machines, created_at, expires_at, last_used_at, revoked_at"
    };
}

impl Accounts {
    async fn insert_key(
        &self,
        connection: &mut AnyConnection,
        tenant: &str,
        account: &str,
        new: &NewKey,
        secret: &str,
        now: u64,
    ) -> Result<ApiKey, Failed> {
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_keys WHERE tenant_id = $1 AND account = $2
             AND revoked_at IS NULL AND expires_at > $3",
        )
        .bind(tenant)
        .bind(account)
        .bind(secs(now))
        .fetch_one(&mut *connection)
        .await?;
        if live >= MAX_LIVE {
            return Err(Failed::Refused(invalid(format!(
                "an account holds at most {MAX_LIVE} live keys; revoke one first"
            ))));
        }
        let key = ApiKey {
            id: format!("key_{}", hex::encode(crate::link::random::<8>())),
            name: new.name.trim().to_string(),
            hint: hint_of(secret),
            scopes: new.scopes.clone(),
            projects: new.projects.clone(),
            machines: new.machines.clone(),
            created_at: now,
            expires_at: now + new.hours * 3600,
            last_used_at: None,
            revoked_at: None,
        };
        let json = |value: &[String]| serde_json::to_string(value).expect("names json");
        sqlx::query(
            "INSERT INTO api_keys (tenant_id, id, account, name, hash, hint, scopes, projects,
               machines, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(tenant)
        .bind(&key.id)
        .bind(account)
        .bind(&key.name)
        .bind(hash(secret))
        .bind(&key.hint)
        .bind(serde_json::to_string(&key.scopes).expect("scopes json"))
        .bind(json(&key.projects))
        .bind(json(&key.machines))
        .bind(secs(now))
        .bind(secs(key.expires_at))
        .execute(&mut *connection)
        .await?;
        let detail = format!(
            "{} ({}): {}; ends {}",
            key.id,
            key.name,
            key.scopes
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            key.expires_at
        );
        tenants::record_in(connection, tenant, account, "api_key.created", &detail, now).await?;
        Ok(key)
    }

    /// A new key for `account`. The key itself is only in this answer.
    pub async fn create_key(
        &self,
        tenant: &str,
        account: &str,
        new: &NewKey,
    ) -> BridgeResult<(ApiKey, String)> {
        new.check()?;
        let now = now_secs();
        let secret = fresh_secret();
        let key = transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            self.insert_key(connection, tenant, account, new, &secret, now)
                .await
        })?;
        Ok((key, secret))
    }

    /// `account`'s keys, newest first.
    pub async fn keys(&self, tenant: &str, account: &str) -> BridgeResult<Vec<ApiKey>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let rows = sqlx::query(concat!(
                "SELECT ",
                key_columns!(),
                " FROM api_keys WHERE tenant_id = $1 AND account = $2 ORDER BY created_at DESC, id"
            ))
            .bind(tenant)
            .bind(account)
            .fetch_all(&mut *connection)
            .await?;
            let mut keys = Vec::with_capacity(rows.len());
            for row in &rows {
                keys.push(key_from_row(row)?);
            }
            Ok(keys)
        })
    }

    async fn end_key(
        &self,
        connection: &mut AnyConnection,
        tenant: &str,
        account: &str,
        id: &str,
        now: u64,
    ) -> Result<(), Failed> {
        let ended = sqlx::query(
            "UPDATE api_keys SET revoked_at = $1
             WHERE tenant_id = $2 AND account = $3 AND id = $4 AND revoked_at IS NULL",
        )
        .bind(secs(now))
        .bind(tenant)
        .bind(account)
        .bind(id)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if ended == 0 {
            return Err(Failed::Refused(BridgeError::new(
                "not_found",
                "no such live key on this account",
            )));
        }
        tenants::record_in(connection, tenant, account, "api_key.revoked", id, now).await?;
        Ok(())
    }

    /// End a key at once.
    pub async fn revoke_key(&self, tenant: &str, account: &str, id: &str) -> BridgeResult<()> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            self.end_key(connection, tenant, account, id, now).await
        })
    }

    /// End a key and make another with the same limits and the same length as it was given.
    pub async fn rotate_key(
        &self,
        tenant: &str,
        account: &str,
        id: &str,
    ) -> BridgeResult<(ApiKey, String)> {
        let now = now_secs();
        let secret = fresh_secret();
        let key = transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let row = sqlx::query(concat!(
                "SELECT ",
                key_columns!(),
                " FROM api_keys WHERE tenant_id = $1 AND account = $2 AND id = $3"
            ))
            .bind(tenant)
            .bind(account)
            .bind(id)
            .fetch_optional(&mut *connection)
            .await?;
            let Some(row) = row else {
                return Err(Failed::Refused(BridgeError::new(
                    "not_found",
                    "no such key on this account",
                )));
            };
            let old = key_from_row(&row)?;
            self.end_key(connection, tenant, account, id, now).await?;
            let new = NewKey {
                name: old.name,
                scopes: old.scopes,
                projects: old.projects,
                machines: old.machines,
                hours: old.expires_at.saturating_sub(old.created_at) / 3600,
            };
            self.insert_key(connection, tenant, account, &new, &secret, now)
                .await
        })?;
        Ok((key, secret))
    }

    /// Who a key acts for and what it was limited to; nothing when it is unknown, revoked or
    /// past its time, or its account can no longer be used.
    pub async fn verify_key(&self, token: &str) -> BridgeResult<Option<(Subject, KeyLimits)>> {
        if !token.starts_with(PREFIX) || token.len() > 128 {
            return Ok(None);
        }
        let now = now_secs();
        let key_hash = hash(token);
        let found = transaction!(&self.store, |connection| {
            let sql = match self.store.backend() {
                Backend::Postgres => "SELECT devagotchi.api_key_tenant($1)",
                Backend::Sqlite => "SELECT tenant_id FROM api_keys WHERE hash = $1",
            };
            let tenant: Option<Option<String>> = sqlx::query_scalar(sql)
                .bind(&key_hash)
                .fetch_optional(&mut *connection)
                .await?;
            let Some(tenant) = tenant.flatten() else {
                return Ok(None);
            };
            self.store.scope(connection, &tenant).await?;
            let row = sqlx::query(concat!(
                "SELECT ",
                account_columns_a!(),
                ", k.id AS key_id, k.name AS key_name, k.scopes, k.projects, k.machines
                 FROM api_keys k JOIN auth_accounts a ON a.id = k.account
                 WHERE k.hash = $1 AND k.revoked_at IS NULL AND k.expires_at > $2
                   AND a.disabled_at IS NULL"
            ))
            .bind(&key_hash)
            .bind(secs(now))
            .fetch_optional(&mut *connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let mut account = account_row(&row)?;
            let limits = KeyLimits {
                id: row.try_get("key_id")?,
                scopes: scopes_of(&row.try_get::<String, _>("scopes")?),
                projects: list(&row.try_get::<String, _>("projects")?),
                machines: list(&row.try_get::<String, _>("machines")?),
            };
            let name: String = row.try_get("key_name")?;
            if self.lapsed(&account, now) {
                return Ok(None);
            }
            tenants::touch(connection, &mut account, now).await?;
            let admitted = self.settle_in(connection, &mut account, now).await?;
            sqlx::query(
                "UPDATE api_keys SET last_used_at = $1 WHERE hash = $2
                 AND (last_used_at IS NULL OR last_used_at < $3)",
            )
            .bind(secs(now))
            .bind(&key_hash)
            .bind(secs(now.saturating_sub(60)))
            .execute(&mut *connection)
            .await?;
            Ok(Some((account, limits, name, admitted)))
        })?;
        Ok(found.map(|(account, limits, name, admitted)| {
            self.announce(admitted);
            let mut subject = self.subject(&account, "key");
            subject.client = Some(name);
            (subject, limits)
        }))
    }
}

/// The seat a key's calls run as when the request names none: a key is never the person.
pub const KEY_SEAT: &str = "ai";

/// Whether `name` could be an allow-listed project or machine typed by hand.
pub fn plausible_name(name: &str) -> bool {
    valid_name(name) || name.contains('/')
}
