//! Access administration delegates validation, receipts and audit to the machine owner.
use super::{Body, Caller, Failure, Reply, input, invalid, response::answer};
use crate::{
    AppState,
    machines::access::{Access, AccessSettings},
};
use axum::{Router, extract::State, routing::get};
use serde_json::json;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/access-settings", get(settings).put(put_settings))
}
fn owner(state: &AppState, caller: &Caller, change: bool) -> Result<Access, Failure> {
    if caller.subject.org.is_some() {
        return Err(devagotchi_contracts::BridgeError::new(
            "not_granted",
            "access settings use this account's environment",
        )
        .into());
    }
    Ok(Access::new(super::principals::person(
        state, caller, change,
    )?))
}
async fn settings(State(state): State<AppState>, caller: Caller) -> Reply {
    super::reads::read(&caller)?;
    let value = owner(&state, &caller, false)?
        .settings(&caller.subject.id)
        .await?;
    Ok(answer(json!(value), false))
}
async fn put_settings(State(state): State<AppState>, caller: Caller, Body(value): Body) -> Reply {
    let access = owner(&state, &caller, true)?;
    let mut value = input::normalize(value, input::Scope::default(), None, &caller.headers)?;
    input::version(&mut value, &caller.headers)?;
    let key = value
        .as_object_mut()
        .unwrap()
        .remove("request_id")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(|| invalid("access settings changes need a request_id or Idempotency-Key"))?;
    let settings: AccessSettings =
        serde_json::from_value(value).map_err(|_| invalid("invalid access settings fields"))?;
    let changed = access
        .put_settings(&caller.subject.id, &settings, &key)
        .await?;
    let replayed = changed.replayed;
    Ok(answer(json!(changed), replayed))
}
