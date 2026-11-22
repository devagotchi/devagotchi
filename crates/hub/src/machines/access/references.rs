//! Catalog deletion checks stay with the owner of access records.
use super::*;
use crate::catalog::Kind;
impl Access {
    /// Catalog holds its mutation lock. Access writes take that lock before their own,
    /// so a concurrent request or settings change cannot introduce a dangling reference.
    pub(crate) async fn catalog_in_use(
        c: &mut AnyConnection,
        tenant: &str,
        kind: Kind,
        id: &str,
    ) -> DbResult<bool> {
        let sql = match kind {
            Kind::Risks => {
                "SELECT EXISTS(SELECT 1 FROM machine_access_settings WHERE tenant_id=$1 AND jsonb_exists(value->'unsandboxed_risk_ids',$2))"
            }
            Kind::Capabilities => {
                "SELECT EXISTS(SELECT 1 FROM machine_access_requests WHERE tenant_id=$1 AND capability=$2 AND status='pending') OR EXISTS(SELECT 1 FROM machine_grants WHERE tenant_id=$1 AND capability=$2 AND ended_at IS NULL)"
            }
            Kind::GrantPresets => {
                "SELECT EXISTS(SELECT 1 FROM machine_access_requests WHERE tenant_id=$1 AND preset_id=$2 AND status='pending') OR EXISTS(SELECT 1 FROM machine_grants WHERE tenant_id=$1 AND preset_id=$2 AND ended_at IS NULL)"
            }
            Kind::ShellProfiles => {
                "SELECT EXISTS(SELECT 1 FROM machine_access_requests WHERE tenant_id=$1 AND prepared_item->'item'->'selector'->>'shell_profile'=$2 AND status='pending') OR EXISTS(SELECT 1 FROM machine_grants WHERE tenant_id=$1 AND item->'selector'->>'shell_profile'=$2 AND ended_at IS NULL)"
            }
            _ => return Ok(false),
        };
        Ok(sqlx::query_scalar(sql)
            .bind(tenant)
            .bind(id)
            .fetch_one(c)
            .await?)
    }
}
