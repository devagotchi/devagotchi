//! Both actual routers must expose the same machine contract.
use serde_json::Value;

fn input(tool: &rmcp::model::Tool) -> Value {
    let mut value = Value::Object(tool.input_schema.as_ref().clone());
    value.as_object_mut().unwrap().remove("title");
    if let Some(properties) = value.get_mut("properties").and_then(Value::as_object_mut) { properties.remove("machine"); }
    if let Some(required) = value.get_mut("required").and_then(Value::as_array_mut) { required.retain(|p| p != "machine"); }
    if value["required"].as_array().is_some_and(Vec::is_empty) { value.as_object_mut().unwrap().remove("required"); }
    if value.get("properties").and_then(Value::as_object).is_some_and(serde_json::Map::is_empty) { value.as_object_mut().unwrap().remove("properties"); }
    value
}

#[test]
fn every_local_tool_has_the_same_contract_and_words_on_the_hub() {
    let local = devagotchi_mcp::local_tools();
    let hub = crate::mcp::tools();
    for tool in &local {
        let name = if tool.name == "broker_capabilities" { "machine_inspect" } else { tool.name.as_ref() };
        let remote = hub.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("missing hub tool {name}"));
        assert_eq!(tool.description, remote.description, "{name}: description");
        let mut a = tool.annotations.clone().unwrap();
        let mut b = remote.annotations.clone().unwrap();
        a.title = None; b.title = None;
        assert_eq!(serde_json::to_value(a).unwrap(), serde_json::to_value(b).unwrap(), "{name}: hints");
        assert_eq!(input(tool), input(remote), "{name}: input");
    }
    for (name, _) in devagotchi_contracts::describe::BROKER_TOOLS {
        assert!(local.iter().any(|t| t.name == *name), "missing local tool {name}");
    }
}
