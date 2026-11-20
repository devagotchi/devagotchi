//! Every hub tool follows `.ai/patterns/mcp-tools.md` (`check_tools`): no exceptions since B4.
#[test]
fn every_hub_tool_follows_the_pattern() {
    let broken = devagotchi_mcp::pattern::check_tools(&crate::mcp::tools());
    assert!(
        broken.is_empty(),
        "every hub tool follows .ai/patterns/mcp-tools.md:
  {}",
        broken.join("
  ")
    );
}
