//! Referential checks compose with mutations under the environment catalog lock.
use super::*;
/// Data editing alone cannot establish the explicitly acknowledged access policy.
/// A04's access owner must compose that live policy/confirmation check here before
/// unsandboxed defaults are available. Until then, selection fails closed.
pub(super) fn default_selection(value: &Value) -> BridgeResult<()> {
    if value["full_access"] == true || value["requires_policy"] == "unsandboxed" {
        return Err(BridgeError::new(
            "policy_denied",
            "this default requires an enabled unsandboxed policy and explicit confirmation",
        ));
    }
    Ok(())
}
pub(super) async fn deletion(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    row: &Row,
) -> DbResult<()> {
    if row.is_default == Some(true) {
        return Err(refused(
            "in_use",
            "choose another default preset before deleting this one",
        ));
    }
    let used:bool=match kind {
        Kind::Capabilities=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog_capability_risks WHERE tenant_id=$1 AND capability_id=$2) OR EXISTS(SELECT 1 FROM catalog_grant_presets p,jsonb_array_elements(p.items) item WHERE p.tenant_id=$1 AND item->>'capability'=$2)").bind(tenant).bind(&row.id).fetch_one(c).await?,
        Kind::Risks=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog_capability_risks WHERE tenant_id=$1 AND risk_id=$2) OR EXISTS(SELECT 1 FROM catalog_docker_runtime_presets WHERE tenant_id=$1 AND jsonb_exists(risk_ids,$2))").bind(tenant).bind(&row.id).fetch_one(c).await?,
        Kind::ShellProfiles=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog_grant_presets p,jsonb_array_elements(p.items) item WHERE p.tenant_id=$1 AND item->'selector'->>'shell_profile'=$2)").bind(tenant).bind(&row.id).fetch_one(c).await?,
        _=>false,
    };
    if used {
        return Err(refused(
            "in_use",
            "another catalog record still refers to this record",
        ));
    }
    Ok(())
}
pub(super) async fn capability_change(
    c: &mut AnyConnection,
    tenant: &str,
    id: &str,
    value: &Value,
) -> DbResult<()> {
    let rows:Vec<String>=sqlx::query_scalar("SELECT item::text FROM catalog_grant_presets p,jsonb_array_elements(p.items) item WHERE p.tenant_id=$1 AND item->>'capability'=$2").bind(tenant).bind(id).fetch_all(c).await?;
    for raw in rows {
        let item: types::PresetItem = serde_json::from_str(&raw)
            .map_err(|_| refused("catalog_damaged", "invalid stored grant preset item"))?;
        validate::item_matches(&item, value).map_err(|_| {
            refused(
                "in_use",
                "this capability change would invalidate an existing grant preset",
            )
        })?;
    }
    Ok(())
}
