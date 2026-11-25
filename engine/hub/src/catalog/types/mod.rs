//! Typed catalog rows; partial writes validate the resulting complete record.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
mod capabilities;
mod connections;
mod presets;
pub(super) use capabilities::*;
pub(super) use connections::*;
pub(super) use presets::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Row {
    pub id: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    #[serde(flatten)]
    pub data: serde_json::Map<String, Value>,
}

fn object() -> Value {
    json!({})
}
fn task() -> String {
    "task".into()
}

pub(super) fn normalized(
    kind: super::Kind,
    value: Value,
) -> devagotchi_contracts::BridgeResult<Value> {
    fn typed<T: serde::de::DeserializeOwned + Serialize>(
        value: Value,
    ) -> devagotchi_contracts::BridgeResult<Value> {
        let value: T = serde_json::from_value(value)
            .map_err(|error| super::invalid(format!("invalid catalog fields: {error}")))?;
        serde_json::to_value(value).map_err(|_| super::invalid("cannot encode catalog fields"))
    }
    match kind {
        super::Kind::Capabilities => typed::<Capability>(value),
        super::Kind::Risks => typed::<Risk>(value),
        super::Kind::CapabilityRisks => typed::<CapabilityRisk>(value),
        super::Kind::ShellProfiles => typed::<ShellProfile>(value),
        super::Kind::GrantPresets => typed::<GrantPreset>(value),
        super::Kind::Runtimes => typed::<RuntimePreset>(value),
        super::Kind::Routers => typed::<RouterEndpoint>(value),
        super::Kind::ChatApps => typed::<ChatApp>(value),
    }
}
