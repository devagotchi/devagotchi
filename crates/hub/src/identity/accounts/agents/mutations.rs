use super::*;

impl Accounts {
    pub async fn put_agent(
        &self,
        actor: &str,
        account: &str,
        edit: &AgentEdit,
    ) -> BridgeResult<Agent> {
        edit.check()?;
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            catalog::initialize_in(connection, &manager.tenant).await?;
            if manager.owner {
                let roles = ADMIN_ROLES
                    .iter()
                    .map(|role| role.to_string())
                    .collect::<Vec<_>>();
                let subject = PolicySubject {
                    id: actor,
                    kind: "human",
                    human: true,
                    roles: &roles,
                };
                catalog::authorize_in(
                    connection,
                    &manager.tenant,
                    &subject,
                    if edit.version == 0 {
                        "admin.principals.create"
                    } else {
                        "admin.principals.update"
                    },
                    &devagotchi_agenthub::policy::Resource::default(),
                )
                .await?;
            }
            catalog::check_profile_in(connection, &manager.tenant, &edit.kind, &edit.roles).await?;
            let current = if edit.version == 0 {
                None
            } else {
                Some(agent_in(connection, &manager.tenant, account, &edit.id).await?)
            };
            if let Some(current) = &current {
                if current.version != edit.version {
                    return Err(BridgeError::new(
                        "version_conflict",
                        "agent changed; reload before saving",
                    )
                    .into());
                }
            }
            // A person may rename their assigned agent. Only the environment owner changes
            // authority: kind can be a policy selector just as roles and enabled state can.
            if !manager.owner
                && current.as_ref().is_none_or(|current| {
                    current.roles != edit.roles
                        || current.kind != edit.kind
                        || current.enabled != edit.enabled
                        || current.wip_limit != edit.wip_limit
                })
            {
                return Err(BridgeError::new(
                    "not_granted",
                    "only the environment owner changes agent authority",
                )
                .into());
            }
            let now = database_now(connection).await?;
            let roles = serde_json::to_string(&edit.roles)
                .map_err(|_| BridgeError::new("invalid", "invalid roles"))?;
            let version = edit
                .version
                .checked_add(1)
                .ok_or_else(|| BridgeError::new("version_conflict", "agent version exhausted"))?;
            let changed = if edit.version == 0 {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM auth_agents WHERE tenant_id=$1 AND account=$2",
                )
                .bind(&manager.tenant)
                .bind(account)
                .fetch_one(&mut *connection)
                .await?;
                if count >= 1000 {
                    return Err(BridgeError::new("limit", "account agent limit reached").into());
                }
                sqlx::query("INSERT INTO auth_agents (tenant_id,account,id,name,kind,roles,enabled,wip_limit,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$9) ON CONFLICT (tenant_id,account,id) DO NOTHING")
                    .bind(&manager.tenant).bind(account).bind(&edit.id).bind(edit.name.trim()).bind(&edit.kind).bind(&roles)
                    .bind(i64::from(edit.enabled)).bind(edit.wip_limit).bind(secs(now)).execute(&mut *connection).await?.rows_affected()
            } else {
                sqlx::query("UPDATE auth_agents SET name=$4,kind=$5,roles=$6,enabled=$7,wip_limit=$8,version=$9,updated_at=$10 WHERE tenant_id=$1 AND account=$2 AND id=$3 AND version=$11")
                    .bind(&manager.tenant).bind(account).bind(&edit.id).bind(edit.name.trim()).bind(&edit.kind).bind(&roles)
                    .bind(i64::from(edit.enabled)).bind(edit.wip_limit).bind(version).bind(secs(now)).bind(edit.version).execute(&mut *connection).await?.rows_affected()
            };
            if changed != 1 {
                return Err(BridgeError::new(
                    "version_conflict",
                    "agent changed; reload before saving",
                )
                .into());
            }
            let saved = agent_in(connection, &manager.tenant, account, &edit.id).await?;
            tenants::record_in(
                connection,
                &manager.tenant,
                actor,
                "agent.saved",
                &serde_json::to_string(&saved).expect("agent JSON"),
                now,
            )
            .await?;
            Ok(saved)
        })
    }
}
