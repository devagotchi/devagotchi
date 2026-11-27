use super::*;

#[test]
fn a_crate_is_named_for_its_folder() {
    let check = Check::CrateNameMatchesFolder {
        prefix: "devagotchi-".to_owned(),
    };
    let files = [
        (
            "engine/hub/Cargo.toml",
            "[package]\nname = \"devagotchi-hub\"\n",
        ),
        (
            "runtime/devagotchi/Cargo.toml",
            "[package]\nname = \"devagotchi\"\n",
        ),
        ("Cargo.toml", "[workspace]\nmembers = []\n"),
    ];
    assert_eq!(found(&check, &files), ["runtime/devagotchi/Cargo.toml"]);
    let one = finding(&check, &files, "runtime/devagotchi/Cargo.toml");
    assert_eq!(one.var("expected_name"), Some("devagotchi-devagotchi"));
    assert_eq!(one.var("folder"), Some("runtime/devagotchi"));
}

#[test]
fn an_unreadable_manifest_is_an_error_not_a_pass() {
    let (_dir, tree) = tree(&[("engine/hub/Cargo.toml", "[package\n")]);
    let error = run(
        &Check::CrateNameMatchesFolder {
            prefix: "devagotchi-".to_owned(),
        },
        &tree,
        &scope(&["**/Cargo.toml"], &[]),
    )
    .unwrap_err();
    assert!(
        error.starts_with("engine/hub/Cargo.toml: not a readable Cargo.toml"),
        "{error}"
    );
}
