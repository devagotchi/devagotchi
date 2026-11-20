//! Bounded browser drafts. Parsing never changes a board or silently drops a row.
use crate::agenthub::{
    ConfigurationState, SettingsEdit, Workflow,
    workflow::{Lane, State, StateKind, Transition},
};
use std::collections::BTreeMap;

pub(super) struct Fields(BTreeMap<String, String>);
impl Fields {
    pub fn new(pairs: Vec<(String, String)>) -> Result<Self, ()> {
        let mut fields = BTreeMap::new();
        for (key, value) in pairs {
            if fields.insert(key, value).is_some() {
                return Err(());
            }
        }
        Ok(Self(fields))
    }
    fn take(&mut self, key: &str) -> Result<String, ()> {
        self.0.remove(key).ok_or(())
    }
    fn number(&mut self, key: &str) -> Result<i64, ()> {
        self.take(key)?.parse().map_err(|_| ())
    }
    fn count(&mut self, key: &str) -> Result<usize, ()> {
        let count = usize::try_from(self.number(key)?).map_err(|_| ())?;
        if count > 512 { Err(()) } else { Ok(count) }
    }
    fn finish(self) -> Result<(), ()> {
        if self.0.is_empty() { Ok(()) } else { Err(()) }
    }
}

pub(super) const SETTINGS: [&str; 8] = [
    "lease_ttl",
    "lease_min_ttl",
    "lease_max_ttl",
    "max_lease_expiries",
    "wip_limit_default",
    "default_priority",
    "repo",
    "environments",
];
#[derive(Clone)]
pub(super) struct SettingsDraft {
    pub version: i64,
    pub values: BTreeMap<String, String>,
}
impl SettingsDraft {
    pub fn current(state: &ConfigurationState) -> Self {
        let s = &state.settings;
        let values = [
            seconds(s.lease_ttl_ms),
            seconds(s.lease_min_ttl_ms),
            seconds(s.lease_max_ttl_ms),
            s.max_lease_expiries.to_string(),
            s.wip_limit_default.to_string(),
            s.default_priority.to_string(),
            s.repo.clone().unwrap_or_default(),
            s.environments.join(", "),
        ];
        Self {
            version: state.settings_version,
            values: SETTINGS
                .into_iter()
                .zip(values)
                .map(|(k, v)| (k.into(), v))
                .collect(),
        }
    }
    pub fn parse(mut fields: Fields) -> Result<Self, ()> {
        let version = fields.number("version")?;
        let mut values = BTreeMap::new();
        for key in SETTINGS {
            values.insert(key.into(), fields.take(key)?);
        }
        fields.finish()?;
        if version < 1 {
            return Err(());
        }
        Ok(Self { version, values })
    }
    pub fn edit(&self) -> Result<SettingsEdit, ()> {
        let get = |key| self.values.get(key).ok_or(()).map(String::as_str);
        let integer = |key| get(key)?.trim().parse::<i64>().map_err(|_| ());
        Ok(SettingsEdit {
            lease_ttl_ms: milliseconds(get("lease_ttl")?)?,
            lease_min_ttl_ms: milliseconds(get("lease_min_ttl")?)?,
            lease_max_ttl_ms: milliseconds(get("lease_max_ttl")?)?,
            max_lease_expiries: integer("max_lease_expiries")?,
            wip_limit_default: integer("wip_limit_default")?,
            default_priority: integer("default_priority")?,
            repo: Some(get("repo")?.to_string()),
            environments: get("environments")?
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        })
    }
}

