//! The catalog is checked as code: every file on disk is compiled in, reads against the
//! schema, sits in the right folder, and renders into something a person can use.

use std::{collections::BTreeSet, fs, path::Path};

use super::*;

fn catalog_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog")
}

fn on_disk() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for vendor in fs::read_dir(catalog_dir()).unwrap() {
        let vendor = vendor.unwrap();
        if !vendor.path().is_dir() {
            continue;
        }
        for file in fs::read_dir(vendor.path()).unwrap() {
            let name = file.unwrap().file_name().into_string().unwrap();
            assert!(name.ends_with(".json"), "{name}: only connectors live here");
            found.insert(format!("{}/{name}", vendor.file_name().to_str().unwrap()));
        }
    }
    found
}

#[test]
fn every_file_on_disk_is_compiled_in_and_nothing_else() {
    let listed: BTreeSet<String> = catalog::FILES.iter().map(|(p, _)| p.to_string()).collect();
    assert_eq!(
        listed,
        on_disk(),
        "catalog.rs lists exactly what is in catalog/"
    );
    assert!(
        listed.len() >= 15,
        "the catalog covers the clients people use"
    );
}

#[test]
fn every_file_reads_and_sits_where_its_vendor_and_id_say() {
    for (path, text) in catalog::FILES {
        let connector: Connector =
            serde_json::from_str(text).unwrap_or_else(|error| panic!("{path}: {error}"));
        assert_eq!(connector.schema, SCHEMA, "{path}");
        assert_eq!(*path, format!("{}/{}.json", connector.vendor, connector.id));
        for name in [&connector.id, &connector.vendor] {
            assert!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{path}: {name:?}"
            );
        }
    }
}

#[test]
fn ids_are_unique() {
    let ids: BTreeSet<&str> = all().iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), all().len());
}

#[test]
fn every_entry_says_how_it_is_known() {
    for c in all() {
        let v = &c.verification;
        assert!(
            v.on.len() == 10 && v.on.as_bytes()[4] == b'-' && v.on.as_bytes()[7] == b'-',
            "{}: {}",
            c.id,
            v.on
        );
        assert!(!v.docs.is_empty(), "{}: names no source", c.id);
        for url in &v.docs {
            assert!(url.starts_with("https://"), "{}: {url}", c.id);
        }
        if v.level == Proof::Ci {
            assert!(
                v.test.is_some(),
                "{}: ci without the test that proves it",
                c.id
            );
        }
    }
}

#[test]
fn every_entry_tells_a_person_what_to_do() {
    for c in all() {
        let i = &c.install;
        let enough = match i.kind {
            InstallKind::CliCommand => i.command.is_some(),
            InstallKind::ConfigFile => i.snippet.is_some(),
            InstallKind::UiSteps => !i.ui_steps.is_empty(),
            InstallKind::ApiParameter => i.snippet.is_some() || !i.ui_steps.is_empty(),
        };
        assert!(enough, "{}: {:?} with nothing to do", c.id, i.kind);
        let wants_url = c.remote_mcp == Some(true);
        let rendered = c.render("https://hub.test/mcp");
        let text = format!(
            "{} {} {}",
            rendered.command.clone().unwrap_or_default(),
            rendered.snippet.clone().unwrap_or_default(),
            rendered.steps.join(" ")
        );
        if wants_url {
            assert!(
                text.contains("https://hub.test/mcp"),
                "{}: never names the hub",
                c.id
            );
        }
        assert!(
            !text.contains("{url}") && !text.contains("{name}"),
            "{}: a placeholder survived",
            c.id
        );
    }
}

#[test]
fn snippets_are_valid_in_the_format_they_claim() {
    for c in all() {
        let Some(snippet) = c.render("https://hub.test/mcp").snippet else {
            continue;
        };
        if c.install.format == Some(ConfigFormat::Json) {
            serde_json::from_str::<serde_json::Value>(&snippet)
                .unwrap_or_else(|error| panic!("{}: {error}\n{snippet}", c.id));
            if let Some(with_key) = c.render("https://hub.test/mcp").key_snippet {
                serde_json::from_str::<serde_json::Value>(&with_key)
                    .unwrap_or_else(|error| panic!("{}: {error}\n{with_key}", c.id));
            }
        }
    }
}

#[test]
fn every_client_has_a_name_a_note_can_reach() {
    for c in all() {
        assert!(!c.aliases.is_empty(), "{}", c.id);
        for alias in &c.aliases {
            assert_eq!(alias, &alias.to_ascii_lowercase(), "{}", c.id);
        }
    }
    assert!(aliases_for_client("Claude Code").contains(&"claude".to_string()));
    assert!(aliases_for_client("codex-cli").contains(&"codex".to_string()));
    assert!(aliases_for_client("nothing we know").is_empty());
    let code = aliases_for_client("Claude Code");
    assert!(code.contains(&"claude-code".to_string()));
    assert!(!code.contains(&"claude-desktop".to_string()));
    assert!(
        aliases_for_client("Authorized Client").is_empty(),
        "whole words only"
    );
}

/// The published schema is the one the types make. To refresh it after changing the types:
/// `DEVAGOTCHI_WRITE_SCHEMA=1 cargo test -p devagotchi-connectors`.
#[test]
fn the_published_schema_is_the_one_the_types_make() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/connector.v1.schema.json");
    let made = format!("{}\n", serde_json::to_string_pretty(&schema()).unwrap());
    if std::env::var_os("DEVAGOTCHI_WRITE_SCHEMA").is_some() {
        fs::write(&path, &made).unwrap();
    }
    let published = fs::read_to_string(&path)
        .expect("schemas/connector.v1.schema.json")
        .replace("\r\n", "\n");
    assert_eq!(published, made);
}
