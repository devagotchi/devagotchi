//! Principal administration uses the same key store, with live authority in its transaction.
use super::*;
use crate::identity::accounts::receipts::{MutationResult, Receipt};
use devagotchi_agenthub::policy::{Resource, Subject as PolicySubject, catalog};

impl Accounts {
    async fn key_manager_in(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
        account: &str,
        action: &str,
    ) -> Result<String, Failed> {
        let manager = self.agent_manager_in(connection, actor, account).await?;
        if manager.owner {
            catalog::initialize_in(connection, &manager.tenant).await?;
            catalog::authorize_in(
                connection,
                &manager.tenant,
                &PolicySubject {
                    id: actor,
                    kind: "human",
                    human: true,
                    roles: &["admin".into()],
                },
                action,
                &Resource::default(),
            )
            .await?;
        }
        Ok(manager.tenant)
    }

    /// Only authenticated people may call these administration methods. An ordinary
    /// person manages their own credentials; an owner may manage this environment.
    pub async fn agent_keys(
        &self,
        actor: &str,
        account: &str,
        agent: &str,
        after: Option<&str>,
    ) -> BridgeResult<Vec<ApiKey>> {
        transaction!(&self.store, |connection| {
            let manager = self.agent_manager_in(connection, actor, account).await?;
            super::super::agents::agent_in(connection, &manager.tenant, account, agent).await?;
            let rows = sqlx::query(
                "SELECT id,name,hint,scopes,projects,machines,agent_id,created_at,expires_at,last_used_at,revoked_at
                 FROM auth_api_keys WHERE tenant_id=$1 AND account=$2 AND agent_id=$3
                 AND ($4::text IS NULL OR id>$4) ORDER BY id LIMIT 101",
            )
            .bind(&manager.tenant)
            .bind(account)
            .bind(agent)
            .bind(after)
            .fetch_all(&mut *connection)
            .await?;
            rows.iter()
                .map(|row| key_from_row(row).map_err(Failed::from))
                .collect()
        })
    }

    /// Issuance deliberately has no receipt: the secret is returned only once.
    pub async fn create_agent_key(
        &self,
        actor: &str,
        account: &str,
        new: &NewKey,
    ) -> BridgeResult<(ApiKey, String)> {
        new.check()?;
        if new.agent_id.is_none() {
            return Err(invalid("principal token requires a registered agent"));
        }
        let now = now_secs();
        let secret = fresh_secret();
        let key = transaction!(&self.store, |connection| {
            let tenant = self
                .key_manager_in(connection, actor, account, "admin.tokens.create")
                .await?;
            let key = self
                .insert_key(connection, &tenant, account, new, &secret, now)
                .await?;
            if actor != account {
                tenants::record_in(
                    connection,
                    &tenant,
                    actor,
                    "agent.token.issued",
                    &format!("{} for {}", key.id, account),
                    now,
                )
                .await?;
            }
            Ok(key)
        })?;
        Ok((key, secret))
    }

    pub async fn revoke_managed_key(
        &self,
        actor: &str,
        account: &str,
        id: &str,
        request_key: Option<&str>,
    ) -> BridgeResult<MutationResult<()>> {
        transaction!(&self.store, |connection| {
            let tenant = self
                .key_manager_in(connection, actor, account, "admin.tokens.revoke")
                .await?;
            let (operation, input) = if actor == account {
                ("revoke_key", serde_json::json!(id))
            } else {
                ("revoke_managed_key", serde_json::json!((account, id)))
            };
            let receipt =
                Receipt::begin(connection, &tenant, actor, request_key, operation, &input).await?;
            if let Receipt::Replay(saved) = receipt {
                return Ok(MutationResult::replay(saved));
            }
            self.end_key(connection, &tenant, account, id, now_secs())
                .await?;
            if actor != account {
                tenants::record_in(
                    connection,
                    &tenant,
                    actor,
                    "agent.token.revoked",
                    &format!("{id} for {account}"),
                    now_secs(),
                )
                .await?;
            }
            receipt.complete(connection, ()).await
        })
    }
}
