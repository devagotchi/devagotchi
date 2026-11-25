use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct Capability {
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "object")]
    pub selector_schema: Value,
    #[serde(default)]
    pub sandbox_levels: Vec<String>,
    #[serde(default = "task")]
    pub default_duration: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct Risk {
    pub title: String,
    pub plain_text: String,
    pub severity: String,
    #[serde(default)]
    pub mitigation: String,
    pub source_url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct CapabilityRisk {
    pub capability_id: String,
    pub risk_id: String,
    #[serde(default = "object")]
    pub when_selector: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct ShellProfile {
    pub os: String,
    pub label: String,
    #[serde(default)]
    pub argv_prefix: Vec<String>,
    #[serde(default)]
    pub env_allow: Vec<String>,
    #[serde(default)]
    pub cwd_default: String,
    #[serde(default)]
    pub pty: bool,
}
