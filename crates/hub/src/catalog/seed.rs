//! Pinned public source data, marked once so deleted/edited defaults stay changed.
use super::*;
const FILES: [(&str, &str); 8] = [
    ("capabilities", include_str!("presets/capabilities.json")),
    ("risks", include_str!("presets/risks.json")),
    (
        "capability-risks",
        include_str!("presets/capability-risks.json"),
    ),
    (
        "shell-profiles",
        include_str!("presets/shell-profiles.json"),
    ),
    ("grant-presets", include_str!("presets/grant-presets.json")),
    ("runtimes", include_str!("presets/runtimes.json")),
    ("routers", include_str!("presets/routers.json")),
    ("chat-apps", include_str!("presets/chat-apps.json")),
];
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    kind: String,
    rows: Vec<serde_json::Map<String, Value>>,
}
pub(super) async fn initialize(c: &mut AnyConnection, tenant: &str) -> DbResult<()> {
    for (name, raw) in FILES {
        let inserted = sqlx::query(
            "INSERT INTO catalog_seed_marks(tenant_id,name) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(tenant)
        .bind(name)
        .execute(&mut *c)
        .await?
        .rows_affected();
        if inserted == 0 {
            continue;
        }
        let file: File = serde_json::from_str(raw)
            .map_err(|_| refused("catalog_seed", "invalid embedded catalog file"))?;
        let kind = Kind::parse(&file.kind)
            .ok_or_else(|| refused("catalog_seed", "unknown embedded catalog kind"))?;
        for mut data in file.rows {
            let id = data
                .remove("id")
                .and_then(|id| id.as_str().map(str::to_owned))
                .filter(|id| valid_id(id))
                .ok_or_else(|| refused("catalog_seed", "invalid embedded catalog id"))?;
            if stored::get(c, tenant, kind, &id).await?.is_some() {
                continue;
            }
            let value = validate::record(c, tenant, kind, &id, Value::Object(data)).await?;
            let default = if kind == Kind::GrantPresets && id == "task-scoped" {
                !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM catalog_grant_presets WHERE tenant_id=$1 AND is_default)")
                    .bind(tenant).fetch_one(&mut *c).await?
            } else {
                false
            };
            let row = Row {
                id,
                version: 1,
                is_default: (kind == Kind::GrantPresets).then_some(default),
                data: value.as_object().expect("typed catalog object").clone(),
            };
            stored::save(c, tenant, kind, &row, true).await?;
        }
    }
    Ok(())
}
