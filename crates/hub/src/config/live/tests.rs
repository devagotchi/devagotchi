use std::collections::BTreeMap;

use super::*;

/// The fields of `pub struct name` in the config's sources, without the `serde(skip)` ones:
/// the keys a config file may hold for that struct.
fn fields_of(name: &str) -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/config");
    let mut sources = String::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            sources.push_str(&std::fs::read_to_string(path).unwrap());
            sources.push('\n');
        }
    }
    let start = sources
        .find(&format!("pub struct {name} {{"))
        .unwrap_or_else(|| panic!("no pub struct {name} in src/config"));
    let body = &sources[start..];
    let body = &body[body.find('{').unwrap() + 1..body.find("\n}").unwrap()];
    let mut fields = Vec::new();
    let mut skipped = false;
    for line in body.lines().map(str::trim) {
        if line.starts_with("#[serde(skip") {
            skipped = true;
        } else if let Some(rest) = line.strip_prefix("pub ") {
            let field = rest.split(':').next().unwrap().trim().to_string();
            if !skipped {
                fields.push(field);
            }
            skipped = false;
        }
    }
    assert!(!fields.is_empty(), "{name} has no fields read");
    fields
}

/// Every key a config file may hold, walking into the split sections.
fn config_keys() -> Vec<String> {
    fn walk(prefix: &str, structure: &str, keys: &mut Vec<String>) {
        for field in fields_of(structure) {
            let key = if prefix.is_empty() {
                field
            } else {
                format!("{prefix}.{field}")
            };
            match SPLIT_SECTIONS.iter().find(|(section, _)| *section == key) {
                Some((_, inner)) => walk(&key, inner, keys),
                None => keys.push(key),
            }
        }
    }
    let mut keys = Vec::new();
    walk("", "HubConfig", &mut keys);
    keys
}

#[test]
fn every_config_key_has_a_page_or_is_listed_as_set_at_deploy() {
    let keys = config_keys();
    for key in [
        "listen",
        "tenancy.unsecured.idle_days",
        "machines.auto_update",
    ] {
        assert!(keys.iter().any(|k| k == key), "{key} not read: {keys:?}");
    }
    let on_page = |key: &str| SETTINGS.iter().any(|setting| setting.key == key);
    let at_deploy = |key: &str| DEPLOY_TIME.iter().any(|(listed, _)| *listed == key);
    let mut missing = Vec::new();
    for key in &keys {
        match (on_page(key), at_deploy(key)) {
            (true, false) | (false, true) => {}
            (true, true) => panic!("{key} is both on /admin and listed as set at deploy"),
            (false, false) => missing.push(key.clone()),
        }
    }
    assert!(
        missing.is_empty(),
        "config keys with neither a setting on /admin (config/live.rs SETTINGS) nor a line in DEPLOY_TIME saying why not: {missing:?}"
    );
    // And nothing listed that the config does not have.
    for setting in SETTINGS {
        assert!(
            keys.contains(&setting.key.to_string()) || PAGE_ONLY.contains(&setting.key),
            "{} is on /admin but not a config key nor PAGE_ONLY",
            setting.key
        );
    }
    for (listed, why) in DEPLOY_TIME {
        assert!(
            keys.iter().any(|k| k == listed),
            "{listed} ({why}) is not a config key"
        );
        assert!(!why.is_empty());
    }
}

#[test]
fn a_setting_takes_only_its_own_kind_inside_its_range() {
    let starts = Setting::find("tenancy.starts_per_hour").unwrap();
    assert_eq!(starts.parse("12"), Ok(Value::Number(12)));
    assert!(starts.parse("0").is_err());
    assert!(starts.parse("10001").is_err());
    assert!(starts.parse("twelve").is_err());
    let switch = Setting::find("machines.auto_update").unwrap();
    assert_eq!(switch.parse("on"), Ok(Value::Switch(true)));
    // An unchecked checkbox sends nothing.
    assert_eq!(switch.parse(""), Ok(Value::Switch(false)));
    assert!(switch.parse("maybe").is_err());
    let password = Setting::find("passwords.min_length").unwrap();
    assert!(password.parse("7").is_err(), "never fewer than 8");
    assert_eq!(password.parse("15"), Ok(Value::Number(15)));
    for setting in SETTINGS {
        let defaults = Live::default().get();
        let value = defaults
            .get(setting.key)
            .unwrap_or_else(|| panic!("{} has no value", setting.key));
        assert_eq!(
            setting.parse(&value.text()),
            Ok(value),
            "{}'s default is in its own range",
            setting.key
        );
        let mut changed = defaults;
        assert!(
            changed.set(setting.key, value),
            "{} can be set",
            setting.key
        );
    }
}

#[test]
fn saved_values_apply_over_the_file_unless_the_file_or_environment_pinned_them() {
    let raw = serde_json::json!({"tenancy": {"starts_per_hour": 3}, "machines": {}});
    let env = |name: &str| (name == crate::config::analytics::RETENTION_ENV).then(|| "30".into());
    let pinned = Pinned::from(&raw, &env);
    assert!(pinned.contains("tenancy.starts_per_hour"));
    assert!(pinned.contains("analytics.retention_days"));
    assert!(
        !pinned.contains("machines.auto_update"),
        "an empty section pins nothing"
    );
    let saved: BTreeMap<String, String> = [
        ("tenancy.starts_per_hour", "50"),
        ("analytics.retention_days", "7"),
        ("machines.auto_update", "off"),
        ("passwords.min_length", "16"),
        ("tenancy.unsecured.idle_days", "900"),
        ("no.such.key", "1"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let base = Live::default().get();
    let live = base.with_saved(&saved, &pinned);
    assert_eq!(
        live.starts_per_hour, base.starts_per_hour,
        "pinned by the file"
    );
    assert_eq!(
        live.retention_days, base.retention_days,
        "pinned by the environment"
    );
    assert!(!live.auto_update);
    assert_eq!(live.password_min, 16);
    assert_eq!(
        live.unsecured.idle_days, base.unsecured.idle_days,
        "out of range: left out"
    );
}

#[test]
fn every_holder_of_the_handle_sees_a_save_at_once() {
    let live = Live::default();
    let held = live.clone();
    let mut settings = live.get();
    settings.password_min = 12;
    live.replace(settings);
    assert_eq!(held.get().password_min, 12);
}
