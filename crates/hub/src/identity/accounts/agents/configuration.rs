use super::*;
use devagotchi_agenthub::policy::Rule;

#[derive(Debug, Clone, Serialize)]
pub struct PrincipalConfiguration {
    pub settings: catalog::PrincipalSettings,
    pub roles: Vec<catalog::Role>,
    pub rules: Vec<Rule>,
}

impl Accounts {
    async fn policy_owner_in(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
    ) -> Result<Account, Failed> {
        let owner = self.agent_manager_in(connection, actor, actor).await?;
        if !owner.owner {
            return Err(BridgeError::new(
                "not_granted",
                "only the environment owner manages agent policy",
            )
            .into());
        }
        catalog::initialize_in(connection, &owner.tenant).await?;
        Ok(owner)
    }

    pub async fn principal_configuration(
        &self,
        actor: &str,
    ) -> BridgeResult<PrincipalConfiguration> {
        transaction!(&self.store, |connection| {
            let owner = self.policy_owner_in(connection, actor).await?;
            Ok(PrincipalConfiguration {
                settings: catalog::settings_in(connection, &owner.tenant).await?,
                roles: catalog::roles_in(connection, &owner.tenant).await?,
                rules: catalog::rules_in(connection, &owner.tenant).await?,
            })
        })
    }

    pub async fn put_principal_role(
        &self,
        actor: &str,
        role: &catalog::Role,
    ) -> BridgeResult<catalog::Role> {
        let roles = ADMIN_ROLES
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();
        let subject = PolicySubject {
            id: actor,
            kind: "human",
            human: true,
            roles: &roles,
        };
        transaction!(&self.store, |connection| {
            let owner = self.policy_owner_in(connection, actor).await?;
            let saved = catalog::put_role_in(connection, &owner.tenant, &subject, role).await?;
            record_configuration(connection, &owner.tenant, actor, "agent.role.saved", &saved)
                .await?;
            Ok(saved)
        })
    }

    pub async fn delete_principal_role(
        &self,
        actor: &str,
        id: &str,
        version: i64,
    ) -> BridgeResult<()> {
        let roles = ADMIN_ROLES
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();
        let subject = PolicySubject {
            id: actor,
            kind: "human",
            human: true,
            roles: &roles,
        };
        let sought = serde_json::to_string(&[id]).expect("role JSON");
        transaction!(&self.store, |connection| {
            let owner = self.policy_owner_in(connection, actor).await?;
            let used: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM auth_agents WHERE tenant_id=$1 AND roles::jsonb @> $2::jsonb",
            )
            .bind(&owner.tenant)
            .bind(&sought)
            .fetch_one(&mut *connection)
            .await?;
            if used > 0 {
                return Err(BridgeError::new("in_use", "role is assigned to an agent").into());
            }
            catalog::delete_role_in(connection, &owner.tenant, &subject, id, version).await?;
            record_configuration(connection, &owner.tenant, actor, "agent.role.deleted", &id).await
        })
    }

    pub async fn put_principal_rule(&self, actor: &str, rule: &Rule) -> BridgeResult<Rule> {
        let roles = ADMIN_ROLES
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();
        let subject = PolicySubject {
            id: actor,
            kind: "human",
            human: true,
            roles: &roles,
        };
        transaction!(&self.store, |connection| {
            let owner = self.policy_owner_in(connection, actor).await?;
            let saved = catalog::put_rule_in(connection, &owner.tenant, &subject, rule).await?;
            record_configuration(
                connection,
                &owner.tenant,
                actor,
                "agent.policy.saved",
                &saved,
            )
            .await?;
            Ok(saved)
        })
    }

    pub async fn delete_principal_rule(
        &self,
        actor: &str,
        id: &str,
        version: i64,
    ) -> BridgeResult<()> {
        let roles = ADMIN_ROLES
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();
        let subject = PolicySubject {
            id: actor,
            kind: "human",
            human: true,
            roles: &roles,
        };
        transaction!(&self.store, |connection| {
            let owner = self.policy_owner_in(connection, actor).await?;
            catalog::delete_rule_in(connection, &owner.tenant, &subject, id, version).await?;
            record_configuration(
                connection,
                &owner.tenant,
                actor,
                "agent.policy.deleted",
                &id,
            )
            .await
        })
    }

    pub async fn put_principal_settings(
        &self,
        actor: &str,
        settings: &catalog::PrincipalSettings,
    ) -> BridgeResult<catalog::PrincipalSettings> {
        settings.validate()?;
        let roles = ADMIN_ROLES
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();
        let subject = PolicySubject {
            id: actor,
            kind: "human",
            human: true,
            roles: &roles,
        };
        let kinds = serde_json::to_string(&settings.kinds).expect("kinds JSON");
        transaction!(&self.store, |connection| {
            let owner = self.policy_owner_in(connection, actor).await?;
            if !settings.kinds.is_empty() {
                let used: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_agents WHERE tenant_id=$1 AND kind NOT IN (SELECT jsonb_array_elements_text($2::jsonb))")
                    .bind(&owner.tenant).bind(&kinds).fetch_one(&mut *connection).await?;
                if used > 0 {
                    return Err(BridgeError::new(
                        "in_use",
                        "a removed kind is assigned to an agent",
                    )
                    .into());
                }
            }
            let saved =
                catalog::put_settings_in(connection, &owner.tenant, &subject, settings).await?;
            record_configuration(
                connection,
                &owner.tenant,
                actor,
                "agent.settings.saved",
                &saved,
            )
            .await?;
            Ok(saved)
        })
    }
}

async fn record_configuration<T: Serialize>(
    connection: &mut AnyConnection,
    tenant: &str,
    actor: &str,
    event: &str,
    value: &T,
) -> Result<(), Failed> {
    let now = database_now(connection).await?;
    let detail = serde_json::to_string(value)
        .map_err(|_| BridgeError::new("invalid", "invalid configuration"))?;
    tenants::record_in(connection, tenant, actor, event, &detail, now).await
}
