//! Offline JSON Schema validation. No network/file resolver features are linked.
use super::*;
pub(super) fn bounded(value: &Value) -> BridgeResult<()> {
    if serde_json::to_vec(value)
        .map_err(|_| invalid("invalid catalog JSON"))?
        .len()
        > 262_144
    {
        return Err(invalid("catalog record exceeds 256 KiB"));
    }
    fn depth(value: &Value, n: usize) -> bool {
        n <= 32
            && match value {
                Value::Array(values) => values.iter().all(|v| depth(v, n + 1)),
                Value::Object(values) => values.values().all(|v| depth(v, n + 1)),
                _ => true,
            }
    }
    if !depth(value, 0) {
        return Err(invalid("catalog JSON exceeds 32 nesting levels"));
    }
    Ok(())
}
pub(super) fn compile(value: &Value) -> BridgeResult<jsonschema::Validator> {
    bounded(value)?;
    if !value.is_object() {
        return Err(invalid("selector_schema must be an object"));
    }
    let empty = json!({"type":"object"});
    jsonschema::options()
        .offline()
        .build(if value == &json!({}) { &empty } else { value })
        .map_err(|_| invalid("invalid selector schema or unresolved external reference"))
}
pub(super) fn instance(schema: &Value, value: &Value) -> BridgeResult<()> {
    bounded(value)?;
    if !compile(schema)?.is_valid(value) {
        return Err(invalid("selector does not match its capability schema"));
    }
    Ok(())
}
