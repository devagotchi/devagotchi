use std::collections::BTreeSet;

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Deserialize;

use super::BoardEntry;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    #[serde(default)]
    pub repositories: Vec<GithubRepository>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubRepository {
    pub repository_id: u64,
    pub repository: String,
    pub project: String,
}

impl GithubConfig {
    pub(crate) fn mapping(&self, repository: &str) -> BridgeResult<&GithubRepository> {
        self.repositories
            .iter()
            .find(|entry| entry.repository.eq_ignore_ascii_case(repository))
            .ok_or_else(|| BridgeError::new("not_found", "repository is not mapped on this hub"))
    }

    pub fn check(&self, boards: &[BoardEntry]) -> BridgeResult<()> {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for entry in &self.repositories {
            let parts: Vec<_> = entry.repository.split('/').collect();
            if entry.repository_id == 0
                || entry.repository_id > i64::MAX as u64
                || parts.len() != 2
                || parts.iter().any(|part| {
                    part.is_empty()
                        || part.len() > 100
                        || !part
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                })
            {
                return Err(BridgeError::new(
                    "invalid_config",
                    "github.repositories needs a positive repository_id and owner/name",
                ));
            }
            if !ids.insert(entry.repository_id)
                || !names.insert(entry.repository.to_ascii_lowercase())
            {
                return Err(BridgeError::new(
                    "invalid_config",
                    "github.repositories contains a duplicate repository",
                ));
            }
            if !boards.iter().any(|board| board.project == entry.project) {
                return Err(BridgeError::new(
                    "invalid_config",
                    format!(
                        "github.repositories project {:?} must name a configured board",
                        entry.project
                    ),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_mapping_is_explicit_and_unique() {
        let boards = vec![BoardEntry {
            project: "demo".into(),
            dir: "unused".into(),
        }];
        let config: GithubConfig = serde_json::from_value(serde_json::json!({
            "repositories": [{"repository_id": 123, "repository": "acme/demo", "project": "demo"}]
        }))
        .unwrap();
        config.check(&boards).unwrap();
        let mut bad = config.clone();
        bad.repositories.push(config.repositories[0].clone());
        assert_eq!(bad.check(&boards).unwrap_err().code, "invalid_config");
        bad = config.clone();
        bad.repositories[0].project = "other".into();
        assert!(bad.check(&boards).is_err());
        bad = config;
        bad.repositories[0].repository_id = 0;
        assert!(bad.check(&boards).is_err());
    }
}
