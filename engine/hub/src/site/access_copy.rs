//! Human machine-access review text; requests and risk snapshots remain escaped data.
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessCopy {
    pub paused: String,
    pub granted: String,
    pub use_limit: String,
    pub capabilities: BTreeMap<String, String>,
    pub title: String,
    pub description: String,
    pub pending: String,
    pub history: String,
    pub grants: String,
    pub live: String,
    pub ended: String,
    pub empty: String,
    pub review: String,
    pub back: String,
    pub more: String,
    pub who: String,
    pub machine: String,
    pub reason: String,
    pub scope: String,
    pub risks: String,
    pub no_risks: String,
    pub mitigation: String,
    pub duration: String,
    pub uses: String,
    pub expires: String,
    pub task: String,
    pub status: String,
    pub approve: String,
    pub narrow: String,
    pub deny: String,
    pub later: String,
    pub note: String,
    pub paths: String,
    pub keep_paths: String,
    pub restrict_paths: String,
    pub paths_help: String,
    pub keep_duration: String,
    pub uses_help: String,
    pub sandbox: String,
    pub unsandboxed: String,
    pub unchanged: String,
    pub revoke: String,
    pub refused: String,
    pub unavailable: String,
    pub stale: String,
    pub request_count: String,
    pub once: String,
    pub task_duration: String,
    pub until_revoked: String,
    pub hours: String,
    pub no_scope: String,
    pub none: String,
    pub levels: BTreeMap<String, String>,
    pub selectors: BTreeMap<String, String>,
    pub statuses: BTreeMap<String, String>,
}

impl AccessCopy {
    pub(super) fn check(&self) -> Result<(), String> {
        for (name, map, keys) in [
            (
                "levels",
                &self.levels,
                &["read-only", "workspace-write", "desktop", "unsandboxed"][..],
            ),
            (
                "capabilities",
                &self.capabilities,
                &[
                    "exec",
                    "proc.interactive",
                    "fs.read",
                    "fs.write",
                    "net.egress",
                    "gui.observe",
                    "gui.input",
                    "gui.window",
                    "gui.app",
                ][..],
            ),
            (
                "selectors",
                &self.selectors,
                &[
                    "paths",
                    "argv_prefix",
                    "hosts",
                    "env_allow",
                    "actions",
                    "apps",
                    "windows",
                    "monitors",
                    "deny_keys",
                    "max_runtime_s",
                    "max_actions_per_minute",
                    "max_edge",
                    "tree_depth",
                    "shell_profile",
                ][..],
            ),
        ] {
            for key in keys {
                if map.get(*key).is_none_or(|v| v.trim().is_empty()) {
                    return Err(format!(
                        "content/messages.json: app.access.{name} lacks {key}"
                    ));
                }
            }
        }
        Ok(())
    }
}
