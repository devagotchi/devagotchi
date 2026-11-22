//! Bounded system sweeps compose existing identity, task and device owners.
//! Grant endings never infer success from database failures or end a live once admission.
use super::*;
use super::{grant_record::GrantRecord, grants::Ended};
use crate::machines::devices::Devices;
use devagotchi_agenthub::{Actor, hosted::HostedBoard};
use serde_json::json;
use sqlx::Row;

pub(crate) struct ReapCandidate {
    pub id: String,
    pub account: String,
    pub project: Option<String>,
}
fn expected_refusal(error: Failed, codes: &[&str]) -> DbResult<()> {
    match error {
        Failed::Refused(error) if codes.contains(&error.code.as_str()) => Ok(()),
        error => Err(error),
    }
}
impl Access {
    /// Internal hub sweep only. The runtime enumerates environments, pages candidates,
    /// and resolves each original account/project without creating a missing board.
    pub(crate) async fn reap_candidates(
        &self,
        tenant: &str,
        after: Option<&str>,
        limit: u32,
    ) -> BridgeResult<Vec<ReapCandidate>> {
        if !(1..=100).contains(&limit) || after.is_some_and(|s| s.len() > 128) {
            return Err(invalid("invalid reaper page"));
        }
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            let rows = sqlx::query(
                "SELECT id,account,task_binding->>'project' AS project FROM machine_grants
                WHERE tenant_id=$1 AND ended_at IS NULL AND id>$2 ORDER BY id LIMIT $3",
            )
            .bind(tenant)
            .bind(after.unwrap_or(""))
            .bind(i64::from(limit))
            .fetch_all(c)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(ReapCandidate {
                        id: row.try_get("id")?,
                        account: row.try_get("account")?,
                        project: row.try_get("project")?,
                    })
                })
                .collect()
        })
    }
    /// `board=None` for a task-bound grant means the trusted registry found no current
    /// board. A registry/database error must be propagated by the caller, not mapped to None.
    pub(crate) async fn reap_grant(
        &self,
        tenant: &str,
        id: &str,
        board: Option<&HostedBoard>,
    ) -> BridgeResult<Option<Ended>> {
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            let grant = GrantRecord::load(c, tenant, id).await?;
            if grant.ended {
                return Ok(None);
            }
            let mut reason = None;
            let profile = match self
                .accounts
                .machine_profile_in(c, &grant.account, &grant.principal)
                .await
            {
                Ok(profile) => Some(profile),
                Err(error) => {
                    expected_refusal(error, &["not_found", "not_granted"])?;
                    reason = Some("principal_off");
                    None
                }
            };
            if let Some(binding) = &grant.task {
                if let Some(board) = board {
                    if let Err(error) = board.check_access_lease_in(c, binding).await {
                        expected_refusal(
                            error,
                            &[
                                "not_found",
                                "not_granted",
                                "stale_fence",
                                "not_holder",
                                "lease_expired",
                            ],
                        )?;
                        reason = Some("task_ended");
                    }
                } else {
                    reason = Some("task_ended");
                }
            }
            // Board lock above precedes any live profile/policy lock here.
            if reason.is_none()
                && let Some(profile) = profile
            {
                let actor = Actor {
                    id: &grant.principal,
                    human: profile.is_none(),
                    wip_limit: None,
                    policy: profile,
                };
                match self
                    .accounts
                    .machine_actor_in(c, &grant.account, &actor)
                    .await
                {
                    Ok(current) if current.tenant == tenant => (),
                    Ok(_) => {
                        return Err(refused(
                            "access_damaged",
                            "stored grant account changed environment",
                        ));
                    }
                    Err(error) => {
                        expected_refusal(error, &["not_found", "not_granted"])?;
                        reason = Some("principal_off");
                    }
                }
            }
            if reason.is_none() {
                if let Err(error) = Devices::new(self.accounts.store.clone())
                    .check_access_in(c, grant.machine.owner.as_deref(), &grant.machine)
                    .await
                {
                    expected_refusal(error, &["not_found", "not_granted", "machine_reenrolled"])?;
                    reason = Some("machine_revoked");
                }
            }
            // Identity lookup may establish scope; restore the sweep's trusted scope.
            self.accounts.store.scope(c, tenant).await?;
            Self::lock_in(c, tenant).await?;
            let grant = GrantRecord::load(c, tenant, id).await?;
            if grant.ended {
                return Ok(None);
            }
            let now = Self::now_in(c).await?;
            if reason.is_none() && grant.expires_at.is_some_and(|at| at <= now) {
                reason = Some("expired");
            }
            if reason.is_none()
                && grant.policy_required
                && !Self::policy_live_in(c, tenant, &grant.machine, now).await?
            {
                reason = Some("policy_disabled");
            }
            if reason.is_none()
                && grant
                    .terms
                    .uses_max
                    .is_some_and(|max| grant.uses >= i64::from(max))
            {
                let running:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM machine_admissions WHERE tenant_id=$1 AND grant_id=$2 AND state='admitted')")
                    .bind(tenant).bind(id).fetch_one(&mut *c).await?;
                if !running {
                    reason = Some("used_up");
                }
            }
            match reason {
                Some(reason) => Self::end_grant_in(c, tenant, id, reason, "system", now).await,
                None => Ok(None),
            }
        })
    }
    pub(crate) async fn expire_requests(&self, tenant: &str, limit: u32) -> BridgeResult<u64> {
        if !(1..=100).contains(&limit) {
            return Err(invalid("request expiry batch must be 1 to 100"));
        }
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            Self::lock_in(c, tenant).await?;
            let now = Self::now_in(c).await?;
            let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM machine_access_requests WHERE tenant_id=$1 AND status='pending' AND expires_at<=$2
                ORDER BY expires_at,id LIMIT $3 FOR UPDATE")
                .bind(tenant).bind(now).bind(i64::from(limit)).fetch_all(&mut *c).await?;
            for id in &ids {
                sqlx::query("UPDATE machine_access_requests SET status='expired',version=version+1 WHERE tenant_id=$1 AND id=$2")
                    .bind(tenant).bind(id).execute(&mut *c).await?;
                Self::audit_in(
                    c,
                    tenant,
                    "system",
                    "access.expired",
                    &json!({"request":id}),
                    now,
                )
                .await?;
            }
            Ok(ids.len() as u64)
        })
    }
}
