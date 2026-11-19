use super::*;

/// What an AI reads from the stdio adapter (its instructions, and every tool as
/// `tools/list` describes it) names no other product.
#[test]
fn the_adapter_names_no_other_product() {
    let adapter = BridgeAdapter::new(PathBuf::from("/nonexistent/broker.sock"));
    let mut texts = vec![adapter.get_info().instructions.unwrap_or_default()];
    for tool in BridgeAdapter::router().list_all() {
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

/// An answer may grow (`crate::outputs`): no output schema the adapter advertises refuses a
/// field added later, and its arguments stay closed.
#[test]
fn every_output_schema_is_open_and_every_input_closed() {
    let tools = BridgeAdapter::router().list_all();
    assert!(tools.iter().any(|tool| tool.output_schema.is_some()));
    for tool in tools {
        if let Some(schema) = &tool.output_schema {
            let text = serde_json::to_string(schema.as_ref()).unwrap();
            assert!(
                !text.contains("\"additionalProperties\":false"),
                "{}: {text}",
                tool.name
            );
        }
        if tool.input_schema.get("properties").is_some() {
            assert_eq!(
                tool.input_schema["additionalProperties"], false,
                "{}: arguments stay closed",
                tool.name
            );
        }
    }
}

#[test]
fn every_local_tool_follows_the_pattern() {
    assert_eq!(
        crate::pattern::check_tools(&crate::local_tools()),
        Vec::<String>::new()
    );
}
