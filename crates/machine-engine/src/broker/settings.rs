//! `get_config` and `set_config_value`: the broker's settings, read whole, and its limits
//! changed while it runs. A changed limit is saved to the broker.json the broker was started
//! from, so a restart keeps it, and applies at once; one that cannot be saved is refused and
//! changes nothing, so what runs is what the file says. The workspace, the exec policy and the
//! grants decide what an agent may do, so only the machine's operator changes them, in
//! broker.json; this refuses them with that reason.
//!
//! `broker_capabilities` reads the same policy for any caller, without the paths and grants
//! `get_config` shows: what this broker is, and the requests it answers under its policy.

use super::{Broker, ExecPolicy, path_string};
use crate::trace;
use devagotchi_contracts::*;
use std::path::Path;

/// What a `granted` broker refuses whatever it is asked: settings changes (`set_config`), and
/// the machine's processes and the web (`account_only` in `system.rs`). Its capabilities
/// leave them out.
const GRANTED_REFUSES: [&str; 4] = ["config_set", "process_list", "process_kill", "url_read"];

/// broker.json's limits when it leaves them out.
pub(super) fn default_output_limit() -> usize {
    262_144
}
pub(super) fn default_max_operations() -> usize {
    64
}
pub(super) fn default_max_concurrent() -> usize {
    4
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) output_limit_bytes: usize,
    pub(crate) max_operations: usize,
    pub(crate) max_concurrent: usize,
}

impl Limits {
    pub(crate) fn from_config(config: &super::BrokerConfig) -> BridgeResult<Self> {
        Self {
            output_limit_bytes: config.output_limit_bytes,
            max_operations: config.max_operations,
            max_concurrent: config.max_concurrent,
        }
        .checked()
        .map_err(|message| BridgeError::new("invalid_config", message))
    }

    /// These limits, or what is wrong with them.
    fn checked(self) -> Result<Self, String> {
        if !OUTPUT_LIMIT.contains(&self.output_limit_bytes) {
            return Err(format!(
                "output_limit_bytes must be {}..={} (it is {})",
                OUTPUT_LIMIT.start(),
                OUTPUT_LIMIT.end(),
                self.output_limit_bytes
            ));
        }
        if !OPERATIONS.contains(&self.max_operations) {
            return Err(format!(
                "max_operations must be {}..={} (it is {})",
                OPERATIONS.start(), OPERATIONS.end(),
                self.max_operations
            ));
        }
        if !CONCURRENT.contains(&self.max_concurrent) {
            return Err(format!(
                "max_concurrent must be {}..={} (it is {})",
                CONCURRENT.start(), CONCURRENT.end(),
                self.max_concurrent
            ));
        }
        if self.max_concurrent > self.max_operations {
            return Err(format!(
                "max_concurrent ({}) must be no greater than max_operations ({})",
                self.max_concurrent, self.max_operations
            ));
        }
        Ok(self)
    }

    /// These limits with `key` set to `value`.
    fn with(self, key: &str, value: u64) -> Result<Self, String> {
        let value = usize::try_from(value).map_err(|_| format!("{value} is too large"))?;
        let mut next = self;
        match key {
            "output_limit_bytes" => next.output_limit_bytes = value,
            "max_operations" => next.max_operations = value,
            "max_concurrent" => next.max_concurrent = value,
            _ => unreachable!("only a changeable key reaches here"),
        }
        next.checked()
    }

    fn get(self, key: &str) -> usize {
        match key {
            "output_limit_bytes" => self.output_limit_bytes,
            "max_operations" => self.max_operations,
            _ => self.max_concurrent,
        }
    }
}

impl Broker {
    /// What this broker is and what it answers, from its policy alone: no work admitted, no
    /// journal, roster or change touched. A faulted or closing broker says so instead.
    pub(super) fn capabilities(&self) -> BridgeResult<BrokerCapabilities> {
        {
            let state = self.shared.lock();
            if let Some(error) = &state.persistence_fault {
                return Err(error.clone());
            }
            if state.closing {
                return Err(BridgeError::new(
                    "shutting_down",
                    "The broker is shutting down and admits no work",
                ));
            }
        }
        let account = self.shared.config.exec_policy == ExecPolicy::Account;
        Ok(BrokerCapabilities {
            api_version: API_VERSION,
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            exec_policy: if account { "account" } else { "granted" }.into(),
            shell_commands: account,
            workspace_scope: if account { "account" } else { "workspace" }.into(),
            supported_requests: REQUEST_METHODS
                .iter()
                .filter(|method| account || !GRANTED_REFUSES.contains(method))
                .map(|method| method.to_string())
                .collect(),
        })
    }

    pub(super) fn config_view(&self, changed: Option<String>) -> BridgeResult<ConfigView> {
        let shared = &self.shared;
        let (limits, held, running) = {
            let state = shared.lock();
            (state.limits, state.operations.len(), state.active_count)
        };
        let mut grants = shared
            .executables
            .iter()
            .map(|path| path_string(path))
            .collect::<BridgeResult<Vec<_>>>()?;
        grants.sort();
        Ok(ConfigView {
            config_file: shared
                .config_file
                .as_deref()
                .map(|path| path.display().to_string()),
            workspace: path_string(&shared.workspace)?,
            exec_policy: match shared.config.exec_policy {
                ExecPolicy::Account => "account",
                ExecPolicy::Granted => "granted",
            }
            .into(),
            allowed_executables: grants,
            output_limit_bytes: limits.output_limit_bytes as u64,
            max_operations: limits.max_operations as u64,
            max_concurrent: limits.max_concurrent as u64,
            changeable: CHANGEABLE_SETTINGS
                .iter()
                .map(|key| key.to_string())
                .collect(),
            operations_held: held as u64,
            operations_running: running as u64,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            account: account_name(),
            home: std::env::home_dir().map(|home| home.display().to_string()),
            version: env!("CARGO_PKG_VERSION").into(),
            changed,
        })
    }

