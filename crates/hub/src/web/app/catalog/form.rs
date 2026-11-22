use super::*;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct Draft {
    pub request_id: String,
    pub id: String,
    pub version: String,
    pub action: String,
    pub fields: BTreeMap<String, String>,
    pub is_default: bool,
    pub confirm_default: String,
}
impl Draft {
    pub fn new(kind: Kind) -> Self {
        let fields = kind
            .fields()
            .iter()
            .map(|f| {
                let value = match f.name {
                    "selector_schema" | "when_selector" | "operator_commands" => "{}",
                    "items" | "facts" | "argv_prefix" | "agent_argv" | "verify_argv" => "[]",
                    "duration" | "default_duration" => "task",
                    "os" => "any",
                    "severity" => "med",
                    "kind" => "direct",
                    _ if f.kind == FieldType::List => "[]",
                    _ => "",
                };
                (f.name.into(), value.into())
            })
            .collect();
        Self {
            request_id: requests::request_id(),
            id: String::new(),
            version: "0".into(),
            action: "save".into(),
            fields,
            is_default: false,
            confirm_default: String::new(),
        }
    }
    pub fn from_record(kind: Kind, row: Row) -> Self {
        let mut draft = Self::new(kind);
        draft.id = row.id;
        draft.version = row.version.to_string();
        draft.is_default = row.is_default == Some(true);
        for f in kind.fields() {
            let value = &row.data[f.name];
            let rendered = match f.kind {
                FieldType::Json | FieldType::List => {
                    serde_json::to_string_pretty(value).expect("JSON value")
                }
                FieldType::Bool => {
                    if value == true {
                        "true".into()
                    } else {
                        String::new()
                    }
                }
                _ => value.as_str().unwrap_or_default().into(),
            };
            draft.fields.insert(f.name.into(), rendered);
        }
        draft
    }
    pub fn submitted(kind: Kind, fields: Vec<(String, String)>) -> (Self, Option<BridgeError>) {
        let mut draft = Self::new(kind);
        draft.request_id.clear();
        draft.version.clear();
        let mut seen = std::collections::BTreeSet::new();
        let mut error = None;
        for (key, value) in fields {
            if !seen.insert(key.clone()) {
                error = Some(invalid("duplicate catalog form field"));
                continue;
            }
            match key.as_str() {
                "id" => draft.id = value,
                "version" => draft.version = value,
                "request_id" => draft.request_id = value,
                "action" => draft.action = value,
                "confirm_default" if kind == Kind::GrantPresets && value.len() <= 64 => {
                    draft.confirm_default = value
                }
                name if kind.fields().iter().any(|f| f.name == name) => {
                    draft.fields.insert(key, value);
                }
                _ => error = Some(invalid("unknown catalog form field")),
            }
        }
        if draft.request_id.is_empty() {
            error = Some(invalid("catalog form needs its original request key"));
        }
        (draft, error)
    }
    pub fn value(&self, kind: Kind) -> Result<Value, BridgeError> {
        let mut result = Map::new();
        for f in kind.fields() {
            let value = self
                .fields
                .get(f.name)
                .map(String::as_str)
                .unwrap_or_default();
            let value = match f.kind {
                FieldType::Json | FieldType::List => serde_json::from_str(value)
                    .map_err(|_| invalid("enter valid JSON in structured fields"))?,
                FieldType::Bool => match value {
                    "" => Value::Bool(false),
                    "true" => Value::Bool(true),
                    _ => return Err(invalid("invalid checkbox value")),
                },
                FieldType::Nullable if value.is_empty() => Value::Null,
                _ => Value::String(value.into()),
            };
            result.insert(f.name.into(), value);
        }
        Ok(Value::Object(result))
    }
}
