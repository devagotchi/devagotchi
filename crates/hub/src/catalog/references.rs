//! Referential checks compose with mutations under the environment catalog lock.
use super::*;
/// Called under the catalog lock, including for receipt replay and default edits.
pub(super) async fn default_selection(
    c: &mut AnyConnection,
    tenant: &str,
    id: &str,
    value: &Value,
    confirmation: Option<&str>,
) -> DbResult<()> {
    let items: Vec<devagotchi_contracts::GrantItem> =
        serde_json::from_value(value["items"].clone())
            .map_err(|_| refused("catalog_damaged", "invalid stored grant preset items"))?;
    if value["full_access"] == true
        || value["requires_policy"] == "unsandboxed"
        || items.iter().any(|item| item.needs_unsandboxed_policy())
    {
        if confirmation != Some(id) {
            return Err(refused(
                "confirm_required",
                "type the exact preset ID to confirm this default",
            ));
        }
        crate::machines::access::Access::require_environment_policy_in(c, tenant).await?;
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
        Kind::Capabilities=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog_capability_risks WHERE tenant_id=$1 AND capability_id=$2) OR EXISTS(SELECT 1 FROM catalog_grant_presets p,jsonb_array_elements(p.items) item WHERE p.tenant_id=$1 AND item->>'capability'=$2)").bind(tenant).bind(&row.id).fetch_one(&mut *c).await?,
        Kind::Risks=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog_capability_risks WHERE tenant_id=$1 AND risk_id=$2) OR EXISTS(SELECT 1 FROM catalog_docker_runtime_presets WHERE tenant_id=$1 AND jsonb_exists(risk_ids,$2))").bind(tenant).bind(&row.id).fetch_one(&mut *c).await?,
        Kind::ShellProfiles=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog_grant_presets p,jsonb_array_elements(p.items) item WHERE p.tenant_id=$1 AND item->'selector'->>'shell_profile'=$2)").bind(tenant).bind(&row.id).fetch_one(&mut *c).await?,
        _=>false,
    };
    if used || crate::machines::access::Access::catalog_in_use(c, tenant, kind, &row.id).await? {
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
