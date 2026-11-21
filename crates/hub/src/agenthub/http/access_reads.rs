//! Access history uses the same canonical identity boundary as execution admission.
use super::{Caller, Failure, Reply, invalid, response::answer};
use crate::{
    AppState,
    identity::accounts::api_keys::Scope,
    machines::access::{Access, AccessPageQuery},
};
use axum::{
    Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    routing::get,
};
use devagotchi_agenthub::{Actor, policy::Identity};
use serde::Deserialize;
use serde_json::json;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/access-requests", get(requests))
        .route("/api/v1/access-requests/{id}", get(lookup))
        .route("/api/v1/grants", get(grants))
}
async fn scoped(state: &AppState, caller: &Caller) -> Result<(Access, Option<Identity>), Failure> {
    super::reads::read(caller)?;
    if let Some(error) = caller.hub.waiting_refusal(&caller.subject) {
        return Err(error.into());
    }
    caller.require_scope(Scope::MachinesRead)?;
    if caller
        .headers
        .contains_key(crate::agenthub::mcp::SEAT_HEADER)
    {
        return Err(invalid(
            "access history uses the credential-bound principal, not a seat header",
        ));
    }
    let accounts = state
        .passkeys
        .as_ref()
        .map(|sign_in| sign_in.accounts().clone())
        .ok_or_else(|| invalid("account access history is unavailable"))?;
    let profile = accounts.machine_identity(&caller.subject).await?;
    Ok((Access::new(accounts), profile))
}
fn actor(caller: &Caller, policy: Option<Identity>) -> Actor<'_> {
    Actor {
        id: caller
            .subject
            .agent
            .as_deref()
            .unwrap_or(&caller.subject.id),
        human: policy.is_none(),
        wip_limit: None,
        policy,
    }
}
async fn requests(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<AccessPageQuery>, QueryRejection>,
) -> Reply {
    let Query(query) = query.map_err(|_| invalid("invalid access request filters"))?;
    caller.access_history_scope(query.machine.as_deref())?;
    let (access, profile) = scoped(&state, &caller).await?;
    Ok(answer(
        json!(
            access
                .access_requests(&caller.subject.id, &actor(&caller, profile), &query)
                .await?
        ),
        false,
    ))
}
async fn grants(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<AccessPageQuery>, QueryRejection>,
) -> Reply {
    let Query(query) = query.map_err(|_| invalid("invalid grant filters"))?;
    caller.access_history_scope(query.machine.as_deref())?;
    let (access, profile) = scoped(&state, &caller).await?;
    Ok(answer(
        json!(
            access
                .access_grants(&caller.subject.id, &actor(&caller, profile), &query)
                .await?
        ),
        false,
    ))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wait {
    #[serde(default)]
    timeout_s: u32,
}
async fn lookup(
    State(state): State<AppState>,
    mut caller: Caller,
    Path(id): Path<String>,
    query: Result<Query<Wait>, QueryRejection>,
) -> Reply {
    let Query(query) = query.map_err(|_| invalid("lookup accepts timeout_s from 0 to 25"))?;
    if query.timeout_s > 25 {
        return Err(invalid("lookup timeout_s must be from 0 to 25"));
    }
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(u64::from(query.timeout_s));
    loop {
        let (access, profile) = scoped(&state, &caller).await?;
        let result = access
            .access_lookup(&caller.subject.id, &actor(&caller, profile), &id)
            .await?;
        for item in &result.items {
            caller.access_history_scope(Some(&item.machine))?;
        }
        if result.status != "pending" || tokio::time::Instant::now() >= deadline {
            return Ok(answer(json!(result), false));
        }
        tokio::time::sleep_until(
            deadline.min(tokio::time::Instant::now() + std::time::Duration::from_millis(400)),
        )
        .await;
        caller.refresh(&state).await?;
    }
}
