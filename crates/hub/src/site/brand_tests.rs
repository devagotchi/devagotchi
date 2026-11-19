//! No brand, domain or operator in the hub's code: every name a person or an AI reads comes
//! from the site. This walks `crates/hub/src` the way the compiler does, from `lib.rs`
//! through each `mod`, reads every string literal outside comments and `#[cfg(test)]`
//! items, and every file the hub serves or runs as it is (scripts, styles), and fails on a
//! brand literal. The program's own name in lowercase (`devagotchi`, its binary, paths and
//! service account) is not a brand; the wire names below cannot change without breaking
//! machines and clients already out there.

use std::path::{Path, PathBuf};

/// What may not appear: the default site's product, its domain, its operator.
const BRANDS: [&str; 3] = ["Devagotchi", "devagotchi.com", "AngryVibes"];

/// Wire names that carry the product's name and stay as they are, and why.
const WIRE: [(&str, &str); 2] = [
    // The seat header machines and AI clients already send (`agenthub/mcp/mod.rs`).
    ("X-Devagotchi-Seat", "an HTTP header on the wire"),
    // Where Windows machines that already joined keep the install.
    (
        "$env:LOCALAPPDATA 'Devagotchi'",
        "the install folder of joined Windows PCs",
    ),
];

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

use crate::source_tests::{scan, sources};

fn brand_in(text: &str) -> Option<&'static str> {
    let mut text = text.to_string();
    for (wire, _) in WIRE {
        text = text.replace(wire, "");
    }
    BRANDS.into_iter().find(|brand| text.contains(brand))
}

#[test]
fn no_brand_literal_is_left_in_the_hubs_code() {
    let root = src();
    let mut found = Vec::new();
    let mut reached = std::collections::BTreeSet::new();
    for (file, text, test_only) in sources() {
        reached.insert(file.clone());
        let scanned = scan(&text);
        if test_only {
            continue;
        }
        for literal in scanned.literals {
            if let Some(brand) = brand_in(&literal.text) {
                found.push(format!(
                    "{}:{}: {brand:?} in {:?}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    literal.line,
                    literal.text.chars().take(100).collect::<String>()
                ));
            }
        }
    }
    // Every Rust file is reached from lib.rs, so none hides from this.
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                assert!(
                    reached.contains(&path),
                    "{} is in no module",
                    path.display()
                );
            } else {
                // Served or run as it is: the stylesheets, the page scripts, the join scripts.
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for (n, line) in text.lines().enumerate() {
                    if let Some(brand) = brand_in(line) {
                        found.push(format!(
                            "{}:{}: {brand:?} in {line:?}",
                            path.strip_prefix(&root).unwrap_or(&path).display(),
                            n + 1
                        ));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "names belong in the site (crates/hub/site/default), not the code:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_scan_sees_what_it_should_and_skips_what_it_should() {
    let scanned = scan(
        r##"
// "Devagotchi" in a comment
/* "Devagotchi" in a block /* nested */ still */
const A: &str = "shown";
const B: &str = r#"raw "quoted""#;
const C: char = '"';
const N: char = '\n';
fn lifetime<'a>(x: &'a str) -> &'a str { x }
#[tool(description = "a description")]
fn tool() {}
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "x/tests.rs"]
mod more;
#[cfg(test)]
mod inline { const D: &str = "hidden"; }
#[cfg(test)]
pub const E: [&str; 1] = ["hidden too"];
mod real;
const F: &str = "after";
"##,
    );
    let texts: Vec<&str> = scanned.literals.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        ["shown", "raw \"quoted\"", "a description", "after"],
        "{texts:?}"
    );
    assert_eq!(
        scanned.modules,
        [
            ("tests".to_string(), true, None),
            ("more".to_string(), true, Some("x/tests.rs".to_string())),
            ("real".to_string(), false, None),
        ]
    );
    assert_eq!(brand_in("joined Devagotchi"), Some("Devagotchi"));
    assert_eq!(
        brand_in("https://app.devagotchi.com"),
        Some("devagotchi.com")
    );
    assert_eq!(brand_in("X-Devagotchi-Seat must be a name"), None);
    assert_eq!(brand_in("devagotchi login"), None);
}
