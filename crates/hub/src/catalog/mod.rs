//! Editable environment catalog data. Owns catalog_*; grants own actual execution authority.
//! Reads and writes share live identity, PostgreSQL, policy, receipts and the hub audit owner.
//!
//! # Owns
//! PostgreSQL catalog_* tables and per-file seed marks (migration 0039).
//! # Serves
//! Eight /api/v1 catalog collections and owner editors under /app/catalog.
//! # Settings
//! Catalog fields and the default preset are edited in those owner pages.
//! # Depends on
//! Identity, AgentHub policy, shared request receipts and the hub's audit owner.
use crate::identity::accounts::Accounts;
use devagotchi_contracts::{BridgeError, BridgeResult};
use devagotchi_store::{Failed, transaction};
use serde_json::{Value, json};
use sqlx::AnyConnection;
use std::sync::Arc;
pub(crate) mod access;
mod kinds;
mod mutations;
mod references;
mod schema;
mod seed;
mod stored;
#[cfg(test)]
mod tests;
mod types;
mod validate;
pub(crate) use kinds::{FieldType, Kind};
pub(crate) use types::Row;
type DbResult<T> = Result<T, Failed>;
fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}
fn refused(code: &str, message: impl Into<String>) -> Failed {
    BridgeError::new(code, message).into()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}

#[derive(Clone)]
pub(crate) struct Catalog {
    accounts: Arc<Accounts>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Changed<T> {
    pub result: T,
    pub replayed: bool,
}
impl Catalog {
    pub fn new(accounts: Arc<Accounts>) -> Self {
        Self { accounts }
    }
    pub async fn require_owner(&self, actor: &str) -> BridgeResult<()> {
        transaction!(&self.accounts.store, |c| {
            self.accounts.catalog_actor_in(c, actor, true).await?;
            Ok(())
        })
    }
    async fn enter(
        &self,
        c: &mut AnyConnection,
        actor: &str,
        kind: Kind,
        action: Option<&str>,
    ) -> DbResult<String> {
        let account = self
            .accounts
            .catalog_actor_in(c, actor, action.is_some())
            .await?;
        if let Some(action) = action {
            use devagotchi_agenthub::policy::{Resource, Subject, catalog};
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
                &format!("{}.{}", kind.policy_prefix(), action),
                &Resource::default(),
            )
            .await?;
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('catalog:' || $1))")
            .bind(&account.tenant)
            .execute(&mut *c)
            .await?;
        seed::initialize(c, &account.tenant).await?;
        Ok(account.tenant)
    }
    pub async fn list(
        &self,
        actor: &str,
        kind: Kind,
        after: Option<&str>,
    ) -> BridgeResult<Vec<Row>> {
        if after.is_some_and(|id| !valid_id(id)) {
            return Err(invalid("invalid catalog cursor"));
        }
        transaction!(&self.accounts.store, |c| {
            let tenant = self.enter(c, actor, kind, None).await?;
            stored::list(c, &tenant, kind, after, 101).await
        })
    }
    pub async fn get(&self, actor: &str, kind: Kind, id: &str) -> BridgeResult<Row> {
        if !valid_id(id) {
            return Err(invalid("invalid catalog id"));
        }
        transaction!(&self.accounts.store, |c| {
            let tenant = self.enter(c, actor, kind, None).await?;
            stored::get(c, &tenant, kind, id)
                .await?
                .ok_or_else(|| refused("not_found", "catalog record does not exist"))
        })
    }
}
