//! The settings the owner changes on `/admin` while the hub runs (hub-settings M7, free-edition
//! U0): each one's key, group, words and range ([`SETTINGS`]), the values in force
//! ([`LiveSettings`]) and the handle every holder of the config reads them through ([`Live`]).
//!
//! Where a value comes from, first that applies:
//!
//! 1. the hub's config file or its environment variable: then it is **pinned**, the page shows
//!    it and says where to change it (a deploy that set it keeps it);
//! 2. the value saved on `/admin`, kept in Postgres (`hub_settings`, migration 0034);
//! 3. the default.
//!
//! A save applies at once: the running hub reads the handle at the moment it decides, never a
//! copy taken at start. Keys a hub sets only at deploy are listed in [`DEPLOY_TIME`], each with
//! why; a test fails when a config key is in neither list.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};

use super::{
    AnalyticsConfig, HubConfig, MachinesConfig, WorkflowsConfig, tenancy::UnsecuredLimits,
};

mod catalog;
pub use catalog::*;

/// The values in force for every setting in [`SETTINGS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveSettings {
    pub starts_per_hour: u32,
    pub max_unsecured: u32,
    pub anonymous_connect: bool,
    pub unsecured: UnsecuredLimits,
    pub password_min: u32,
    pub auto_update: bool,
    pub link_ttl_secs: u64,
    pub retention_days: u32,
}

impl LiveSettings {
    /// The defaults, with the config file's values over them.
    pub fn from_config(config: &HubConfig) -> Self {
        Self::from_sections(
            &config.tenancy,
            config.machines,
            config.workflows,
            config.analytics,
        )
    }

    fn from_sections(
        tenancy: &super::TenancyConfig,
        machines: MachinesConfig,
        workflows: WorkflowsConfig,
        analytics: AnalyticsConfig,
    ) -> Self {
        Self {
            starts_per_hour: tenancy.starts_per_hour,
            max_unsecured: tenancy.max_unsecured,
            anonymous_connect: tenancy.anonymous_connect,
            unsecured: tenancy.unsecured,
            password_min: 8,
            auto_update: machines.auto_update,
            link_ttl_secs: workflows.link_ttl_secs,
            retention_days: analytics.retention_days,
        }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        Some(match key {
            "tenancy.starts_per_hour" => Value::Number(self.starts_per_hour.into()),
            "tenancy.max_unsecured" => Value::Number(self.max_unsecured.into()),
            "tenancy.anonymous_connect" => Value::Switch(self.anonymous_connect),
            "tenancy.unsecured.machines" => Value::Number(self.unsecured.machines.into()),
            "tenancy.unsecured.clients" => Value::Number(self.unsecured.clients.into()),
            "tenancy.unsecured.idle_days" => Value::Number(self.unsecured.idle_days.into()),
            "passwords.min_length" => Value::Number(self.password_min.into()),
            "machines.auto_update" => Value::Switch(self.auto_update),
            "workflows.link_ttl_secs" => Value::Number(self.link_ttl_secs as i64),
            "analytics.retention_days" => Value::Number(self.retention_days.into()),
            _ => return None,
        })
    }

    /// Set one setting; a value of the wrong kind or out of `u32` is refused (the range is
    /// [`Setting::parse`]'s).
    pub fn set(&mut self, key: &str, value: Value) -> bool {
        let as_u32 = |n: i64| u32::try_from(n).ok();
        match (key, value) {
            ("tenancy.anonymous_connect", Value::Switch(on)) => self.anonymous_connect = on,
            ("machines.auto_update", Value::Switch(on)) => self.auto_update = on,
            ("workflows.link_ttl_secs", Value::Number(n)) if n > 0 => self.link_ttl_secs = n as u64,
            (key, Value::Number(n)) => {
                let Some(n) = as_u32(n) else { return false };
                match key {
                    "tenancy.starts_per_hour" => self.starts_per_hour = n,
                    "tenancy.max_unsecured" => self.max_unsecured = n,
                    "tenancy.unsecured.machines" => self.unsecured.machines = n,
                    "tenancy.unsecured.clients" => self.unsecured.clients = n,
                    "tenancy.unsecured.idle_days" => self.unsecured.idle_days = n,
                    "passwords.min_length" => self.password_min = n,
                    "analytics.retention_days" => self.retention_days = n,
                    _ => return false,
                }
            }
            _ => return false,
        }
        true
    }

    /// These settings with the saved values over them, except the pinned ones. A saved value
    /// that no longer checks out (a range narrowed since) is left out and named in the log.
    pub fn with_saved(mut self, saved: &BTreeMap<String, String>, pinned: &Pinned) -> Self {
        for (key, text) in saved {
            let Some(setting) = Setting::find(key) else {
                continue;
            };
            if pinned.contains(key) {
                continue;
            }
            match setting.parse(text) {
                Ok(value) => {
                    self.set(key, value);
                }
                Err(why) => eprintln!("devagotchi hub: saved setting {key} left out: {why}"),
            }
        }
        self
    }
}

/// The settings a deploy set (in the config file or its environment variable): the page shows
/// them and does not change them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pinned(Arc<BTreeSet<&'static str>>);

impl Pinned {
    /// The settings `raw` (the config file as JSON) names, and those whose variable `env` has.
    pub fn from(raw: &serde_json::Value, env: &dyn Fn(&str) -> Option<String>) -> Self {
        let set = SETTINGS
            .iter()
            .filter(|setting| {
                let in_file = setting
                    .key
                    .split('.')
                    .try_fold(raw, |value, part| value.get(part))
                    .is_some();
                let in_env = setting
                    .env
                    .and_then(env)
                    .is_some_and(|value| !value.trim().is_empty());
                in_file || in_env
            })
            .map(|setting| setting.key)
            .collect();
        Self(Arc::new(set))
    }

    pub fn contains(&self, key: &str) -> bool {
        self.0.contains(key)
    }
}

/// The settings in force, shared by every holder of the config: a save on `/admin` replaces
/// them for all at once.
#[derive(Debug, Clone)]
pub struct Live(Arc<RwLock<LiveSettings>>);

impl Live {
    pub fn new(settings: LiveSettings) -> Self {
        Self(Arc::new(RwLock::new(settings)))
    }

    /// The settings now.
    pub fn get(&self) -> LiveSettings {
        *self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn replace(&self, settings: LiveSettings) {
        *self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
    }
}

impl Default for Live {
    fn default() -> Self {
        Self::new(LiveSettings::from_sections(
            &super::TenancyConfig::default(),
            MachinesConfig::default(),
            WorkflowsConfig::default(),
            AnalyticsConfig::default(),
        ))
    }
}

#[cfg(test)]
mod tests;
