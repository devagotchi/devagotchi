//! Registered nonhuman profiles and their credential bindings.
//!
//! Owns: auth_agents and auth_agent_oauth_bindings; optional auth_api_keys.agent_id.
//! Serves: account-scoped profile/catalog APIs and AgentHub transaction recipient checks.
//! Settings: kinds, roles, policy and WIP on /app/agents/configuration; profiles on /app/agents.
//! Depends on: identity accounts, tenant audit, AgentHub policy catalog and PostgreSQL.

use super::*;
use devagotchi_agenthub::policy::{Subject as PolicySubject, catalog};
use serde::{Deserialize, Serialize};

mod bindings;
mod configuration;
mod directory;
pub(crate) use directory::Directory;
mod mutations;
mod roster;
pub use bindings::OAuthBinding;
pub(super) use bindings::oauth_agent_in;
pub use configuration::PrincipalConfiguration;
#[cfg(test)]
mod concurrency_tests;
#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub account: String,
    pub name: String,
    pub kind: String,
    pub roles: Vec<String>,
    pub enabled: bool,
    pub wip_limit: Option<i64>,
    pub version: i64,
    pub last_seen: Option<u64>,
}

impl Agent {
    pub fn principal(&self) -> String {
        format!("@agent:{}:{}", self.account, self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEdit {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub roles: Vec<String>,
    pub enabled: bool,
    pub wip_limit: Option<i64>,
    /// Zero creates; an existing agent requires its current version.
    pub version: i64,
}

impl AgentEdit {
    fn check(&self) -> BridgeResult<()> {
        if !devagotchi_agenthub::valid_name(&self.id)
            || self.name.trim().is_empty()
            || self.name.chars().count() > 100
            || self.name.chars().any(char::is_control)
            || self.version < 0
            || self.wip_limit.is_some_and(|limit| limit < 0)
        {
            return Err(BridgeError::new(
                "invalid",
                "invalid agent identity, name, WIP or version",
            ));
        }
        Ok(())
    }
}

const COLUMNS: &str = "id,account,name,kind,roles,enabled,wip_limit,version,last_seen";
const ADMIN_ROLES: &[&str] = &["admin"];

fn stored(row: &AnyRow) -> Result<Agent, Failed> {
    let raw: String = row.try_get("roles")?;
    let roles: Vec<String> = serde_json::from_str(&raw)
        .map_err(|_| BridgeError::new("account_store", "invalid stored agent roles"))?;
    if roles.len() > 128
        || roles
            .iter()
            .enumerate()
            .any(|(i, role)| !devagotchi_agenthub::valid_name(role) || roles[..i].contains(role))
    {
        return Err(BridgeError::new("account_store", "invalid stored agent roles").into());
    }
    let agent = Agent {
        id: row.try_get("id")?,
        account: row.try_get("account")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        roles,
        enabled: row.try_get::<i64, _>("enabled")? == 1,
        wip_limit: row.try_get("wip_limit")?,
        version: row.try_get("version")?,
        last_seen: row.try_get::<Option<i64>, _>("last_seen")?.map(unsecs),
    };
    if !devagotchi_agenthub::valid_name(&agent.id)
        || !valid_account_id(&agent.account)
        || !devagotchi_agenthub::valid_name(&agent.kind)
        || agent.version < 1
        || agent.wip_limit.is_some_and(|limit| limit < 0)
    {
        return Err(BridgeError::new("account_store", "invalid stored agent profile").into());
    }
    Ok(agent)
}

pub(super) async fn agent_in(
    connection: &mut AnyConnection,
    tenant: &str,
    account: &str,
    id: &str,
) -> Result<Agent, Failed> {
    let sql =
        format!("SELECT {COLUMNS} FROM auth_agents WHERE tenant_id=$1 AND account=$2 AND id=$3");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(tenant)
        .bind(account)
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(|| BridgeError::new("not_found", "agent not found in this account"))?;
    stored(&row)
}

async fn database_now(connection: &mut AnyConnection) -> Result<u64, Failed> {
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()))::bigint")
            .fetch_one(connection)
            .await?;
    Ok(unsecs(now))
}

impl Accounts {
    /// Called only for authenticated people by browser administration. A live owner may
    /// manage accounts in this environment; another person manages only their own account.
    pub(super) async fn agent_manager_in(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
        target: &str,
    ) -> Result<Account, Failed> {
        if self
            .enter(connection, Lookup::Account(actor))
            .await?
            .is_none()
        {
            return Err(BridgeError::new("not_granted", "account unavailable").into());
        }
        let manager = account_in(connection, actor)
            .await?
            .filter(|account| account.disabled_at.is_none())
            .ok_or_else(|| BridgeError::new("not_granted", "account unavailable"))?;
        if actor != target && !manager.owner {
            return Err(BridgeError::new("not_granted", "manage only your own agents").into());
        }
        let target_account = account_in(connection, target)
            .await?
            .filter(|account| account.tenant == manager.tenant && account.disabled_at.is_none())
            .ok_or_else(|| {
                BridgeError::new("not_granted", "account unavailable in this environment")
            })?;
        if self.lapsed(&target_account, now_secs()) || self.lapsed(&manager, now_secs()) {
            return Err(BridgeError::new("not_granted", "account unavailable").into());
        }
        Ok(manager)
    }

    pub async fn agents(
        &self,
        actor: &str,
        account: &str,
        after: Option<&str>,
    ) -> BridgeResult<Vec<Agent>> {
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            let sql = format!(
                "SELECT {COLUMNS} FROM auth_agents WHERE tenant_id=$1 AND account=$2 AND ($3::text IS NULL OR id>$3) ORDER BY id LIMIT 101"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&manager.tenant)
                .bind(account)
                .bind(after)
                .fetch_all(&mut *connection)
                .await?;
            rows.iter().map(stored).collect()
        })
    }

    pub async fn agent(&self, actor: &str, account: &str, id: &str) -> BridgeResult<Agent> {
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            agent_in(connection, &manager.tenant, account, id).await
        })
    }

    /// Resolve only an authenticated canonical principal, never a caller-selected seat.
    pub async fn live_agent(
        &self,
        tenant: &str,
        account: &str,
        principal: &str,
    ) -> BridgeResult<(Agent, i64)> {
        let prefix = format!("@agent:{account}:");
        let id = principal
            .strip_prefix(&prefix)
            .filter(|id| devagotchi_agenthub::valid_name(id))
            .ok_or_else(|| BridgeError::new("not_granted", "invalid registered agent identity"))?;
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let agent = live_in(connection, tenant, account, id).await?;
            let limit = agent.wip_limit.unwrap_or(
                catalog::settings_in(connection, tenant)
                    .await?
                    .wip_limit_default,
            );
            Ok((agent, limit))
        })
    }
}

pub(super) async fn live_in(
    connection: &mut AnyConnection,
    tenant: &str,
    account: &str,
    id: &str,
) -> Result<Agent, Failed> {
    let agent = agent_in(connection, tenant, account, id).await?;
    if !agent.enabled {
        return Err(BridgeError::new("agent_disabled", "registered agent is disabled").into());
    }
    catalog::check_profile_in(connection, tenant, &agent.kind, &agent.roles).await?;
    let now = database_now(connection).await?;
    sqlx::query("UPDATE auth_agents SET last_seen=$4 WHERE tenant_id=$1 AND account=$2 AND id=$3 AND (last_seen IS NULL OR last_seen <= $4-15)")
        .bind(tenant).bind(account).bind(id).bind(secs(now)).execute(connection).await?;
    Ok(agent)
}
