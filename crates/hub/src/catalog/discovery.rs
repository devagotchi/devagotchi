//! One bounded catalog snapshot for request discovery; failures never become empty lists.
use super::*;
impl Catalog {
    pub(crate) async fn access_summary_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
    ) -> DbResult<Value> {
        let tenant = self.enter(c, account, Kind::Capabilities, None).await?;
        let mut result = json!({});
        for (key, kind) in [
            ("capabilities", Kind::Capabilities),
            ("presets", Kind::GrantPresets),
            ("risks", Kind::Risks),
            ("capability_risks", Kind::CapabilityRisks),
        ] {
            let rows = stored::list(c, &tenant, kind, None, 1001).await?;
            if rows.len() > 1000 {
                return Err(refused(
                    "catalog_damaged",
                    "access catalog exceeds its bounded size",
                ));
            }
            if kind == Kind::GrantPresets {
                result["default_preset"] = json!(
                    rows.iter()
                        .find(|row| row.is_default == Some(true))
                        .map(|row| &row.id)
                );
            }
            result[key] = serde_json::to_value(rows)
                .map_err(|_| refused("catalog_damaged", "catalog cannot be encoded"))?;
        }
        Ok(result)
    }
}
