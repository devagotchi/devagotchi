//! Versioned board and owner-catalog configuration, backed by AgentHub's transactions.
use super::{Body, Caller, Failure, Reply, Scope, input, invalid};
use crate::{
    AppState,
    agenthub::{SettingsEdit, Workflow, mcp::Kind},
    tenancy::orgs::Role,
};
use axum::{
    Router,
    extract::Path,
    routing::{get, post},
};
use devagotchi_contracts::BridgeError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod local;
mod workflows;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/workflows",
            get(workflows::list).post(workflows::create),
        )
        .route(
            "/api/v1/workflows/{id}",
            axum::routing::put(workflows::update).delete(workflows::delete),
        )
        .route("/api/v1/projects/{project}/configuration", get(local::read))
        .route(
            "/api/v1/projects/{project}/settings",
            axum::routing::put(local::settings),
        )
        .route(
            "/api/v1/projects/{project}/workflow",
            axum::routing::put(local::workflow),
        )
        .route(
            "/api/v1/projects/{project}/workflow/bind",
            post(local::bind),
        )
        .route(
            "/api/v1/projects/{project}/workflow/detach",
            post(local::detach),
        )
}

fn owner(caller: &Caller, project: &str) -> Result<(), Failure> {
    if matches!(caller.subject.via, "hub" | "key")
        || caller.subject.agent.is_some()
        || caller
            .headers
            .contains_key(crate::agenthub::mcp::SEAT_HEADER)
    {
        return Err(BridgeError::new("humans_only", "configuration requires a person").into());
    }
    let (_, kind, role) = caller.hub.board_access(&caller.subject, project)?;
    if kind != Kind::Account || role != Role::Owner {
        return Err(BridgeError::new(
            "not_owner",
            "this API requires the complete account board namespace",
        )
        .into());
    }
    Ok(())
}

fn project(scope: Scope) -> Result<String, Failure> {
    scope.project.ok_or_else(|| invalid("project is required"))
}

fn prepare(value: Value, scope: Scope, caller: &Caller) -> Result<Value, Failure> {
    let mut value = input::normalize(value, scope, None, &caller.headers)?;
    input::version(&mut value, &caller.headers)?;
    Ok(value)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowInput {
    project: String,
    request_id: Option<String>,
    version: i64,
    workflow: Workflow,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionInput {
    project: String,
    request_id: Option<String>,
    version: i64,
}

fn positive(version: i64) -> Result<(), Failure> {
    if version <= 0 {
        return Err(invalid("supply the current positive configuration version"));
    }
    Ok(())
}
