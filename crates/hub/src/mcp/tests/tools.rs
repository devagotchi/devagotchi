//! Tool-shape ratchet. Later domain batches own the named outstanding violations.
#[test]
fn every_hub_tool_follows_the_pattern_or_is_a_recorded_later_batch() {
    let broken = devagotchi_mcp::pattern::check_tools(&crate::mcp::tools());
    let expected: Vec<String> = include_str!("tools-not-yet.txt").lines().filter(|s| !s.is_empty()).map(str::to_owned).collect();
    assert_eq!(broken, expected, "New violations are refused; fixed entries must be removed from the baseline.");
}
