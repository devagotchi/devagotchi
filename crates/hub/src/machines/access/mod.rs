//! Durable machine access coordination; the broker owns actual execution.
//!
//! # Owns
//! machine_access_settings, machine_sandbox_policies, machine_access_requests,
//! machine_grants, machine_admissions, machine_access_notifications, machine_resume_alerts.
//! # Serves
//! Composable access lifecycle methods; HTTP/MCP/browser adapters are being absorbed.
//! # Settings
//! Access durations, pending expiry, denial cooldown, acknowledged risks and authority TTL.
//! # Depends on
//! Identity, AgentHub policy and leases, environment catalogs, machine registry and PostgreSQL.
use crate::{catalog::Catalog, identity::accounts::Accounts};
use devagotchi_contracts::{BridgeError, BridgeResult};
use devagotchi_store::{Failed, transaction};
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use std::sync::Arc;
mod admission_action;
mod admission_completion;
mod admissions;
mod control;
mod decisions;
mod grant_record;
mod grants;
mod notifications;
mod observation;
mod policies;
mod reaper;
mod records;
mod references;
mod requests;
mod resume;
mod resume_ack;
mod resume_agent;
mod terms;
pub(crate) use requests::{RequestInput, Requested};
pub(crate) use terms::{GrantTerms, Narrow};
mod settings;
pub(crate) use settings::AccessSettings;
#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod decision_tests;
#[cfg(test)]
mod default_tests;
#[cfg(test)]
mod grant_tests;
#[cfg(test)]
mod notification_tests;
#[cfg(test)]
mod observation_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod reaper_tests;
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod resume_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod task_tests;
#[cfg(test)]
mod terms_tests;

type DbResult<T> = Result<T, Failed>;
fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}
fn refused(code: &str, message: &str) -> Failed {
    BridgeError::new(code, message).into()
}
fn encoded<T: Serialize>(value: &T) -> DbResult<String> {
    serde_json::to_string(value).map_err(|_| invalid("access data cannot be encoded").into())
}
#[derive(Clone)]
pub(crate) struct Access {
    accounts: Arc<Accounts>,
    catalog: Catalog,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Changed<T> {
    pub result: T,
    pub replayed: bool,
}
impl Access {
    pub(crate) fn new(accounts: Arc<Accounts>) -> Self {
        Self {
            catalog: Catalog::new(accounts.clone()),
            accounts,
        }
    }
    async fn owner_in(&self, c: &mut AnyConnection, actor: &str, action: &str) -> DbResult<String> {
        self.owner_resource_in(
            c,
            actor,
            action,
            &devagotchi_agenthub::policy::Resource::default(),
        )
        .await
    }
    async fn owner_resource_in(
        &self,
        c: &mut AnyConnection,
        actor: &str,
        action: &str,
        resource: &devagotchi_agenthub::policy::Resource<'_>,
    ) -> DbResult<String> {
        use devagotchi_agenthub::{
            Actor,
            policy::{Subject, catalog},
        };
        let account = self
            .accounts
            .machine_actor_in(c, actor, &Actor::human(actor))
            .await?;
        if !account.owner {
            return Err(refused(
                "not_granted",
                "only the environment owner administers machine access",
            ));
        }
        catalog::initialize_in(c, &account.tenant).await?;
        catalog::authorize_in(
            c,
            &account.tenant,
            &Subject {
                id: actor,
                kind: "human",
                human: true,
                roles: &["admin".into()],
            },
            action,
            resource,
        )
        .await?;
        Ok(account.tenant)
    }
    async fn lock_in(c: &mut AnyConnection, tenant: &str) -> DbResult<()> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('machine_access:' || $1))")
            .bind(tenant)
            .execute(c)
            .await?;
        Ok(())
    }
    async fn now_in(c: &mut AnyConnection) -> DbResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(c)
                .await?,
        )
    }
    async fn audit_in(
        c: &mut AnyConnection,
        tenant: &str,
        actor: &str,
        action: &str,
        detail: &serde_json::Value,
        now: i64,
    ) -> DbResult<()> {
        crate::identity::accounts::tenants::record_in(
            c,
            tenant,
            actor,
            action,
            &detail.to_string(),
            (now.max(0) as u64) / 1000,
        )
        .await
    }
}
