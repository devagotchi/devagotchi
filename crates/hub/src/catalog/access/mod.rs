//! Catalog-owned preparation inside the access owner's transaction.
//! This is validated request material, not a grant or a machine admission.
//! Lock order: board (if bound), principal policy, catalog, then access records.
use super::*;
use devagotchi_contracts::{GrantDuration, GrantItem, GrantRiskCondition, GrantRiskContext};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Selection {
    Items { items: Vec<GrantItem> },
    Preset { id: String, version: i64 },
}
/// Facts come from the authenticated machine registry, never from tool arguments.
pub(crate) struct MachineFacts<'a> {
    pub os: &'a str,
    pub display_server: &'a str,
    pub kind: &'a str,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Prepared {
    pub items: Vec<PreparedItem>,
    pub duration: GrantDuration,
    pub preset: Option<Row>,
    pub needs_unsandboxed_policy: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PreparedItem {
    pub item: GrantItem,
    pub capability: Row,
    pub shell_profile: Option<Row>,
    pub risks: Vec<RiskSnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RiskSnapshot {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub plain_text: String,
    pub severity: String,
    pub mitigation: String,
    pub source_url: Option<String>,
    /// Preserve all matching predicates, including their versions, for explanation.
    pub conditions: Vec<Row>,
}

fn decode<T: serde::de::DeserializeOwned>(row: &Row) -> DbResult<T> {
    serde_json::from_value(Value::Object(row.data.clone()))
        .map_err(|_| refused("catalog_damaged", "invalid stored access catalog record"))
}
fn encode<T: Serialize>(value: &T) -> DbResult<Value> {
    serde_json::to_value(value)
        .map_err(|_| refused("catalog_damaged", "cannot encode access catalog record"))
}

impl Catalog {
    /// Acquire/seed the catalog without resolving a mutable preset before receipt replay.
    pub(crate) async fn access_scope_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
    ) -> DbResult<String> {
        self.enter(c, account, Kind::Capabilities, None).await
    }
    /// Snapshot acknowledged risks for the access owner while holding the catalog lock.
    pub(crate) async fn access_risks_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
        ids: &[String],
    ) -> DbResult<Vec<Row>> {
        if ids.is_empty() || ids.len() > 64 {
            return Err(invalid("acknowledge 1 to 64 catalog risks").into());
        }
        let tenant = self.enter(c, account, Kind::Risks, None).await?;
        let mut rows = Vec::with_capacity(ids.len());
        for id in ids {
            rows.push(validate::exists(c, &tenant, Kind::Risks, id).await?);
        }
        Ok(rows)
    }
    /// Caller must authenticate the canonical account and principal in this transaction.
    /// Board/task locks must already be held when the request is task-bound.
    /// Live identity, policy, task and machine authority still belong to the access owner.
    pub(crate) async fn prepare_access_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
        selection: &Selection,
        duration: Option<GrantDuration>,
        facts: &MachineFacts<'_>,
    ) -> DbResult<Prepared> {
        let tenant = self.enter(c, account, Kind::Capabilities, None).await?;
        schema::bounded(&encode(selection)?)?;
        let (items, preset, default_duration, required_policy) = match selection {
            Selection::Items { items } => (items.clone(), None, None, false),
            Selection::Preset { id, version } => {
                let row = validate::exists(c, &tenant, Kind::GrantPresets, id).await?;
                if *version < 1 || row.version != *version {
                    return Err(refused(
                        "version_conflict",
                        "grant preset changed; reload before requesting access",
                    ));
                }
                let preset: types::GrantPreset = decode(&row)?;
                let items = serde_json::from_value::<Vec<GrantItem>>(encode(&preset.items)?)
                    .map_err(|_| {
                        invalid("preset contains an unsupported executable capability or scope")
                    })?;
                let default_duration = GrantDuration::try_from(preset.duration).map_err(invalid)?;
                (
                    items,
                    Some(row),
                    Some(default_duration),
                    preset.full_access || preset.requires_policy.is_some(),
                )
            }
        };
        if items.is_empty() || items.len() > 64 {
            return Err(invalid("access request needs 1 to 64 items").into());
        }
        let mut output = Vec::with_capacity(items.len());
        let mut defaults = Vec::with_capacity(items.len());
        for item in items {
            item.validate().map_err(|e| invalid(e.message))?;
            let value = encode(&item)?;
            let id = value["capability"]
                .as_str()
                .expect("closed capability enum");
            let row = validate::exists(c, &tenant, Kind::Capabilities, id).await?;
            let typed: types::Capability = decode(&row)?;
            let preset_item =
                serde_json::from_value(value).map_err(|_| invalid("invalid access item"))?;
            validate::item_matches(&preset_item, &Value::Object(row.data.clone()))?;
            defaults.push(GrantDuration::try_from(typed.default_duration).map_err(invalid)?);
            let profile = match &item.selector.shell_profile {
                None => None,
                Some(id) => {
                    let row = validate::exists(c, &tenant, Kind::ShellProfiles, id).await?;
                    let profile: types::ShellProfile = decode(&row)?;
                    if profile.os != "any" && profile.os != facts.os {
                        return Err(invalid(
                            "shell profile does not support the target machine OS",
                        )
                        .into());
                    }
                    Some(row)
                }
            };
            output.push(PreparedItem {
                item,
                capability: row,
                shell_profile: profile,
                risks: Vec::new(),
            });
        }
        if let (Some(chosen), Some(preset_duration)) = (duration, default_duration)
            && !chosen.within(preset_duration)
        {
            return Err(refused(
                "scope_wider_than_preset",
                "duration exceeds the selected preset",
            ));
        }
        let duration = match duration.or(default_duration) {
            Some(value) => value,
            None if defaults.iter().all(|value| *value == defaults[0]) => defaults[0],
            None => {
                return Err(
                    invalid("choose a duration for capabilities with different defaults").into(),
                );
            }
        };
        duration.validate().map_err(|e| invalid(e.message))?;
        // Read the complete bounded set under the same catalog lock as presets and schemas.
        let conditions = stored::list(c, &tenant, Kind::CapabilityRisks, None, 1001).await?;
        if conditions.len() > 1000 {
            return Err(refused("catalog_damaged", "too many risk conditions"));
        }
        for prepared in &mut output {
            prepared.risks = risks(c, &tenant, prepared, duration, facts, &conditions).await?;
        }
        let prepared = Prepared {
            needs_unsandboxed_policy: required_policy
                || output.iter().any(|v| v.item.needs_unsandboxed_policy()),
            items: output,
            duration,
            preset,
        };
        schema::bounded(&encode(&prepared)?)?;
        Ok(prepared)
    }
}

