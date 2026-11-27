use super::*;

#[test]
fn a_module_that_is_both_a_file_and_a_folder_of_rust_is_found() {
    let files = [
        ("src/broker.rs", ""),
        ("src/broker/system.rs", ""),
        ("src/files/mod.rs", ""),
        ("src/files/lines.rs", ""),
        ("src/assets.rs", ""),
        ("src/assets/app.css", ""),
        ("src/main.rs", ""),
        ("src/main/helper.rs", ""),
        ("src/broker/system/web.rs", ""),
        ("src/broker/system/web/page.rs", ""),
    ];
    assert_eq!(
        found(&Check::ModuleHome, &files),
        [
            "src/broker.rs",
            "src/broker/system.rs",
            "src/broker/system/web.rs"
        ]
    );
    let one = finding(&Check::ModuleHome, &files, "src/broker.rs");
    assert_eq!(one.var("module"), Some("broker"));
    assert_eq!(one.var("home"), Some("src/broker/mod.rs"));
}