    pub(super) fn set_config(&self, input: ConfigSetInput) -> BridgeResult<ConfigView> {
        let key = input.key.as_str();
        if !CHANGEABLE_SETTINGS.contains(&key) {
            let reason = match key {
                "workspace" | "exec_policy" | "allowed_executables" => format!(
                    "{key} decides what an agent may do here, so only the machine's operator changes it, in broker.json (then restart the broker)"
                ),
                _ => format!("{key} is not a broker setting"),
            };
            return Err(BridgeError::new(
                "setting_not_changeable",
                format!(
                    "{reason}; set_config_value changes {}",
                    CHANGEABLE_SETTINGS.join(", ")
                ),
            ));
        }
        if self.shared.config.exec_policy == ExecPolicy::Granted {
            return Err(BridgeError::new(
                "setting_not_changeable",
                "This broker runs only granted programs, so its operator keeps every setting; ask them to change broker.json",
            ));
        }
        // Under the registry lock, so two changes are saved and applied in one order. Saved
        // first and applied only once saved: the live limits never differ from broker.json.
        let (before, after, saved, shrunk) = {
            let mut state = self.shared.lock();
            let before = state.limits;
            let after = before
                .with(key, input.value)
                .map_err(|message| BridgeError::new("invalid_request", message))?;
            let saved = match &self.shared.config_file {
                Some(file) => {
                    save(file, key, input.value).map_err(|error| {
                        BridgeError::new(
                            "config_not_saved",
                            format!(
                                "{key} stays {}: it could not be saved to {}: {error}",
                                before.get(key),
                                file.display()
                            ),
                        )
                    })?;
                    format!("saved to {}", file.display())
                }
                None => "applies until the broker restarts (it was not started from a broker.json)"
                    .into(),
            };
            state.limits = after;
            // Fewer kept: retire finished jobs down to the new limit now, not one per start.
            // Their keys stay retired; a journal error is returned, not swallowed.
            let shrunk = state.shrink();
            (before, after, saved, shrunk)
        };
        let changed = format!("{key} {} -> {}, {saved}", before.get(key), after.get(key));
        trace::line("settings", &changed);
        // The limit changed (and perhaps some operations retired) either way.
        self.shared.changed();
        shrunk.map_err(|error| {
            BridgeError::new(
                &error.code,
                format!(
                    "{changed}; retiring finished operations down to it stopped: {}",
                    error.message
                ),
            )
        })?;
        self.config_view(Some(changed))
    }
}

/// `key` set to `value` in the broker.json at `file`, everything else kept, replaced whole so
/// a crash leaves the old file or the new one, never half of one.
fn save(file: &Path, key: &str, value: u64) -> std::io::Result<()> {
    let text = std::fs::read(file)?;
    let mut config: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&text).map_err(std::io::Error::other)?;
    config.insert(key.into(), value.into());
    let mut bytes = serde_json::to_vec_pretty(&config).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    let next = file.with_extension("json.saving");
    std::fs::write(&next, bytes)?;
    std::fs::set_permissions(&next, std::fs::metadata(file)?.permissions())?;
    std::fs::rename(&next, file)
}

/// The OS account this broker runs as, from the OS itself: a service often has no `USER`.
fn account_name() -> Option<String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};
    let me = Pid::from_u32(std::process::id());
    let mut system = System::new().ok()?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[me]),
        false,
        ProcessRefreshKind::nothing().with_user(UpdateKind::Always),
    );
    let id = system.process(me)?.user_id()?.clone();
    let users = Users::new_with_refreshed_list().ok()?;
    users
        .get_user_by_id(&id)
        .map(|user| user.name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            output_limit_bytes: 262_144,
            max_operations: 64,
            max_concurrent: 4,
        }
    }

    #[test]
    fn a_limit_changes_only_within_its_range_and_its_neighbours() {
        assert_eq!(
            limits().with("max_concurrent", 8).unwrap().max_concurrent,
            8
        );
        assert!(
            limits()
                .with("max_concurrent", 65)
                .unwrap_err()
                .contains("1..=64")
        );
        assert!(
            limits()
                .with("max_operations", 2)
                .unwrap_err()
                .contains("no greater than")
        );
        assert!(
            limits()
                .with("output_limit_bytes", 10)
                .unwrap_err()
                .contains("1024..=")
        );
    }

    #[test]
    fn a_saved_limit_keeps_the_rest_of_the_file() {
        let dir = std::env::temp_dir().join(format!("dvg-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("broker.json");
        std::fs::write(
            &file,
            r#"{"workspace": "/w", "exec_policy": "account", "max_concurrent": 4}"#,
        )
        .unwrap();
        save(&file, "max_concurrent", 9).unwrap();
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(back["max_concurrent"], 9);
        assert_eq!(back["workspace"], "/w");
        assert_eq!(back["exec_policy"], "account");
        assert!(!dir.join("broker.json.saving").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
