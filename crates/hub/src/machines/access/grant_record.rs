//! Grant snapshots are immutable; current state is reread after lifecycle locks.
use super::*;
use crate::machines::devices::MachineBinding;
use devagotchi_agenthub::hosted::TaskLeaseBinding;
use devagotchi_contracts::{GrantDuration, GrantItem};
use sqlx::Row;

pub(super) struct GrantRecord {
    pub account: String,
    pub principal: String,
    pub machine: MachineBinding,
    pub task: Option<TaskLeaseBinding>,
    pub terms: GrantTerms,
    pub version: i64,
    pub uses: i64,
    pub expires_at: Option<i64>,
    pub paused: bool,
    pub ended: bool,
    pub policy_required: bool,
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> DbResult<T> {
    serde_json::from_str(value).map_err(|_| refused("access_damaged", "invalid grant snapshot"))
}
impl GrantRecord {
    pub async fn load(c: &mut AnyConnection, tenant: &str, id: &str) -> DbResult<Self> {
        let row=sqlx::query("SELECT g.account,g.principal,g.machine_binding::text,g.task_binding::text,g.guide_binding::text,
            g.item::text,g.duration,g.uses_max,g.version,g.uses,g.expires_at,g.paused,g.ended_at,g.full_access,r.preset_snapshot::text
            FROM machine_grants g JOIN machine_access_requests r ON r.tenant_id=g.tenant_id AND r.id=g.request_id
            WHERE g.tenant_id=$1 AND g.id=$2")
            .bind(tenant).bind(id).fetch_optional(c).await?.ok_or_else(||refused("grant_required","grant not found"))?;
        if row.try_get::<Option<String>, _>("guide_binding")?.is_some() {
            return Err(refused(
                "not_granted",
                "guide grant requires its live run authority",
            ));
        }
        let item: GrantItem = decode(&row.try_get::<String, _>("item")?)?;
        let preset: Option<serde_json::Value> = row
            .try_get::<Option<String>, _>("preset_snapshot")?
            .map(|s| decode(&s))
            .transpose()?;
        let policy_required = row.try_get::<bool, _>("full_access")?
            || item.needs_unsandboxed_policy()
            || preset
                .as_ref()
                .is_some_and(|p| p.get("requires_policy").is_some_and(|v| !v.is_null()));
        let terms = GrantTerms {
            item,
            duration: GrantDuration::try_from(row.try_get::<String, _>("duration")?)
                .map_err(invalid)?,
            uses_max: row
                .try_get::<Option<i64>, _>("uses_max")?
                .map(u32::try_from)
                .transpose()
                .map_err(|_| invalid("invalid grant use limit"))?,
        };
        terms.validate()?;
        Ok(Self {
            account: row.try_get("account")?,
            principal: row.try_get("principal")?,
            machine: decode(&row.try_get::<String, _>("machine_binding")?)?,
            task: row
                .try_get::<Option<String>, _>("task_binding")?
                .map(|s| decode(&s))
                .transpose()?,
            terms,
            version: row.try_get("version")?,
            uses: row.try_get("uses")?,
            expires_at: row.try_get("expires_at")?,
            paused: row.try_get("paused")?,
            ended: row.try_get::<Option<i64>, _>("ended_at")?.is_some(),
            policy_required,
        })
    }
    pub fn require_live(&self, now: i64) -> DbResult<()> {
        if self.ended || self.paused || self.expires_at.is_some_and(|at| at <= now) {
            return Err(refused("grant_ended", "grant is ended, paused or expired"));
        }
        // Use exhaustion only prohibits a fresh admission. A once process may continue.
        Ok(())
    }
}