fn seconds(ms: i64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
fn milliseconds(value: &str) -> Result<i64, ()> {
    let (whole, fraction) = value.trim().split_once('.').unwrap_or((value.trim(), ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(());
    }
    let whole: i64 = whole.parse().map_err(|_| ())?;
    let fraction: i64 = format!("{fraction:0<3}").parse().map_err(|_| ())?;
    whole
        .checked_mul(1000)
        .and_then(|n| n.checked_add(fraction))
        .ok_or(())
}

#[derive(Clone)]
pub(super) struct WorkflowDraft {
    pub version: i64,
    pub workflow: Workflow,
}
impl WorkflowDraft {
    pub fn current(state: &ConfigurationState) -> Self {
        Self {
            version: state.workflow_version,
            workflow: state.workflow.clone(),
        }
    }
    pub fn parse(mut fields: Fields) -> Result<(Self, String), ()> {
        let version = fields.number("version")?;
        let action = fields.0.remove("action").unwrap_or_else(|| "save".into());
        let id = fields.take("id")?;
        let name = fields.take("name")?;
        let initial_state = fields.take("initial_state")?;
        let attention_state = fields.take("attention_state")?;
        let mut states = Vec::new();
        for i in 0..fields.count("states")? {
            let key = fields.take(&format!("state_{i}_key"))?;
            let label = fields.take(&format!("state_{i}_label"))?;
            let kind = match fields.take(&format!("state_{i}_kind"))?.as_str() {
                "queue" => StateKind::Queue,
                "active" => StateKind::Active,
                "review" => StateKind::Review,
                "blocked" => StateKind::Blocked,
                "done" => StateKind::Done,
                "attention" => StateKind::Attention,
                _ => return Err(()),
            };
            states.push(State { key, label, kind });
        }
        let mut lanes = Vec::new();
        for i in 0..fields.count("lanes")? {
            let queue = fields.take(&format!("lane_{i}_queue"))?;
            let active = fields.take(&format!("lane_{i}_active"))?;
            let complete_to = fields.take(&format!("lane_{i}_complete_to"))?;
            let release_to = fields.take(&format!("lane_{i}_release_to"))?;
            let distinct_from_previous = match fields.0.remove(&format!("lane_{i}_distinct")) {
                None => false,
                Some(v) if v == "true" => true,
                _ => return Err(()),
            };
            lanes.push(Lane {
                queue,
                active,
                complete_to,
                release_to,
                distinct_from_previous,
            });
        }
        let mut transitions = Vec::new();
        for i in 0..fields.count("transitions")? {
            transitions.push(Transition {
                from: fields.take(&format!("transition_{i}_from"))?,
                to: fields.take(&format!("transition_{i}_to"))?,
            });
        }
        fields.finish()?;
        if version < 1 {
            return Err(());
        }
        Ok((
            Self {
                version,
                workflow: Workflow {
                    id,
                    name,
                    initial_state,
                    attention_state,
                    states,
                    lanes,
                    transitions,
                },
            },
            action,
        ))
    }
    /// Draft-only controls. They never invoke an update, advance a version, or create an event.
    pub fn change_rows(&mut self, action: &str, new_state: &str) -> Result<(), ()> {
        let w = &mut self.workflow;
        match action {
            "add-state" if w.states.len() < 512 => {
                let mut n = w.states.len() + 1;
                while w.state(&format!("state_{n}")).is_some() {
                    n += 1;
                }
                w.states.push(State {
                    key: format!("state_{n}"),
                    label: new_state.into(),
                    kind: StateKind::Queue,
                });
            }
            "add-lane" if w.lanes.len() < 512 => w.lanes.push(Lane {
                queue: w.initial_state.clone(),
                active: w
                    .states
                    .iter()
                    .find(|s| s.kind == StateKind::Active)
                    .map(|s| s.key.clone())
                    .unwrap_or_default(),
                complete_to: w.initial_state.clone(),
                release_to: w.initial_state.clone(),
                distinct_from_previous: false,
            }),
            "add-transition" if w.transitions.len() < 512 => w.transitions.push(Transition {
                from: "*".into(),
                to: w.initial_state.clone(),
            }),
            _ => {
                let (kind, index) = action
                    .strip_prefix("remove-")
                    .and_then(|s| s.rsplit_once('-'))
                    .ok_or(())?;
                let i: usize = index.parse().map_err(|_| ())?;
                match kind {
                    "state" if i < w.states.len() => {
                        w.states.remove(i);
                    }
                    "lane" if i < w.lanes.len() => {
                        w.lanes.remove(i);
                    }
                    "transition" if i < w.transitions.len() => {
                        w.transitions.remove(i);
                    }
                    _ => return Err(()),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations_round_trip_without_float_rounding_or_overflow() {
        for n in [1, 999, 1000, 1001, 15_000, 900_000, i64::MAX] {
            assert_eq!(milliseconds(&seconds(n)), Ok(n));
        }
        for input in ["-1", "1.0001", "NaN", "1e30", "9223372036854776", "1..2"] {
            assert!(milliseconds(input).is_err());
        }
        assert!(
            Fields::new(vec![
                ("version".into(), "1".into()),
                ("version".into(), "2".into())
            ])
            .is_err()
        );
    }
}
