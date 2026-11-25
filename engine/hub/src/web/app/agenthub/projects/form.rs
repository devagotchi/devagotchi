use super::*;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Draft {
    pub request_id: String,
    pub id: String,
    pub name: String,
    pub version: i64,
    pub workflow: String,
}
impl Draft {
    pub fn new() -> Self {
        Self {
            request_id: requests::request_id(),
            id: String::new(),
            name: String::new(),
            version: 0,
            workflow: String::new(),
        }
    }
    pub fn from_record(record: ProjectRecord, workflows: &[SharedWorkflow]) -> Self {
        let workflow = record
            .workflow_id
            .as_deref()
            .and_then(|id| workflows.iter().find(|w| w.workflow.id == id))
            .map(|w| format!("{}:{}", w.workflow.id, w.version))
            .unwrap_or_default();
        Self {
            request_id: requests::request_id(),
            id: record.id,
            name: record.name,
            version: record.version,
            workflow,
        }
    }
    pub fn edit(&self) -> Result<ProjectEdit, BridgeError> {
        let (workflow_id, workflow_version) = if self.workflow.is_empty() {
            (None, None)
        } else {
            let (id, version) = self
                .workflow
                .split_once(':')
                .ok_or_else(|| BridgeError::new("invalid", "invalid workflow selection"))?;
            let version = version
                .parse()
                .map_err(|_| BridgeError::new("invalid", "invalid workflow version"))?;
            (Some(id.to_owned()), Some(version))
        };
        Ok(ProjectEdit {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version,
            workflow_id,
            workflow_version,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Removal {
    pub request_id: String,
    pub version: i64,
}
