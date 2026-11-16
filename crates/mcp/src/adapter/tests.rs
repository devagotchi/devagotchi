use super::*;

/// What an AI reads from the stdio adapter (its instructions, and every tool as
/// `tools/list` describes it) names no other product.
#[test]
fn the_adapter_names_no_other_product() {
    let adapter = BridgeAdapter::new(PathBuf::from("/nonexistent/broker.sock"));
    let mut texts = vec![adapter.get_info().instructions.unwrap_or_default()];
    for tool in (BridgeAdapter::tool_router()
        + BridgeAdapter::files_router()
        + BridgeAdapter::system_router()
        + BridgeAdapter::terminal_router())
    .list_all()
    {
        texts.push(serde_json::to_string(&tool).unwrap());
    }
    assert!(texts.len() > 1);
    for text in texts {
        let lower = text.to_lowercase();
        for name in ["desktop commander", "desktopcommander", "desktop-commander"] {
            assert!(!lower.contains(name), "{name} in {text}");
        }
    }
}
