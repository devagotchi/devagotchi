//! Registration and audit ratchet over actual source, before routers can hide duplicates.
use std::collections::BTreeSet;

#[test]
fn tool_names_are_unique_and_every_body_reaches_its_audit_owner() {
    let mut names = BTreeSet::new();
    let mut broken = Vec::new();
    for (_, text, test_only) in crate::source_tests::sources() {
        if test_only {
            continue;
        }
        for block in text.split("#[tool(").skip(1) {
            let compact: String = block.chars().filter(|c| !c.is_whitespace()).collect();
            let name = compact
                .split("name=\"")
                .nth(1)
                .expect("tool name")
                .split('"')
                .next()
                .unwrap();
            assert!(names.insert(name.to_string()), "duplicate tool {name}");
            if ["get_recent_tool_calls", "get_usage_stats"].contains(&name) {
                continue;
            }
            let body = compact.split(")]").nth(1).expect("tool attribute closes");
            let audit = [
                "self.call(",
                "self.board_call",
                "self.recorded(",
                "self.record_completion(",
            ]
            .iter()
            .filter_map(|call| body.find(call))
            .min();
            if audit.is_none() {
                broken.push(format!("{name}: missing audit helper"));
            }
            // Async result blocks may use ? internally; their entire result must reach the
            // helper. Behaviour tests exercise refusals, not just the source spelling.
        }
    }
    let served: BTreeSet<_> = crate::mcp::tool_names().into_iter().collect();
    assert_eq!(names, served);
    broken.sort();
    let expected: Vec<_> = include_str!("audit-not-yet.txt")
        .lines()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    assert_eq!(
        broken, expected,
        "Only the recorded B3/B4 audit gaps remain; remove fixed entries."
    );
}
