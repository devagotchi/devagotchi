use super::*;
use devagotchi_contracts::{
    GrantCapability, GrantDuration, GrantLevel, GrantRiskCondition, GrantSelector,
};
use types::*;

fn duration(value: &str) -> BridgeResult<()> {
    GrantDuration::try_from(value.to_owned())
        .map(|_| ())
        .map_err(invalid)
}

fn text(value: &str, required: bool) -> BridgeResult<()> {
    if (required && value.trim().is_empty()) || value.len() > 16_384 || value.contains('\0') {
        return Err(invalid(
            "catalog text is missing, too long or contains a NUL",
        ));
    }
    Ok(())
}
fn strings(values: &[String]) -> BridgeResult<()> {
    if values.len() > 256 {
        return Err(invalid("catalog list exceeds 256 values"));
    }
    for value in values {
        text(value, true)?;
    }
    Ok(())
}
fn address(value: &str) -> BridgeResult<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| invalid("catalog URL must be absolute HTTP or HTTPS"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid(
            "catalog URL must use HTTP or HTTPS and must not contain credentials",
        ));
    }
    Ok(())
}
fn decoded<T: serde::de::DeserializeOwned>(value: &Value) -> BridgeResult<T> {
    serde_json::from_value(value.clone()).map_err(|_| invalid("invalid catalog fields"))
}
pub(super) async fn record(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    id: &str,
    value: Value,
) -> DbResult<Value> {
    schema::bounded(&value)?;
    let value = types::normalized(kind, value)?;
    match kind {
        Kind::Capabilities => {
            let row: Capability = decoded(&value)?;
            text(&row.label, true)?;
            text(&row.description, false)?;
            text(&row.default_duration, true)?;
            duration(&row.default_duration)?;
            strings(&row.sandbox_levels)?;
            if row.sandbox_levels.is_empty()
                || row.sandbox_levels.iter().any(|v| {
                    !matches!(
                        v.as_str(),
                        "read-only" | "workspace-write" | "desktop" | "unsandboxed"
                    )
                })
            {
                return Err(invalid("capability needs supported sandbox levels").into());
            }
            schema::compile(&row.selector_schema)?;
            if let Ok(capability) = serde_json::from_value::<GrantCapability>(json!(id)) {
                for level in &row.sandbox_levels {
                    let level: GrantLevel = decoded(&json!(level))?;
                    if !capability.supports(level) {
                        return Err(
                            invalid("sandbox level is not supported by this capability").into()
                        );
                    }
                }
            }
        }
        Kind::Risks => {
            let row: Risk = decoded(&value)?;
            text(&row.title, true)?;
            text(&row.plain_text, true)?;
            text(&row.mitigation, false)?;
            if !matches!(row.severity.as_str(), "low" | "med" | "high") {
                return Err(invalid("risk severity must be low, med or high").into());
            }
            if let Some(url) = &row.source_url {
                address(url)?;
            }
        }
        Kind::CapabilityRisks => {
            let row: CapabilityRisk = decoded(&value)?;
            exists(c, tenant, Kind::Capabilities, &row.capability_id).await?;
            exists(c, tenant, Kind::Risks, &row.risk_id).await?;
            if !row.when_selector.is_object() {
                return Err(invalid("risk condition must be an object").into());
            }
            let condition: GrantRiskCondition = decoded(&row.when_selector)?;
            condition.validate().map_err(|e| invalid(e.message))?;
        }
        Kind::ShellProfiles => {
            let row: ShellProfile = decoded(&value)?;
            os(&row.os)?;
            text(&row.label, true)?;
            strings(&row.argv_prefix)?;
            strings(&row.env_allow)?;
            text(&row.cwd_default, false)?;
        }
        Kind::GrantPresets => {
            let row: GrantPreset = decoded(&value)?;
            text(&row.label, true)?;
            text(&row.description, false)?;
            text(&row.duration, true)?;
            duration(&row.duration)?;
            if row.full_access && row.requires_policy.as_deref() != Some("unsandboxed") {
                return Err(invalid("full access requires the unsandboxed policy").into());
            }
            if row
                .requires_policy
                .as_ref()
                .is_some_and(|v| v != "unsandboxed")
            {
                return Err(invalid("unknown required access policy").into());
            }
            if row.items.is_empty() || row.items.len() > 64 {
                return Err(invalid("a grant preset needs 1 to 64 items").into());
            }
            for item in &row.items {
                let cap = exists(c, tenant, Kind::Capabilities, &item.capability).await?;
                item_matches(item, &Value::Object(cap.data))?;
                if let Some(profile) = item.selector.get("shell_profile").and_then(Value::as_str) {
                    exists(c, tenant, Kind::ShellProfiles, profile).await?;
                }
            }
        }
        Kind::Runtimes => {
            let row: RuntimePreset = decoded(&value)?;
            os(&row.os)?;
            text(&row.label, true)?;
            text(&row.what, false)?;
            strings(&row.hosts)?;
            strings(&row.risk_ids)?;
            strings(&row.verify_argv)?;
            if row.facts.len() > 64 || row.agent_argv.len() > 64 || row.operator_commands.len() > 64
            {
                return Err(invalid("runtime preset has too many entries").into());
            }
            for fact in row.facts {
                text(&fact.text, true)?;
                if let Some(url) = fact.url {
                    address(&url)?;
                }
            }
            for (platform, command) in row.operator_commands {
                text(&platform, true)?;
                text(&command, true)?;
            }
            for argv in row.agent_argv {
                if argv.is_empty() {
                    return Err(invalid("runtime command argv cannot be empty").into());
                }
                strings(&argv)?;
            }
            for id in row.risk_ids {
                exists(c, tenant, Kind::Risks, &id).await?;
            }
        }
        Kind::Routers => {
            let row: RouterEndpoint = decoded(&value)?;
            text(&row.label, true)?;
            address(&row.url)?;
            if !matches!(row.kind.as_str(), "litellm" | "openrouter" | "direct") {
                return Err(invalid("unknown router kind").into());
            }
        }
        Kind::ChatApps => {
            let row: ChatApp = decoded(&value)?;
            text(&row.label, true)?;
            text(&row.connect_template, false)?;
            strings(&row.provider_ids)?;
            if row.provider_ids.iter().any(|id| !valid_id(id)) {
                return Err(invalid("invalid provider id").into());
            }
        }
    }
    Ok(value)
}
fn os(value: &str) -> BridgeResult<()> {
    if !matches!(value, "any" | "linux" | "macos" | "windows") {
        return Err(invalid("unsupported catalog OS"));
    }
    Ok(())
}
pub(super) async fn exists(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    id: &str,
) -> DbResult<Row> {
    if !valid_id(id) {
        return Err(invalid("invalid catalog reference id").into());
    }
    stored::get(c, tenant, kind, id)
        .await?
        .ok_or_else(|| invalid(format!("unknown {} record {id}", kind.name())).into())
}
pub(super) fn item_matches(item: &PresetItem, cap: &Value) -> BridgeResult<()> {
    let cap: Capability = decoded(cap)?;
    if !cap.sandbox_levels.contains(&item.sandbox_level) {
        return Err(invalid("capability does not offer that sandbox level"));
    }
    schema::instance(&cap.selector_schema, &item.selector)?;
    if let Ok(capability) = serde_json::from_value::<GrantCapability>(json!(item.capability)) {
        let level: GrantLevel = decoded(&json!(item.sandbox_level))?;
        if !capability.supports(level) {
            return Err(invalid("sandbox level is not supported by this capability"));
        }
        let selector: GrantSelector = decoded(&item.selector)?;
        selector.validate().map_err(|e| invalid(e.message))?;
    }
    Ok(())
}
