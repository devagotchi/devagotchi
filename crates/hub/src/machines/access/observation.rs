//! Existing GUI observation may accompany input only under current matching authority.
use super::*;
use crate::machines::devices::MachineBinding;
use devagotchi_agenthub::{
    hosted::TaskLeaseBinding,
    policy::{Resource, Subject, catalog as policy},
};

impl Access {
    pub(super) async fn indefinite_allowed_in(
        c: &mut AnyConnection,
        tenant: &str,
        subject: &Subject<'_>,
        resource: &Resource<'_>,
    ) -> DbResult<bool> {
        match policy::authorize_in(c, tenant, subject, "grant.indefinite", resource).await {
            Ok(()) => Ok(true),
            Err(Failed::Refused(error)) if error.code == "policy_denied" => Ok(false),
            Err(error) => Err(error),
        }
    }
    /// Current identity and machine have already been checked. A task-bound observation
    /// may participate only in the exact lease already proved and locked by this request.
    /// Heartbeat expiry/current labels are evidence, not lease identity.
    pub(super) async fn observation_in(
        c: &mut AnyConnection,
        tenant: &str,
        account: &str,
        principal: &str,
        machine: &MachineBinding,
        task: Option<&TaskLeaseBinding>,
        now: i64,
        indefinite_allowed: bool,
    ) -> DbResult<bool> {
        let policy_live = Self::policy_live_in(c, tenant, machine, now).await?;
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM machine_grants g JOIN machine_access_requests r
            ON r.tenant_id=g.tenant_id AND r.id=g.request_id WHERE g.tenant_id=$1 AND g.account=$2 AND g.principal=$3
            AND g.machine_binding=$4::jsonb AND g.capability='gui.observe' AND g.ended_at IS NULL AND NOT g.paused
            AND (g.expires_at IS NULL OR g.expires_at>$5) AND g.guide_binding IS NULL
            AND (g.task_binding IS NULL OR ($6::jsonb IS NOT NULL AND (g.task_binding-'expires_at'-'labels')=($6::jsonb-'expires_at'-'labels')))
            AND (g.duration<>'until_revoked' OR $7)
            AND ($8 OR (NOT g.full_access AND g.item->>'sandbox_level'<>'unsandboxed'
                AND (r.preset_snapshot->'requires_policy' IS NULL OR r.preset_snapshot->'requires_policy'='null'::jsonb)))
            AND (g.uses_max IS NULL OR g.uses<g.uses_max OR EXISTS(SELECT 1 FROM machine_admissions a
                WHERE a.tenant_id=g.tenant_id AND a.grant_id=g.id AND a.state='admitted'))) ")
            .bind(tenant).bind(account).bind(principal).bind(encoded(machine)?).bind(now).bind(task.map(encoded).transpose()?)
            .bind(indefinite_allowed).bind(policy_live).fetch_one(c).await?)
    }
}
