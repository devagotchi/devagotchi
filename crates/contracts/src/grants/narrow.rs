use super::*;
use selector::{host_within, path_within};

fn list_within(
    g: &Option<Vec<String>>,
    r: &Option<Vec<String>>,
    absent_any: bool,
    includes: impl Fn(&str, &str) -> bool,
) -> bool {
    match (g, r) {
        (_, None) if absent_any => true,
        (None, Some(_)) if absent_any => false,
        (None, _) => true,
        (Some(g), r) => g
            .iter()
            .all(|v| r.as_ref().is_some_and(|r| r.iter().any(|p| includes(v, p)))),
    }
}
fn names_within(g: &Option<AllowedNames>, r: &Option<AllowedNames>) -> bool {
    match (g, r) {
        (_, None | Some(AllowedNames::Any(_))) => true,
        (Some(AllowedNames::Names(g)), Some(AllowedNames::Names(r))) => {
            g.iter().all(|v| r.contains(v))
        }
        _ => false,
    }
}
fn limit_within(g: Option<u32>, r: Option<u32>) -> bool {
    match r {
        None => true,
        Some(r) => g.is_some_and(|g| g <= r),
    }
}
impl GrantSelector {
    /// Every field is checked against the same semantics used by call matching.
    pub fn within(&self, requested: &Self) -> BridgeResult<()> {
        self.validate()?;
        requested.validate()?;
        let exact = |a: &str, b: &str| a == b;
        for (ok, why) in [
            (
                list_within(&self.paths, &requested.paths, true, path_within),
                "paths widened",
            ),
            (
                list_within(&self.hosts, &requested.hosts, false, host_within),
                "hosts widened",
            ),
            (
                list_within(&self.env_allow, &requested.env_allow, false, exact),
                "environment widened",
            ),
            (
                list_within(&self.actions, &requested.actions, true, exact),
                "actions widened",
            ),
            (names_within(&self.apps, &requested.apps), "apps widened"),
            (
                names_within(&self.windows, &requested.windows),
                "windows widened",
            ),
            (
                names_within(&self.monitors, &requested.monitors),
                "monitors widened",
            ),
            (
                list_within(&requested.deny_keys, &self.deny_keys, false, exact),
                "denied keys removed",
            ),
            (
                limit_within(self.max_runtime_s, requested.max_runtime_s),
                "runtime limit widened",
            ),
            (
                limit_within(
                    self.max_actions_per_minute,
                    requested.max_actions_per_minute,
                ),
                "action rate widened",
            ),
            (
                limit_within(self.max_edge, requested.max_edge),
                "capture size widened",
            ),
            (
                limit_within(self.tree_depth, requested.tree_depth),
                "tree depth widened",
            ),
            (
                self.shell_profile == requested.shell_profile,
                "shell profile changed",
            ),
        ] {
            if !ok {
                return Err(denied(why));
            }
        }
        match (&self.argv_prefix, &requested.argv_prefix) {
            (_, None) => {}
            (Some(g), Some(r)) if g.iter().all(|g| r.iter().any(|r| g.starts_with(r))) => {}
            _ => return Err(denied("command prefixes widened")),
        }
        Ok(())
    }
}
