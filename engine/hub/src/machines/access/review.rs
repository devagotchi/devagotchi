//! Human review projections retain original scope, risks and identity after authority ends.
use super::*;
use crate::catalog::access::PreparedItem;

pub(crate) struct AccessReview {
    pub id: String,
    pub account: String,
    pub requester: String,
    pub machine: String,
    pub project: Option<String>,
    pub ticket: Option<String>,
    pub reason: String,
    pub prepared: PreparedItem,
    pub terms: GrantTerms,
    pub version: i64,
    pub status: String,
    pub expires_at: i64,
    pub granted: Option<GrantTerms>,
    pub decision_note: Option<String>,
}
impl Access {
    pub(crate) async fn review_request(
        &self,
        person: &str,
        id: &str,
    ) -> BridgeResult<AccessReview> {
        if id.is_empty() || id.len() > 64 {
            return Err(invalid("invalid request identifier"));
        }
        transaction!(&self.accounts.store, |c| {
            let owner = self.accounts.catalog_actor_in(c, person, true).await?;
            Self::lock_in(c, &owner.tenant).await?;
            let pending = records::Pending::load(c, &owner.tenant, id).await?;
            let now = Self::now_in(c).await?;
            let requester = self
                .accounts
                .machine_principal_label_in(c, &owner.tenant, &pending.account, &pending.principal)
                .await?;
            let decision: (Option<String>, Option<String>) = sqlx::query_as("SELECT grant_id,decision_note FROM machine_access_requests WHERE tenant_id=$1 AND id=$2")
                .bind(&owner.tenant).bind(id).fetch_one(&mut *c).await?;
            let granted = match decision.0 {
                Some(grant_id) => Some(
                    grant_record::GrantRecord::load(c, &owner.tenant, &grant_id)
                        .await?
                        .terms,
                ),
                None => None,
            };
            Ok(AccessReview {
                id: pending.id,
                account: pending.account,
                requester,
                granted,
                decision_note: decision.1,
                machine: pending.machine.name,
                project: pending.task.as_ref().map(|t| t.project.clone()),
                ticket: pending.task.as_ref().map(|t| t.ticket.clone()),
                reason: pending.reason,
                prepared: pending.prepared,
                terms: pending.terms,
                version: pending.version,
                status: if pending.status == "pending" && pending.expires_at <= now {
                    "expired".into()
                } else {
                    pending.status
                },
                expires_at: pending.expires_at,
            })
        })
    }
    pub(crate) async fn pending_count(&self, person: &str) -> BridgeResult<u64> {
        transaction!(&self.accounts.store, |c| {
            let owner = self.accounts.catalog_actor_in(c, person, true).await?;
            let now = Self::now_in(c).await?;
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM machine_access_requests WHERE tenant_id=$1 AND status='pending' AND expires_at>$2")
                .bind(&owner.tenant).bind(now).fetch_one(c).await?;
            u64::try_from(count)
                .map_err(|_| refused("access_damaged", "invalid pending request count"))
        })
    }
}
