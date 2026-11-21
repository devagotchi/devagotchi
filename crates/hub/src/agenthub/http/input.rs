use super::{Failure, invalid};
use crate::AppState;
use axum::{
    Json,
    extract::{FromRequest, Request},
    http::{HeaderMap, StatusCode},
};
use devagotchi_contracts::BridgeError;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    pub project: Option<String>,
}

impl axum::extract::FromRequestParts<AppState> for Scope {
    type Rejection = Failure;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Failure> {
        axum::extract::Query::<Self>::from_request_parts(parts, state)
            .await
            .map(|query| query.0)
            .map_err(|_| invalid("query accepts one project name"))
    }
}

pub(super) fn encoded<T: serde::Serialize>(value: &T) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(|_| invalid("cannot encode request"))
}

pub(super) struct Body(pub Value);
impl FromRequest<AppState> for Body {
    type Rejection = Failure;
    async fn from_request(request: Request, state: &AppState) -> Result<Self, Failure> {
        Json::<Value>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error| {
                if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    Failure(BridgeError::new("too_large", "JSON body exceeds 1 MiB"))
                } else {
                    invalid("send a valid JSON object with Content-Type application/json")
                }
            })
    }
}

pub(super) fn decode<T: DeserializeOwned>(value: Value) -> Result<T, Failure> {
    serde_json::from_value(value).map_err(|error| invalid(format!("invalid request: {error}")))
}

pub(super) fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, Failure> {
    let mut values = headers.get_all(name).iter();
    let value = values.next();
    if values.next().is_some() {
        return Err(invalid(format!("supply {name} only once")));
    }
    value
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid(format!("invalid {name} header")))
        })
        .transpose()
}

fn set(value: &mut Value, name: &str, expected: Value) -> Result<(), Failure> {
    if let Some(given) = value.get(name)
        && given != &expected
    {
        return Err(invalid(format!(
            "{name} disagrees with the request path, query or header"
        )));
    }
    value[name] = expected;
    Ok(())
}

/// Resolve explicit transport fields before typed decoding and receipt hashing.
pub(super) fn normalize(
    mut value: Value,
    scope: Scope,
    id: Option<&str>,
    headers: &HeaderMap,
) -> Result<Value, Failure> {
    if !value.is_object() {
        return Err(invalid("JSON body must be an object"));
    }
    if let Some(project) = scope.project {
        set(&mut value, "project", project.into())?;
    }
    if let Some(id) = id {
        set(&mut value, "id", id.into())?;
    }
    if let Some(key) = header(headers, "idempotency-key")? {
        set(&mut value, "request_id", key.into())?;
    }
    Ok(value)
}

pub(super) fn version(value: &mut Value, headers: &HeaderMap) -> Result<(), Failure> {
    let Some(tag) = header(headers, "if-match")? else {
        return Ok(());
    };
    let number = tag
        .strip_prefix('"')
        .and_then(|tag| tag.strip_suffix('"'))
        .unwrap_or(tag);
    let version: i64 = number
        .parse()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            invalid("If-Match must contain one positive version, optionally in double quotes")
        })?;
    if let Some(given) = value.get("version")
        && given.as_i64() != Some(version)
    {
        return Err(Failure(BridgeError::new(
            "precondition_failed",
            "body version differs from If-Match",
        )));
    }
    value["version"] = version.into();
    Ok(())
}

pub(super) fn ttl(value: &mut Value) -> Result<(), Failure> {
    if let Some(seconds) = value
        .as_object_mut()
        .and_then(|body| body.remove("ttl_seconds"))
    {
        let milliseconds = seconds
            .as_i64()
            .and_then(|seconds| seconds.checked_mul(1000))
            .ok_or_else(|| invalid("ttl_seconds must be an integer that fits in milliseconds"))?;
        set(value, "ttl_ms", milliseconds.into())?;
    }
    Ok(())
}
