use super::*;

fn check() -> Check {
    let own = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    Check::DomainRoleFiles {
        roots: own(&["src"]),
        allowed_files: own(&["mod.rs", "model.rs", "store.rs", "tests.rs"]),
        allowed_dirs: own(&["web", "tests"]),
    }
}

#[test]
fn a_domain_holds_only_its_role_files_and_folders() {
    let files = [
        ("src/lib.rs", ""),
        ("src/settings/mod.rs", ""),
        ("src/settings/store.rs", ""),
        ("src/settings/web/admin.rs", ""),
        ("src/settings/tests/admin.rs", ""),
        ("src/settings/hub.css", ""),
        ("src/machines/mod.rs", ""),
        ("src/machines/routes.rs", ""),
        ("src/machines/access/mod.rs", ""),
        ("src/machines/access/model.rs", ""),
        ("src/machines/access/grants.rs", ""),
        ("src/machines/join/compose.yml", ""),
        ("src/site/model.rs", ""),
    ];
    assert_eq!(
        found(&check(), &files),
        [
            "src/machines/access/grants.rs",
            "src/machines/join",
            "src/machines/routes.rs",
            "src/site",
        ]
    );
    let file = finding(&check(), &files, "src/machines/routes.rs");
    assert_eq!(file.var("domain"), Some("src/machines"));
    assert!(file.var("remedy").unwrap().contains("`routes.rs`"));
    let folder = finding(&check(), &files, "src/site");
    assert!(
        folder.var("detail").unwrap().contains("without a `mod.rs`"),
        "{folder:?}"
    );
}
