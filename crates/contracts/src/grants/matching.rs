use super::*;
use selector::{host_within, path_within, valid_host};

/// The broker derives this from the actual operation, never from a client's assertion.
#[derive(Debug, Clone)]
pub struct GrantCall {
    pub capability: GrantCapability,
    pub level: GrantLevel,
    pub argv: Vec<String>,
    pub paths: Vec<String>,
    pub hosts: Vec<String>,
    pub env_names: Vec<String>,
    pub apps: Vec<String>,
    pub windows: Vec<String>,
    pub monitors: Vec<String>,
    pub actions: Vec<String>,
    pub keys: Vec<String>,
    pub runtime_s: Option<u32>,
    pub actions_per_minute: Option<u32>,
    pub max_edge: Option<u32>,
    pub tree_depth: Option<u32>,
    pub shell_profile: Option<String>,
}
impl GrantCall {
    pub fn new(capability: GrantCapability, level: GrantLevel) -> Self {
        Self {
            capability,
            level,
            argv: Vec::new(),
            paths: Vec::new(),
            hosts: Vec::new(),
            env_names: Vec::new(),
            apps: Vec::new(),
            windows: Vec::new(),
            monitors: Vec::new(),
            actions: Vec::new(),
            keys: Vec::new(),
            runtime_s: None,
            actions_per_minute: None,
            max_edge: None,
            tree_depth: None,
            shell_profile: None,
        }
    }
}
fn list_fits(
    actual: &[String],
    allowed: &Option<Vec<String>>,
    absent_any: bool,
    includes: impl Fn(&str, &str) -> bool,
) -> bool {
    actual.iter().all(|v| match allowed {
        None => absent_any,
        Some(allowed) => allowed.iter().any(|p| includes(v, p)),
    })
}
fn names_fit(actual: &[String], allowed: &Option<AllowedNames>) -> bool {
    actual
        .iter()
        .all(|v| allowed.as_ref().is_none_or(|a| a.contains(v)))
}
fn limit_fits(actual: Option<u32>, limit: Option<u32>) -> bool {
    limit.is_none_or(|limit| actual.is_some_and(|actual| actual > 0 && actual <= limit))
}
impl GrantSelector {
    pub fn permits(
        &self,
        capability: GrantCapability,
        level: GrantLevel,
        call: &GrantCall,
    ) -> BridgeResult<()> {
        self.validate()?;
        selector::argv(&call.argv)?;
        for list in [
            &call.paths,
            &call.hosts,
            &call.env_names,
            &call.apps,
            &call.windows,
            &call.monitors,
            &call.actions,
            &call.keys,
        ] {
            selector::strings(list)?;
        }
        if call.paths.iter().any(|p| selector::lexical(p).is_none()) {
            return Err(invalid("call paths need an absolute or home anchor"));
        }
        if !capability.covers(call.capability)
            || !call.level.within(level)
            || !capability.supports(level)
            || !call.capability.supports(call.level)
        {
            return Err(denied("capability or sandbox level is outside this grant"));
        }
        if call.hosts.iter().any(|host| !valid_host(host, false)) {
            return Err(invalid("a call must name concrete hosts"));
        }
        if matches!(
            call.capability,
            GrantCapability::Exec | GrantCapability::Process
        ) && call.argv.is_empty()
        {
            return Err(invalid(
                "command admission needs its actual argument vector",
            ));
        }
        if matches!(
            call.capability,
            GrantCapability::Exec
                | GrantCapability::Process
                | GrantCapability::FileRead
                | GrantCapability::FileWrite
        ) && call.paths.is_empty()
        {
            return Err(invalid("operation admission needs its actual target paths"));
        }
        if matches!(
            call.capability,
            GrantCapability::Input | GrantCapability::Window | GrantCapability::App
        ) && call.apps.is_empty()
        {
            return Err(invalid("GUI admission needs its actual target app"));
        }
        if call.capability == GrantCapability::Input && call.actions.is_empty() {
            return Err(invalid("GUI input needs its actual action"));
        }
        if call.capability == GrantCapability::Observe
            && call.monitors.is_empty()
            && call.windows.is_empty()
        {
            return Err(invalid(
                "observation needs actual monitor or window targets",
            ));
        }
        if call.capability == GrantCapability::Network && call.hosts.is_empty() {
            return Err(invalid("network admission needs actual hosts"));
        }
        let exact = |a: &str, b: &str| a == b;
        for (ok, why) in [
            (
                list_fits(&call.paths, &self.paths, true, path_within),
                "path outside grant",
            ),
            (
                list_fits(&call.hosts, &self.hosts, false, host_within),
                "host outside grant",
            ),
            (
                list_fits(&call.env_names, &self.env_allow, false, exact),
                "environment outside grant",
            ),
            (
                list_fits(&call.actions, &self.actions, true, exact),
                "action outside grant",
            ),
            (names_fit(&call.apps, &self.apps), "app outside grant"),
            (
                names_fit(&call.windows, &self.windows),
                "window outside grant",
            ),
            (
                names_fit(&call.monitors, &self.monitors),
                "monitor outside grant",
            ),
            (
                call.keys
                    .iter()
                    .all(|k| self.deny_keys.as_ref().is_none_or(|keys| !keys.contains(k))),
                "key denied by grant",
            ),
            (
                limit_fits(call.runtime_s, self.max_runtime_s),
                "runtime exceeds grant",
            ),
            (
                limit_fits(call.actions_per_minute, self.max_actions_per_minute),
                "action rate exceeds grant",
            ),
            (
                limit_fits(call.max_edge, self.max_edge),
                "capture size exceeds grant",
            ),
            (
                limit_fits(call.tree_depth, self.tree_depth),
                "tree depth exceeds grant",
            ),
            (
                self.shell_profile
                    .as_ref()
                    .is_none_or(|p| call.shell_profile.as_ref() == Some(p)),
                "shell profile outside grant",
            ),
        ] {
            if !ok {
                return Err(denied(why));
            }
        }
        if !call.argv.is_empty()
            && self
                .argv_prefix
                .as_ref()
                .is_some_and(|prefixes| !prefixes.iter().any(|p| call.argv.starts_with(p)))
        {
            return Err(denied("command outside grant"));
        }
        Ok(())
    }
}
