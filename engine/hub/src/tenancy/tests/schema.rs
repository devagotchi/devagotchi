//! Generate valid arguments so isolation tests reach real handlers.
use serde_json::{Value, json};

/// Arguments for `schema`, filled from `ids` by property name and with harmless values
/// elsewhere: enough for the call to reach the tool, with the other tenant's ids where a
/// tool takes one.
pub(crate) fn fill(schema: &Value, root: &Value, ids: &Value) -> Value {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap();
        let defs = root
            .get("$defs")
            .or_else(|| root.get("definitions"))
            .unwrap();
        return fill(&defs[name], root, ids);
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(options) = schema.get(key).and_then(Value::as_array) {
            let first = options
                .iter()
                .find(|o| o["type"] != "null")
                .unwrap_or(&options[0]);
            return fill(first, root, ids);
        }
    }
    if let Some(value) = schema.get("const") {
        return value.clone();
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
        return choices[0].clone();
    }
    let kind = match &schema["type"] {
        Value::Array(kinds) => kinds
            .iter()
            .find(|k| *k != "null")
            .cloned()
            .unwrap_or(Value::Null),
        kind => kind.clone(),
    };
    match kind.as_str() {
        Some("object") => {
            let mut out = serde_json::Map::new();
            let required: Vec<&str> = schema["required"]
                .as_array()
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if let Some(properties) = schema["properties"].as_object() {
                for (name, property) in properties {
                    if let Some(id) = ids.get(name) {
                        out.insert(name.clone(), id.clone());
                    } else if required.contains(&name.as_str()) {
                        out.insert(name.clone(), fill(property, root, ids));
                    }
                }
            }
            Value::Object(out)
        }
        Some("integer") | Some("number") => json!(1),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        _ => json!("x"),
    }
}
