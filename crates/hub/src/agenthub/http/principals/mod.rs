//! Human administration adapters; identity owns every mutation and its receipt.
use super::{Body, Caller, Failure, Reply, input, invalid, response::answer};
use crate::{
    AppState,
    identity::accounts::{Accounts, agents::AgentEdit, receipts::MutationResult},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::{delete, get},
};
use devagotchi_contracts::BridgeError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

mod configuration;
mod tokens;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    account: Option<String>,
    after: Option<String>,
    role: Option<String>,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/principals", get(list).post(create))
        .route("/api/v1/principals/{id}", get(detail).put(update))
        .route(
            "/api/v1/principals/{id}/tokens",
            get(tokens::list).post(tokens::create),
        )
        .route("/api/v1/tokens/{id}", delete(tokens::revoke))
        .merge(configuration::routes())
}

fn person(state: &AppState, caller: &Caller, change: bool) -> Result<Arc<Accounts>, Failure> {
    if let Some(error) = caller.hub.waiting_refusal(&caller.subject) {
        return Err(error.into());
    }
    if matches!(caller.subject.via, "hub" | "key")
        || caller.subject.agent.is_some()
        || caller
            .headers
            .contains_key(crate::agenthub::mcp::SEAT_HEADER)
    {
        return Err(
            BridgeError::new("humans_only", "principal administration requires a person").into(),
        );
    }
    if change {
        let origin = input::header(&caller.headers, "origin")?;
        let site = input::header(&caller.headers, "sec-fetch-site")?;
        if origin.is_some_and(|origin| origin != state.hub.config.public_url.trim_end_matches('/'))
            || site.is_some_and(|site| site != "same-origin")
        {
            return Err(BridgeError::new("not_granted", "use this hub's own origin").into());
        }
    }
    state
        .passkeys
        .as_ref()
        .map(|sign_in| sign_in.accounts().clone())
        .ok_or_else(|| {
            BridgeError::new("not_offered", "account administration is unavailable").into()
        })
}

fn target<'a>(caller: &'a Caller, scope: &'a Scope) -> &'a str {
    scope.account.as_deref().unwrap_or(&caller.subject.id)
}

fn mutation<T: Serialize>(result: MutationResult<T>) -> Reply {
    Ok(answer(
        json!({"result":result.value,"replayed":result.replayed}),
        result.replayed,
    ))
}

fn prepared(
    value: Value,
    caller: &Caller,
    id: Option<&str>,
) -> Result<(Value, Option<String>), Failure> {
    let mut value = input::normalize(value, super::Scope::default(), id, &caller.headers)?;
    input::version(&mut value, &caller.headers)?;
    let key = value
        .as_object_mut()
        .unwrap()
        .remove("request_id")
        .map(|key| {
            key.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("request_id must be a string"))
        })
        .transpose()?;
    Ok((value, key))
}

async fn list(State(state): State<AppState>, caller: Caller, Query(scope): Query<Scope>) -> Reply {
    let accounts = person(&state, &caller, false)?;
    let mut agents = accounts
        .principal_roster(
            &caller.subject.id,
            scope.account.as_deref(),
            scope.after.as_deref(),
            scope.role.as_deref(),
        )
        .await?;
    let next = (agents.len() > 100).then(|| format!("{}/{}", agents[99].account, agents[99].id));
    agents.truncate(100);
    Ok(answer(json!({"principals":agents,"next":next}), false))
}

async fn detail(
    State(state): State<AppState>,
    caller: Caller,
    Query(scope): Query<Scope>,
    Path(id): Path<String>,
) -> Reply {
    let accounts = person(&state, &caller, false)?;
    let agent = accounts
        .agent(&caller.subject.id, target(&caller, &scope), &id)
        .await?;
    Ok(answer(json!(agent), false))
}

async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Query(scope): Query<Scope>,
    Body(value): Body,
) -> Reply {
    save(state, caller, scope, None, value).await
}

async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Query(scope): Query<Scope>,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    save(state, caller, scope, Some(id), value).await
}

async fn save(
    state: AppState,
    caller: Caller,
    scope: Scope,
    id: Option<String>,
    value: Value,
) -> Reply {
    let accounts = person(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, id.as_deref())?;
    let edit: AgentEdit = input::decode(value)?;
    if id.is_none() && edit.version != 0 || id.is_some() && edit.version <= 0 {
        return Err(invalid(
            "POST creates with version 0; PUT requires the current positive version",
        ));
    }
    mutation(
        accounts
            .put_agent_requested(
                &caller.subject.id,
                target(&caller, &scope),
                &edit,
                key.as_deref(),
            )
            .await?,
    )
}
