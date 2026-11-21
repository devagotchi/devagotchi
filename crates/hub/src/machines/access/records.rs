//! Scoped immutable request material. State is reread after taking lifecycle locks.
use super::*;
use crate::{
    catalog::{Row as CatalogRow, access::PreparedItem},
    machines::devices::MachineBinding,
};
use devagotchi_agenthub::hosted::TaskLeaseBinding;
use devagotchi_contracts::GrantDuration;
use sqlx::Row;

pub(super) struct Pending {
    pub id: String,
    pub account: String,
    pub principal: String,
    pub machine: MachineBinding,
    pub task: Option<TaskLeaseBinding>,
    pub prepared: PreparedItem,
    pub preset: Option<CatalogRow>,
    pub terms: GrantTerms,
    pub version: i64,
    pub status: String,
    pub expires_at: i64,
    pub reason: String,
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> DbResult<T> {
    serde_json::from_str(value)
        .map_err(|_| refused("access_damaged", "invalid stored access snapshot"))
}
impl Pending {
    pub async fn load(c: &mut AnyConnection, tenant: &str, id: &str) -> DbResult<Self> {
        let row = sqlx::query("SELECT id,account,principal,machine_binding::text,task_binding::text,guide_binding::text,
            prepared_item::text,preset_snapshot::text,duration,uses_max,version,status,expires_at,reason
            FROM machine_access_requests WHERE tenant_id=$1 AND id=$2")
            .bind(tenant).bind(id).fetch_optional(c).await?
            .ok_or_else(|| refused("not_found", "access request not found"))?;
        if row.try_get::<Option<String>, _>("guide_binding")?.is_some() {
            return Err(refused(
                "not_granted",
                "guide-bound decisions need their live guide authority",
            ));
        }
        let prepared: PreparedItem = decode(&row.try_get::<String, _>("prepared_item")?)?;
        let duration =
            GrantDuration::try_from(row.try_get::<String, _>("duration")?).map_err(invalid)?;
        let uses_max = row
            .try_get::<Option<i64>, _>("uses_max")?
            .map(u32::try_from)
            .transpose()
            .map_err(|_| refused("access_damaged", "invalid stored use limit"))?;
        let terms = GrantTerms {
            item: prepared.item.clone(),
            duration,
            uses_max,
        };
        terms.validate()?;
        Ok(Self {
            id: row.try_get("id")?,
            account: row.try_get("account")?,
            principal: row.try_get("principal")?,
            machine: decode(&row.try_get::<String, _>("machine_binding")?)?,
            task: row
                .try_get::<Option<String>, _>("task_binding")?
                .map(|s| decode(&s))
                .transpose()?,
            preset: row
                .try_get::<Option<String>, _>("preset_snapshot")?
                .map(|s| decode(&s))
                .transpose()?,
            prepared,
            terms,
            version: row.try_get("version")?,
            status: row.try_get("status")?,
            expires_at: row.try_get("expires_at")?,
            reason: row.try_get("reason")?,
        })
    }
    pub fn require_pending(&self, expected: i64, now: i64, person: &str) -> DbResult<()> {
        if self.principal == person {
            return Err(refused(
                "own_request",
                "nobody approves or denies their own request",
            ));
        }
        if self.version != expected {
            return Err(refused(
                "version_conflict",
                "access request changed; reload before deciding",
            ));
        }
        if self.status != "pending" || self.expires_at <= now {
            return Err(refused(
                "not_pending",
                "access request is no longer pending",
            ));
        }
        Ok(())
    }
}
