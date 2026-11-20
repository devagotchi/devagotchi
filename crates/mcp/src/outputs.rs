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

/// Keywords whose value is one schema.
pub const ONE_SCHEMA: [&str; 11] = [
    "items",
    "additionalProperties",
    "additionalItems",
    "not",
    "if",
    "then",
    "else",
    "contains",
    "propertyNames",
    "unevaluatedItems",
    "unevaluatedProperties",
];
/// Keywords whose value is a list of schemas.
pub const SCHEMA_LISTS: [&str; 4] = ["anyOf", "allOf", "oneOf", "prefixItems"];
/// Keywords whose value maps names to schemas.
pub const NAMED_SCHEMAS: [&str; 4] = ["properties", "patternProperties", "$defs", "definitions"];

/// A boolean where a schema goes, as the object schema that means the same (`true` is `{}`,
/// `false` is `{"not": {}}`). schemars writes `true` for a `serde_json::Value` with no doc
/// comment (`Ticket.progress`), and an MCP client on the TypeScript SDK refuses the whole tool
/// list over one (B3: `outputSchema.properties.progress: Invalid input`).
fn as_object(schema: &mut Value) {
    match schema {
        Value::Bool(true) => *schema = Value::Object(Default::default()),
        Value::Bool(false) => {
            *schema = serde_json::json!({ "not": {} });
        }
        _ => {}
    }
}

/// `schema` and every schema inside it, without `additionalProperties: false`, and with no
/// boolean where a schema goes.
pub fn open(schema: &mut Value) {
    match schema {
        Value::Object(rules) => {
            if rules.get("additionalProperties") == Some(&Value::Bool(false)) {
                rules.remove("additionalProperties");
            }
            for (keyword, rule) in rules.iter_mut() {
                let keyword = keyword.as_str();
                if NAMED_SCHEMAS.contains(&keyword) {
                    // Maps of names to schemas: each value is a schema, the names are not.
                    if let Value::Object(named) = rule {
                        for each in named.values_mut() {
                            as_object(each);
                            open(each);
                        }
                    }
                } else if ONE_SCHEMA.contains(&keyword) {
                    as_object(rule);
                    open(rule);
                } else if SCHEMA_LISTS.contains(&keyword) {
                    if let Value::Array(schemas) = rule {
                        for each in schemas.iter_mut() {
                            as_object(each);
                            open(each);
                        }
                    }
                } else if !matches!(
                    keyword,
                    "enum" | "const" | "default" | "examples" | "required"
                ) {
                    // Not schemas, but they may hold some (`$defs` under another name).
                    open(rule);
                }
            }
        }
        Value::Array(schemas) => schemas.iter_mut().for_each(open),
        _ => {}
    }
}

/// Where `schema` holds a boolean in a schema's place, as `/`-joined paths; empty when it
/// holds none. A boolean `additionalProperties` is not counted: `false` is how an input is
/// closed, and clients take it.
pub fn boolean_schemas(schema: &Value) -> Vec<String> {
    fn walk(schema: &Value, path: &str, found: &mut Vec<String>) {
        let Value::Object(rules) = schema else { return };
        for (keyword, rule) in rules {
            let here = format!("{path}/{keyword}");
            let keyword = keyword.as_str();
            let mut check = |each: &Value, at: String, found: &mut Vec<String>| {
                if each.is_boolean() {
                    found.push(at);
                } else {
                    walk(each, &at, found);
                }
            };
            if NAMED_SCHEMAS.contains(&keyword) {
                if let Value::Object(named) = rule {
                    for (name, each) in named {
                        check(each, format!("{here}/{name}"), found);
                    }
                }
            } else if keyword == "additionalProperties" && rule.is_boolean() {
                // `false` closes an input (I1); every client takes a boolean here.
            } else if ONE_SCHEMA.contains(&keyword) {
                check(rule, here, found);
            } else if SCHEMA_LISTS.contains(&keyword) {
                if let Value::Array(schemas) = rule {
                    for (n, each) in schemas.iter().enumerate() {
                        check(each, format!("{here}/{n}"), found);
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(schema, "", &mut found);
    found
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::open;

    #[test]
    fn a_boolean_in_a_schemas_place_becomes_the_object_that_means_the_same() {
        let mut schema = json!({
            "type": "object",
            "properties": { "progress": true, "never": false, "list": { "type": "array", "items": true } },
            "anyOf": [true, { "type": "object" }],
            "uniqueItems": true,
        });
        assert_eq!(super::boolean_schemas(&schema).len(), 4, "{schema}");
        open(&mut schema);
        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "progress": {},
                    "never": { "not": {} },
                    "list": { "type": "array", "items": {} }
                },
                "anyOf": [{}, { "type": "object" }],
                "uniqueItems": true,
            })
        );
        assert!(super::boolean_schemas(&schema).is_empty());
    }

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
