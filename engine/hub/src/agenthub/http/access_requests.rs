//! Request creation composes live identity, actual broker facts and the original task board.
use super::{
    Body, Caller, Reply,
    access_reads::{actor, identity},
    invalid,
    response::answer,
};
use crate::{AppState, identity::accounts::api_keys::Scope, machines::access::RequestInput};
use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use serde_json::json;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/v1/machines/{id}/access-capabilities",
        get(capabilities),
    )
}
async fn capabilities(
    State(state): State<AppState>,
    mut caller: Caller,
    Path(id): Path<String>,
) -> Reply {
    super::reads::read(&caller)?;
    caller.require_scope(Scope::MachinesRead)?;
    caller.access_history_scope(Some(&id))?;
    let (access, profile) = identity(&state, &caller).await?;
    let machine = access
        .request_machine(
            &caller.subject.id,
            &actor(&caller, profile.clone()),
            &caller.hub.stored_name(&caller.subject, &id),
        )
        .await?;
    let facts = access
        .broker_facts(
            &caller.subject.id,
            &actor(&caller, profile),
            &machine,
            &caller.hub.links,
        )
        .await?;
    caller.refresh(&state).await?;
    caller.require_scope(Scope::MachinesRead)?;
    caller.access_history_scope(Some(&id))?;
    identity(&state, &caller).await?;
    Ok(answer(json!({"machine":id,"capabilities":facts}), false))
}
pub(super) async fn create(
    State(state): State<AppState>,
    mut caller: Caller,
    Body(value): Body,
) -> Reply {
    caller.require_scope(Scope::ApprovalsRequest)?;
    let (value, key) = super::access_admin::prepared(value, &caller, None, false)?;
    let mut input: RequestInput =
        serde_json::from_value(value).map_err(|_| invalid("invalid access request fields"))?;
    let requested_name = input.machine.clone();
    let project = input.task.as_ref().map(|task| task.project.clone());
    caller.access_request_scope(&requested_name, project.as_deref())?;
    let (access, profile) = identity(&state, &caller).await?;
    if profile.is_none() {
        super::principals::person(&state, &caller, true)?;
    }
    input.machine = caller.hub.stored_name(&caller.subject, &input.machine);
    let machine = access
        .request_machine(
            &caller.subject.id,
            &actor(&caller, profile.clone()),
            &input.machine,
        )
        .await?;
    let facts = access
        .broker_facts(
            &caller.subject.id,
            &actor(&caller, profile),
            &machine,
            &caller.hub.links,
        )
        .await?;
    let board = if let Some(project) = project.as_ref() {
        let boards = caller.hub.boards.clone();
        let account = caller.subject.id.clone();
        let project = project.clone();
        Some(
            tokio::task::spawn_blocking(move || boards.open_account(&account, &project))
                .await
                .map_err(|_| invalid("task board resolution failed"))??
                .ok_or_else(|| invalid("task project is unavailable"))?,
        )
    } else {
        None
    };
    // Network and board resolution can wait; revalidate the original credential afterwards.
    caller.refresh(&state).await?;
    caller.require_scope(Scope::ApprovalsRequest)?;
    caller.access_request_scope(&requested_name, project.as_deref())?;
    let (_, profile) = identity(&state, &caller).await?;
    let result = access
        .request(
            &caller.subject.id,
            &actor(&caller, profile),
            board.as_ref().map(|b| b.hosted()),
            &machine,
            &facts.catalog(),
            &input,
            &key,
        )
        .await?;
    Ok(answer(
        json!({"result":result.result,"replayed":result.replayed,"machine_capabilities":facts}),
        result.replayed,
    ))
}
