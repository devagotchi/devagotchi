//! The engines that read names: of files, of folders, their case, and the top level.

use super::*;
use crate::{engines::folder_case::kebab, rules::Case};

#[test]
fn a_forbidden_file_name_is_found_by_its_name_alone() {
    let check = Check::ForbiddenFileName {
        patterns: vec!["*_tests.rs".to_owned()],
    };
    let files = [
        ("site/assets_tests.rs", ""),
        ("site/tests.rs", ""),
        ("site/tests/assets.rs", ""),
        ("site_tests/mod.rs", ""),
    ];
    assert_eq!(found(&check, &files), ["site/assets_tests.rs"]);
    let one = finding(&check, &files, "site/assets_tests.rs");
    assert_eq!(one.var("pattern"), Some("*_tests.rs"));
}

#[test]
fn a_forbidden_folder_name_is_found_at_any_depth() {
    let check = Check::ForbiddenFolderName {
        names: vec!["utils".to_owned(), "shared".to_owned()],
    };
    let files = [
        ("utils/a.rs", ""),
        ("web/configuration/shared/b.rs", ""),
        ("web/shared-pages/c.rs", ""),
        ("web/utils.rs", ""),
    ];
    assert_eq!(found(&check, &files), ["utils", "web/configuration/shared"]);
}

#[test]
fn a_folder_not_in_kebab_case_is_told_its_kebab_name() {
    let check = Check::FolderCase { case: Case::Kebab };
    let files = [
        ("machine-broker/a.rs", ""),
        ("machine_engine/b.rs", ""),
        ("Hub/c.rs", ""),
        ("v2-api/d.rs", ""),
    ];
    assert_eq!(found(&check, &files), ["Hub", "machine_engine"]);
    assert_eq!(
        finding(&check, &files, "machine_engine").var("suggestion"),
        Some("machine-engine")
    );
    assert_eq!(kebab("MachineEngine"), "machine-engine");
    assert_eq!(kebab("local IPC"), "local-ipc");
}

#[test]
fn only_the_agreed_folders_sit_at_the_top() {
    let check = Check::LayerFolders {
        allowed: vec![
            "engine".to_owned(),
            "interface".to_owned(),
            "docs".to_owned(),
        ],
    };
    let files = [
        ("engine/a.rs", ""),
        ("interfaces/mcp/b.rs", ""),
        ("crates/c.rs", ""),
        ("docs/d.md", ""),
        ("README.md", ""),
    ];
    assert_eq!(found(&check, &files), ["crates", "interfaces"]);
    let plural = finding(&check, &files, "interfaces");
    assert!(
        plural
            .var("suggestion")
            .unwrap()
            .contains("to `interface/`"),
        "{plural:?}"
    );
    let other = finding(&check, &files, "crates");
    assert!(
        other
            .var("suggestion")
            .unwrap()
            .starts_with("move what `crates/`")
    );
}
