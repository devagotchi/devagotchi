use super::*;

fn violation(rule: &str, file: &str, line: Option<usize>) -> Violation {
    Violation {
        rule: rule.to_owned(),
        category: "layout".to_owned(),
        severity: "error".to_owned(),
        file: file.to_owned(),
        line,
        statement: "A new file holds at most 500 lines.".to_owned(),
        why: "Small files are read whole & reviewed.".to_owned(),
        expected: "x.rs holds at most 500 lines".to_owned(),
        fix: "Split <x.rs> by \"subject\"".to_owned(),
        example: "engine/hub/src/settings".to_owned(),
        subject: file.to_owned(),
    }
}

#[test]
fn the_xml_has_the_agreed_shape_and_is_escaped() {
    let report = Report {
        rules: 2,
        violations: vec![violation("layout.file-size-cap", "a&b.rs", Some(501))],
    };
    assert_eq!(
        report.xml(),
        "<bebop-report rules=\"2\" violations=\"1\">\n\
         \x20 <violation rule=\"layout.file-size-cap\" category=\"layout\" severity=\"error\" file=\"a&amp;b.rs\" line=\"501\">\n\
         \x20   <statement>A new file holds at most 500 lines.</statement>\n\
         \x20   <why>Small files are read whole &amp; reviewed.</why>\n\
         \x20   <expected>x.rs holds at most 500 lines</expected>\n\
         \x20   <fix>Split &lt;x.rs&gt; by &quot;subject&quot;</fix>\n\
         \x20   <example path=\"engine/hub/src/settings\"/>\n\
         \x20 </violation>\n\
         </bebop-report>\n"
    );
}

#[test]
fn a_finding_with_no_line_has_no_line_attribute() {
    let report = Report {
        rules: 1,
        violations: vec![violation("naming.layer-folders", "interfaces", None)],
    };
    assert!(report.xml().contains("file=\"interfaces\">"));
    assert!(!report.xml().contains("line="));
}

#[test]
fn a_clean_report_says_so_in_both_forms() {
    let report = Report {
        rules: 8,
        violations: Vec::new(),
    };
    assert_eq!(
        report.xml(),
        "<bebop-report rules=\"8\" violations=\"0\">\n</bebop-report>\n"
    );
    assert_eq!(report.text(), "bebop: 8 rules checked, no violations.\n");
}

#[test]
fn the_text_groups_by_rule_and_carries_the_same_content() {
    let report = Report {
        rules: 2,
        violations: vec![
            violation("layout.file-size-cap", "a.rs", Some(501)),
            violation("layout.file-size-cap", "b.rs", None),
            violation("naming.layer-folders", "interfaces", None),
        ],
    };
    let text = report.text();
    assert!(
        text.starts_with("bebop: 2 rules checked, 3 violations.\n"),
        "{text}"
    );
    assert!(text.contains("\nlayout.file-size-cap (layout, error): 2 violations\n"));
    assert!(text.contains("\nnaming.layer-folders (layout, error): 1 violation\n"));
    assert!(text.contains("  - a.rs:501\n"));
    assert!(text.contains("      Fix: Split <x.rs> by \"subject\"\n"));
    assert!(text.contains("  Why: Small files are read whole & reviewed.\n"));
    assert!(text.contains("  Example: engine/hub/src/settings\n"));
}

#[test]
fn characters_xml_cannot_hold_are_replaced() {
    assert_eq!(escape("a\u{0}b'c"), "a\u{FFFD}b&apos;c");
}
