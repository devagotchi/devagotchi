//! What a tool's answer promises, as `tools/list` advertises it: the fields it always writes,
//! never that it will write nothing more.
//!
//! The answer types are `deny_unknown_fields`, so schemars writes `additionalProperties:
//! false` into their output schemas. An MCP client checks `structuredContent` against the
//! schema it holds, and claude.ai holds a tool list long after it fetched it. The day an
//! answer gained a field (`FileContent.identity`), every `read_file` through a client still
//! holding the older list was refused ("must NOT have additional properties"). An answer may
//! grow; its advertised schema says so. Arguments keep their closed schemas: a misspelt
//! argument is still refused.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use serde_json::Value;

/// `router` with every tool's output schema open to fields added later.
pub fn open_outputs<S>(mut router: ToolRouter<S>) -> ToolRouter<S> {
    for route in router.map.values_mut() {
        if let Some(schema) = &route.attr.output_schema {
            let mut value = Value::Object(schema.as_ref().clone());
            open(&mut value);
            if let Value::Object(opened) = value {
                route.attr.output_schema = Some(Arc::new(opened));
            }
        }
    }
    router
}

/// `schema` and every schema inside it, without `additionalProperties: false`.
pub fn open(schema: &mut Value) {
    match schema {
        Value::Object(rules) => {
            if rules.get("additionalProperties") == Some(&Value::Bool(false)) {
                rules.remove("additionalProperties");
            }
            for (keyword, rule) in rules.iter_mut() {
                match keyword.as_str() {
                    // Maps of names to schemas: each value is a schema, the names are not.
                    "properties" | "patternProperties" | "$defs" | "definitions" => {
                        if let Value::Object(named) = rule {
                            named.values_mut().for_each(open);
                        }
                    }
                    // Values, not schemas.
                    "enum" | "const" | "default" | "examples" | "required" => {}
                    _ => open(rule),
                }
            }
        }
        Value::Array(schemas) => schemas.iter_mut().for_each(open),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::open;

    #[test]
    fn every_closed_object_opens_and_names_and_values_stay() {
        let mut schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "additionalProperties": { "type": "string" },
                "page": { "$ref": "#/$defs/Page" },
                "kinds": { "type": "array", "items": { "type": "object", "additionalProperties": false } },
                "either": { "anyOf": [{ "type": "object", "additionalProperties": false }, { "type": "null" }] }
            },
            "$defs": { "Page": { "type": "object", "additionalProperties": false, "properties": {} } },
            "default": { "additionalProperties": false },
            "required": ["page"]
        });
        open(&mut schema);
        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "additionalProperties": { "type": "string" },
                    "page": { "$ref": "#/$defs/Page" },
                    "kinds": { "type": "array", "items": { "type": "object" } },
                    "either": { "anyOf": [{ "type": "object" }, { "type": "null" }] }
                },
                "$defs": { "Page": { "type": "object", "properties": {} } },
                "default": { "additionalProperties": false },
                "required": ["page"]
            })
        );
    }

    #[test]
    fn an_open_schema_and_a_typed_map_stay_as_they_are() {
        let mut schema = json!({
            "type": "object",
            "additionalProperties": { "type": "integer" }
        });
        let before = schema.clone();
        open(&mut schema);
        assert_eq!(schema, before);
    }
}
