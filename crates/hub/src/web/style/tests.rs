//! The stylesheet's rules (`.ai/dev/plans/standalone-build.md`, N9): one cascade order, the
//! primitives in `components`, and each page's sheet in `pages`, scoped to that page, with no
//! viewport breakpoints of its own.

use super::{PAGES, PRIMITIVES};

/// The selectors of every rule in `css`, with the at-rule preludes they sit under, comments
/// removed: `(prelude path, selector)`.
fn rules(css: &str) -> Vec<(Vec<String>, String)> {
    let mut text = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        text.push_str(&rest[..start]);
        rest = rest[start..]
            .split_once("*/")
            .map(|(_, after)| after)
            .unwrap_or("");
    }
    text.push_str(rest);
    let (mut out, mut path, mut prelude) = (Vec::new(), Vec::<String>::new(), String::new());
    let mut in_rule = false;
    for c in text.chars() {
        match c {
            '{' if !in_rule => {
                let head = prelude.trim().to_string();
                prelude.clear();
                if head.starts_with('@') {
                    path.push(head);
                } else {
                    out.push((path.clone(), head));
                    in_rule = true;
                }
            }
            '}' if in_rule => in_rule = false,
            '}' => {
                path.pop();
                prelude.clear();
            }
            ';' if !in_rule => prelude.clear(),
            _ if !in_rule => prelude.push(c),
            _ => {}
        }
    }
    out
}

#[test]
fn the_stylesheet_declares_the_cascade_order_first() {
    let site = crate::site::builtin();
    assert!(
        site.stylesheet()
            .starts_with("@layer reset, tokens, base, components, pages;\n"),
        "the served stylesheet starts with the layer order"
    );
}

#[test]
fn the_primitives_are_components_and_nothing_else() {
    let found = rules(PRIMITIVES);
    assert!(!found.is_empty());
    for (path, selector) in found {
        assert_eq!(
            path.first().map(String::as_str),
            Some("@layer components"),
            "{selector} sits in @layer components"
        );
        assert!(
            !path.iter().any(|p| p.starts_with("@media")),
            "{selector}: a primitive is tuned by a custom property, not a breakpoint"
        );
    }
}

#[test]
fn every_page_sheet_is_scoped_to_its_page_with_no_viewport_breakpoint() {
    for (page, css) in PAGES {
        let scope = format!("[data-page=\"{page}\"]");
        let found = rules(css);
        assert!(!found.is_empty(), "{page}: an empty sheet");
        for (path, selector) in found {
            assert_eq!(
                path.first().map(String::as_str),
                Some("@layer pages"),
                "{page}: {selector} sits in @layer pages"
            );
            assert!(
                !path.iter().any(|p| p.starts_with("@media")),
                "{page}: {selector}: a page sizes itself from the fluid scales and \
                 container queries, never a viewport breakpoint"
            );
            for part in selector.split(',') {
                assert!(
                    part.trim().starts_with(&scope),
                    "{page}: {} does not start with {scope}",
                    part.trim()
                );
            }
        }
    }
}

#[test]
fn every_scale_step_a_sheet_reads_is_a_token() {
    let site = crate::site::builtin();
    let served = site.stylesheet();
    let sheets = std::iter::once(PRIMITIVES).chain(PAGES.iter().map(|(_, css)| *css));
    for css in sheets {
        for (at, _) in css.match_indices("var(--") {
            let name: String = css[at + 4..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if name.starts_with("--step-") || name.starts_with("--space-") {
                assert!(
                    served.contains(&format!("{name}: clamp(")),
                    "{name} is read but tokens.json does not set it as a clamp()"
                );
            }
        }
    }
}
