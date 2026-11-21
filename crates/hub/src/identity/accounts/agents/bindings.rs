use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct OAuthBinding {
    pub client: String,
    pub agent_id: String,
    pub enabled: bool,
    pub version: i64,
}

impl Accounts {
    /// Stable client ids with live credentials for this account; names are display only.
    pub async fn agent_client_options(
        &self,
        actor: &str,
        account: &str,
    ) -> BridgeResult<Vec<(String, String)>> {
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            let now = database_now(connection).await?;
            let clients = sqlx::query_as("SELECT c.id,c.name FROM oauth_clients c WHERE EXISTS (SELECT 1 FROM oauth_tokens t WHERE t.client=c.id AND t.tenant_id=$1 AND t.account=$2 AND t.expires_at>$3) OR EXISTS (SELECT 1 FROM auth_agent_oauth_bindings b WHERE b.client=c.id AND b.tenant_id=$1 AND b.account=$2) ORDER BY c.id LIMIT 1001")
                .bind(&manager.tenant).bind(account).bind(secs(now)).fetch_all(&mut *connection).await?;
            if clients.len() > 1000 {
                return Err(BridgeError::new("limit", "too many OAuth clients").into());
            }
            Ok(clients)
        })
    }
    /// One account's stable OAuth client id, covering every access/refresh family for it.
    /// A disabled row stays present so revocation cannot fall back to an unbound identity.
    pub async fn bind_agent_oauth(
        &self,
        actor: &str,
        account: &str,
        binding: &OAuthBinding,
    ) -> BridgeResult<OAuthBinding> {
        if binding.version < 0 {
            return Err(BridgeError::new("invalid", "invalid binding version"));
        }
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            catalog::lock_in(connection, &manager.tenant).await?;
            if binding.enabled {
                live_in(connection, &manager.tenant, account, &binding.agent_id).await?;
            } else {
                agent_in(connection, &manager.tenant, account, &binding.agent_id).await?;
            }
            let known: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM oauth_tokens WHERE tenant_id=$1 AND account=$2 AND client=$3",
            )
            .bind(&manager.tenant)
            .bind(account)
            .bind(&binding.client)
            .fetch_one(&mut *connection)
            .await?;
            if known == 0 {
                return Err(BridgeError::new(
                    "not_found",
                    "OAuth client has no connection to this account",
                )
                .into());
            }
            let version = binding
                .version
                .checked_add(1)
                .ok_or_else(|| BridgeError::new("version_conflict", "binding version exhausted"))?;
            let changed = if binding.version == 0 {
                sqlx::query("INSERT INTO auth_agent_oauth_bindings (tenant_id,account,client,agent_id,enabled,version) VALUES ($1,$2,$3,$4,$5,1) ON CONFLICT (tenant_id,account,client) DO NOTHING")
                    .bind(&manager.tenant).bind(account).bind(&binding.client).bind(&binding.agent_id).bind(i64::from(binding.enabled)).execute(&mut *connection).await?.rows_affected()
            } else {
                sqlx::query("UPDATE auth_agent_oauth_bindings SET agent_id=$4,enabled=$5,version=$6 WHERE tenant_id=$1 AND account=$2 AND client=$3 AND version=$7")
                    .bind(&manager.tenant).bind(account).bind(&binding.client).bind(&binding.agent_id).bind(i64::from(binding.enabled)).bind(version).bind(binding.version).execute(&mut *connection).await?.rows_affected()
            };
            if changed != 1 {
                return Err(BridgeError::new(
                    "version_conflict",
                    "binding changed; reload before saving",
                )
                .into());
            }
            let saved = OAuthBinding {
                version,
                ..binding.clone()
            };
            let now = database_now(connection).await?;
            tenants::record_in(
                connection,
                &manager.tenant,
                actor,
                "agent.oauth.bound",
                &serde_json::to_string(&saved).expect("binding JSON"),
                now,
            )
            .await?;
            Ok(saved)
        })
    }

    pub async fn agent_oauth_bindings(
        &self,
        actor: &str,
        account: &str,
    ) -> BridgeResult<Vec<OAuthBinding>> {
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            let rows: Vec<(String,String,i64,i64)> = sqlx::query_as("SELECT client,agent_id,enabled,version FROM auth_agent_oauth_bindings WHERE tenant_id=$1 AND account=$2 ORDER BY client LIMIT 1001")
                .bind(&manager.tenant).bind(account).fetch_all(&mut *connection).await?;
            if rows.len() > 1000 {
                return Err(BridgeError::new("limit", "too many OAuth bindings").into());
            }
            Ok(rows
                .into_iter()
                .map(|(client, agent_id, enabled, version)| OAuthBinding {
                    client,
                    agent_id,
                    enabled: enabled == 1,
                    version,
                })
                .collect())
        })
    }
}

pub(in crate::identity::accounts) async fn oauth_agent_in(
    connection: &mut AnyConnection,
    tenant: &str,
    account: &str,
    client: &str,
) -> Result<Option<Agent>, Failed> {
    let binding: Option<(String,i64)> = sqlx::query_as("SELECT agent_id,enabled FROM auth_agent_oauth_bindings WHERE tenant_id=$1 AND account=$2 AND client=$3")
        .bind(tenant).bind(account).bind(client).fetch_optional(&mut *connection).await?;
    let Some((id, enabled)) = binding else {
        return Ok(None);
    };
    if enabled != 1 {
        return Err(
            BridgeError::new("agent_disabled", "registered agent connection is disabled").into(),
        );
    }
    Ok(Some(live_in(connection, tenant, account, &id).await?))
}
