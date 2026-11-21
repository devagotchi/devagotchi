//! Stable, bounded mutation identity independent of JSON object insertion order.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct Request {
    pub(super) tenant: String,
    pub(super) principal: String,
    pub(super) key: String,
    pub(super) operation: String,
    pub(super) digest: String,
}

impl Request {
    /// Build before entering a retrying transaction. The trusted adapter supplies the
    /// canonical principal and includes resource ids in `body`; never use a caller seat
    /// as a substitute for credential identity. Keys are scoped across all operations.
    pub fn new<T: Serialize>(
        tenant: &str,
        principal: &str,
        key: &str,
        operation: &str,
        body: &T,
    ) -> BridgeResult<Self> {
        let valid =
            |s: &str, bound| !s.is_empty() && s.len() <= bound && !s.chars().any(char::is_control);
        if !crate::valid_tenant_id(tenant)
            || !valid(principal, 256)
            || !valid(key, 200)
            || !valid(operation, 128)
        {
            return Err(BridgeError::new(
                "invalid",
                "invalid request identity or key bounds",
            ));
        }
        let value = serde_json::to_value(body)
            .map_err(|_| BridgeError::new("invalid", "request cannot be encoded"))?;
        let mut bytes = Vec::new();
        canonical(&value, &mut bytes, 0)?;
        let mut digest = Sha256::new();
        digest.update(b"devagotchi-request-v1\0");
        digest.update(operation.as_bytes());
        digest.update(b"\0");
        digest.update(&bytes);
        Ok(Self {
            tenant: tenant.into(),
            principal: principal.into(),
            key: key.into(),
            operation: operation.into(),
            digest: hex::encode(digest.finalize()),
        })
    }
}

fn canonical(value: &Value, out: &mut Vec<u8>, depth: usize) -> BridgeResult<()> {
    if depth > 64 {
        return Err(BridgeError::new(
            "invalid",
            "request nesting exceeds its bound",
        ));
    }
    match value {
        Value::Object(fields) => {
            out.push(b'{');
            let mut keys: Vec<_> = fields.keys().collect();
            keys.sort();
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend(serde_json::to_vec(key).expect("JSON key"));
                out.push(b':');
                canonical(&fields[key], out, depth + 1)?;
            }
            out.push(b'}');
        }
        Value::Array(values) => {
            out.push(b'[');
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                canonical(value, out, depth + 1)?;
            }
            out.push(b']');
        }
        value => out.extend(serde_json::to_vec(value).expect("JSON value")),
    }
    if out.len() > 1024 * 1024 {
        return Err(BridgeError::new("invalid", "request exceeds its bound"));
    }
    Ok(())
}
