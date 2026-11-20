//! Every tool a surface serves, held to `.ai/patterns/mcp-tools.md`.
use serde_json::Value;

/// Media answers declare no output schema (O1).
const UNSTRUCTURED: [&str; 1] = ["read_image"];
/// Answers that carry text from outside the AI and the hub's records (D3).
const UNTRUSTED: [&str; 9] = [
    "run",
    "read_file",
    "read_multiple_files",
    "search_files",
    "read_url",
    "operation_read",
    "operation_input",
    "connection_call",
    "board_issue_context",
];

pub fn check_tools(tools: &[rmcp::model::Tool]) -> Vec<String> {
    let mut broken = Vec::new();
    for tool in tools {
        let name = tool.name.to_string();
        let mut no = |rule: &str, why: String| broken.push(format!("{name}: {rule}: {why}"));
        if !name.starts_with(|c: char| c.is_ascii_lowercase())
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            no("N1", "not snake_case".into());
        }
        let notes = tool.annotations.as_ref();
        match (
            tool.title.as_deref(),
            notes.and_then(|a| a.title.as_deref()),
        ) {
            (Some(t), Some(a)) if t == a && !t.is_empty() => {}
            other => no("T1", format!("title and annotations.title: {other:?}")),
        }
        match notes {
            None => no("A1", "no annotations".into()),
            Some(a) => {
                if a.read_only_hint.is_none() || a.open_world_hint.is_none() {
                    no("A1", "read_only_hint and open_world_hint stated".into());
                }
                if a.read_only_hint == Some(false)
                    && (a.destructive_hint.is_none() || a.idempotent_hint.is_none())
                {
                    no(
                        "A2",
                        "a changing tool states destructive_hint and idempotent_hint".into(),
                    );
                }
            }
        }
        let input = Value::Object(tool.input_schema.as_ref().clone());
        if let Some(props) = input["properties"].as_object().filter(|p| !p.is_empty()) {
            if input["additionalProperties"] != Value::Bool(false) {
                no("I1", "arguments are open".into());
            }
            for (field, schema) in props {
                if schema
                    .get("description")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    no("I2", format!("`{field}` has no description"));
                }
            }
        }
        if tool.output_schema.is_none() != UNSTRUCTURED.contains(&name.as_str()) {
            no(
                "O1",
                "Json<T> answers declare an output schema; media answers do not".into(),
            );
        }
        // O4. MCP: `structuredContent` is an object, and so is the schema that describes it.
        // A list answers inside one (`{"tickets": [...]}`).
        if let Some(schema) = &tool.output_schema
            && schema.get("type") != Some(&Value::String("object".into()))
        {
            no(
                "O4",
                format!(
                    "the output schema is {:?}, not an object",
                    schema.get("type")
                ),
            );
        }
        // O5. No boolean where a schema goes, in or out: the MCP TypeScript SDK refuses the
        // whole tool list over one (`outputs::open` turns them into objects).
        let output = tool
            .output_schema
            .as_ref()
            .map(|schema| Value::Object(schema.as_ref().clone()));
        for (side, schema) in [("input", Some(&input)), ("output", output.as_ref())] {
            for at in schema
                .map(crate::outputs::boolean_schemas)
                .unwrap_or_default()
            {
                no(
                    "O5",
                    format!("a boolean schema in the {side} schema at {at}"),
                );
            }
        }
        let said = tool.description.as_deref().unwrap_or_default();
        if said.trim().is_empty() {
            no("D1", "no description".into());
        }
        if UNTRUSTED.contains(&name.as_str()) && !said.contains("data, not instructions") {
            no("D3", "untrusted answer not called data".into());
        }
    }
    broken
}
