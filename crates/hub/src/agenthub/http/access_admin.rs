//! Human access actions share the domain owners' CAS, receipts and atomic effects.
use super::{
    Body, Caller, Failure, Reply, input, invalid, machine_access::owner, response::answer,
};
use crate::{
    AppState,
    machines::access::{Changed, ControlChange, PolicyEdit, ResumeAction, ResumeReason},
};
use axum::{
    Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/sandbox-policies", get(policies).put(set_policy))
        .route("/api/v1/grants/{id}/revoke", post(revoke))
        .route("/api/v1/resume-alerts", get(alerts))
        .route("/api/v1/resume-alerts/check", post(check))
        .route("/api/v1/resume-alerts/{id}/{action}", post(acknowledge))
        .route(
            "/api/v1/machines/{id}/access-control",
            get(machine).post(control),
        )
}
fn changed<T: Serialize>(result: Changed<T>) -> Reply {
    let replayed = result.replayed;
    Ok(answer(json!(result), replayed))
}
fn prepared(
    mut value: Value,
    caller: &Caller,
    id: Option<&str>,
    version: bool,
) -> Result<(Value, String), Failure> {
    value = input::normalize(value, input::Scope::default(), id, &caller.headers)?;
    if version {
        input::version(&mut value, &caller.headers)?;
    } else if caller.headers.contains_key("if-match") {
        return Err(invalid("this operation does not accept If-Match"));
    }
    let body = value.as_object_mut().unwrap();
    if id.is_some() {
        body.remove("id");
    }
    let key = body
        .remove("request_id")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(|| invalid("access changes need request_id or Idempotency-Key"))?;
    Ok((value, key))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<String>,
    limit: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineTarget {
    name: String,
    generation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    version: i64,
}
async fn policies(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<Page>, QueryRejection>,
) -> Reply {
    super::reads::read(&caller)?;
    let Query(page) = query.map_err(|_| invalid("invalid policy page"))?;
    Ok(answer(
        json!(
            owner(&state, &caller, false)?
                .sandbox_policies(
                    &caller.subject.id,
                    page.after.as_deref(),
                    page.limit.unwrap_or(100)
                )
                .await?
        ),
        false,
    ))
}
async fn set_policy(State(state): State<AppState>, caller: Caller, Body(value): Body) -> Reply {
    let access = owner(&state, &caller, true)?;
    let (mut value, key) = prepared(value, &caller, None, true)?;
    let target: Option<MachineTarget> = input::decode(
        value
            .as_object_mut()
            .unwrap()
            .remove("machine")
            .unwrap_or(Value::Null),
    )?;
    let edit: PolicyEdit = input::decode(value)?;
    let machine = match target {
        Some(target) => Some(
            access
                .machine_administration(&caller.subject.id, &target.name, Some(&target.generation))
                .await?
                .binding,
        ),
        None => None,
    };
    changed(
        access
            .set_policy(&caller.subject.id, machine.as_ref(), &edit, &key)
            .await?,
    )
}
async fn revoke(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    let access = owner(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, Some(&id), true)?;
    let input: Version = input::decode(value)?;
    changed(
        access
            .revoke(&caller.subject.id, &id, input.version, &key)
            .await?,
    )
}
async fn alerts(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<Page>, QueryRejection>,
) -> Reply {
    super::reads::read(&caller)?;
    let Query(page) = query.map_err(|_| invalid("invalid resume alert page"))?;
    Ok(answer(
        json!(
            owner(&state, &caller, false)?
                .resume_alerts(
                    &caller.subject.id,
                    page.after.as_deref(),
                    page.limit.unwrap_or(100)
                )
                .await?
        ),
        false,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    ui_session_id: String,
    reason: ResumeReason,
    after: Option<String>,
    limit: Option<u32>,
}
async fn check(State(state): State<AppState>, caller: Caller, Body(value): Body) -> Reply {
    let access = owner(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, None, false)?;
    let input: Check = input::decode(value)?;
    changed(
        access
            .check_resume(
                &caller.subject.id,
                &input.ui_session_id,
                input.reason,
                input.after.as_deref(),
                input.limit.unwrap_or(100),
                &key,
            )
            .await?,
    )
}
async fn acknowledge(
    State(state): State<AppState>,
    caller: Caller,
    Path((id, action)): Path<(String, String)>,
    Body(value): Body,
) -> Reply {
    let access = owner(&state, &caller, true)?;
    let action: ResumeAction = input::decode(json!(action))?;
    let (value, key) = prepared(value, &caller, Some(&id), true)?;
    let input: Version = input::decode(value)?;
    changed(
        access
            .acknowledge_resume(&caller.subject.id, &id, input.version, action, &key)
            .await?,
    )
}
async fn machine(State(state): State<AppState>, caller: Caller, Path(id): Path<String>) -> Reply {
    super::reads::read(&caller)?;
    Ok(answer(
        json!(
            owner(&state, &caller, false)?
                .machine_administration(&caller.subject.id, &id, None)
                .await?
        ),
        false,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    generation: String,
    version: i64,
    action: ControlChange,
}
async fn control(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    let access = owner(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, Some(&id), true)?;
    let input: Control = input::decode(value)?;
    let machine = access
        .machine_administration(&caller.subject.id, &id, Some(&input.generation))
        .await?
        .binding;
    changed(
        access
            .control_machine(
                &caller.subject.id,
                &machine,
                input.version,
                input.action,
                &key,
            )
            .await?,
    )
}