async fn risks(
    c: &mut AnyConnection,
    tenant: &str,
    prepared: &PreparedItem,
    duration: GrantDuration,
    facts: &MachineFacts<'_>,
    conditions: &[Row],
) -> DbResult<Vec<RiskSnapshot>> {
    let mut matching: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for row in conditions {
        let condition: types::CapabilityRisk = decode(row)?;
        if condition.capability_id != prepared.capability.id {
            continue;
        }
        let predicate: GrantRiskCondition = serde_json::from_value(condition.when_selector)
            .map_err(|_| refused("catalog_damaged", "invalid stored risk predicate"))?;
        if predicate
            .matches(&GrantRiskContext {
                selector: &prepared.item.selector,
                level: prepared.item.sandbox_level,
                duration,
                display_server: facts.display_server,
                machine_kind: facts.kind,
            })
            .map_err(|_| refused("catalog_damaged", "invalid stored risk predicate"))?
        {
            matching
                .entry(condition.risk_id)
                .or_default()
                .push(row.clone());
        }
    }
    let mut result = Vec::with_capacity(matching.len());
    for (id, conditions) in matching {
        let row = validate::exists(c, tenant, Kind::Risks, &id).await?;
        let risk: types::Risk = decode(&row)?;
        result.push(RiskSnapshot {
            id,
            version: row.version,
            title: risk.title,
            plain_text: risk.plain_text,
            severity: risk.severity,
            mitigation: risk.mitigation,
            source_url: risk.source_url,
            conditions,
        });
    }
    let rank = |severity: &str| match severity {
        "high" => 0,
        "med" => 1,
        _ => 2,
    };
    result.sort_by(|a, b| (rank(&a.severity), &a.id).cmp(&(rank(&b.severity), &b.id)));
    Ok(result)
}
