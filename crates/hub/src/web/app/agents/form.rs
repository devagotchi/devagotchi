use super::*;

#[derive(Deserialize)]
pub(super) struct AgentForm {
    pub account: String,
    pub request_id: Option<String>,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub roles: String,
    pub wip: String,
    pub version: i64,
    #[serde(default)]
    pub enabled: bool,
}

impl AgentForm {
    pub fn parse(pairs: Vec<(String, String)>) -> devagotchi_contracts::BridgeResult<Self> {
        let mut fields = std::collections::BTreeMap::new();
        let mut selected = Vec::new();
        for (key, value) in pairs {
            if key == "role" {
                selected.push(value);
            } else if fields.insert(key, value).is_some() {
                return Err(devagotchi_contracts::BridgeError::new(
                    "invalid",
                    "duplicate agent field",
                ));
            }
        }
        if !selected.is_empty() {
            if fields.contains_key("roles") {
                return Err(devagotchi_contracts::BridgeError::new(
                    "invalid",
                    "conflicting role fields",
                ));
            }
            fields.insert("roles".into(), selected.join(","));
        }
        fields.entry("roles".into()).or_default();
        let version = fields
            .remove("version")
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(|| {
                devagotchi_contracts::BridgeError::new("invalid", "invalid agent version")
            })?;
        let enabled = match fields.remove("enabled").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            _ => {
                return Err(devagotchi_contracts::BridgeError::new(
                    "invalid",
                    "invalid enabled state",
                ));
            }
        };
        let request_id = fields.remove("request_id");
        let mut take = |name| {
            fields.remove(name).ok_or_else(|| {
                devagotchi_contracts::BridgeError::new("invalid", "missing agent field")
            })
        };
        Ok(Self {
            account: take("account")?,
            request_id,
            id: take("id")?,
            name: take("name")?,
            kind: take("kind")?,
            roles: take("roles")?,
            wip: take("wip")?,
            version,
            enabled,
        })
    }
    pub fn new(account: &str) -> Self {
        Self {
            account: account.into(),
            request_id: Some(request_id()),
            id: String::new(),
            name: String::new(),
            kind: "service".into(),
            roles: "worker".into(),
            wip: String::new(),
            version: 0,
            enabled: true,
        }
    }
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            account: agent.account.clone(),
            request_id: Some(request_id()),
            id: agent.id.clone(),
            name: agent.name.clone(),
            kind: agent.kind.clone(),
            roles: agent.roles.join(", "),
            wip: agent.wip_limit.map(|n| n.to_string()).unwrap_or_default(),
            version: agent.version,
            enabled: agent.enabled,
        }
    }
    pub fn edit(&self) -> devagotchi_contracts::BridgeResult<AgentEdit> {
        let wip_limit = if self.wip.trim().is_empty() {
            None
        } else {
            Some(
                self.wip
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .filter(|n| *n >= 0)
                    .ok_or_else(|| {
                        devagotchi_contracts::BridgeError::new(
                            "invalid",
                            "the active task limit must be zero or a positive whole number",
                        )
                    })?,
            )
        };
        Ok(AgentEdit {
            id: self.id.trim().into(),
            name: self.name.clone(),
            kind: self.kind.trim().into(),
            roles: self
                .roles
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect(),
            enabled: self.enabled,
            wip_limit,
            version: self.version,
        })
    }
}

pub(super) use super::super::requests::{request_field, request_id};
