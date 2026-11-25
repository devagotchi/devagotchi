use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct PresetItem {
    pub capability: String,
    #[serde(default = "object")]
    pub selector: Value,
    pub sandbox_level: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct GrantPreset {
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub items: Vec<PresetItem>,
    #[serde(default = "task")]
    pub duration: String,
    #[serde(default)]
    pub full_access: bool,
    pub requires_policy: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct Fact {
    pub text: String,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct RuntimePreset {
    pub os: String,
    pub label: String,
    #[serde(default)]
    pub what: String,
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub operator_commands: BTreeMap<String, String>,
    #[serde(default)]
    pub agent_argv: Vec<Vec<String>>,
    #[serde(default)]
    pub verify_argv: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub risk_ids: Vec<String>,
    #[serde(default)]
    pub licence_confirm: bool,
}
