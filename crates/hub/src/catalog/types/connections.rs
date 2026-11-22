use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct RouterEndpoint {
    pub label: String,
    pub url: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::catalog) struct ChatApp {
    pub label: String,
    #[serde(default)]
    pub provider_ids: Vec<String>,
    #[serde(default)]
    pub connect_template: String,
}
