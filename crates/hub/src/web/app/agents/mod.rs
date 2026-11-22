//! Registered agents for the current environment.
//!
//! Owns: no tables; identity owns profiles and credential bindings.
//! Serves: /app/agents, profile and OAuth-binding forms, /app/agents/configuration.
//! Settings: registered kinds/roles/policy and WIP, with owner-only authority changes.
//! Depends on: identity accounts, AgentHub authorized workload reads, site copy and pages.
use crate::{
    AppState,
    identity::{
        Subject,
        accounts::{
            Accounts,
            agents::{Agent, AgentEdit, OAuthBinding},
        },
    },
    web::{
        encode,
        pages::{self, Tab, escape},
        routes::unavailable,
    },
};
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;

#[cfg(test)]
mod acceptance_tests;
mod configuration;
mod form;
#[cfg(test)]
mod receipt_tests;
mod render;
#[cfg(test)]
mod tests;

#[derive(Default, Deserialize)]
struct Shown {
    account: Option<String>,
    after: Option<String>,
    role: Option<String>,
    saved: Option<String>,
    board: Option<String>,
    ticket: Option<String>,
}

struct Context {
    actor: Subject,
    accounts: Arc<Accounts>,
    account: String,
    account_name: String,
    owner: bool,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/agents", get(list).post(save))
        .route("/app/agents/new", get(new))
        .route(
            "/app/agents/configuration",
            get(configuration::page).post(configuration::save),
        )
        .route("/app/agents/{id}", get(detail))
        .route("/app/agents/{id}/oauth", axum::routing::post(bind))
}

async fn context(
    state: &AppState,
    headers: &HeaderMap,
    account: Option<&str>,
    change: bool,
) -> Result<Context, Response> {
    let actor = super::person(state, headers, "/app/agents").await?;
    if change
        && (!crate::web::approvals::same_origin(headers, &state.hub.config.public_url)
            || headers
                .get("sec-fetch-site")
                .is_some_and(|value| value != "same-origin"))
    {
        return Err(error(&devagotchi_contracts::BridgeError::new(
            "not_granted",
            "manage agents from this hub's own page",
        )));
    }
    let accounts = state
        .passkeys
        .as_ref()
        .ok_or_else(crate::web::operator::hidden)?
        .accounts()
        .clone();
    let live = accounts
        .get(&actor.id)
        .await
        .map_err(|e| error(&e))?
        .filter(|account| account.disabled_at.is_none())
        .ok_or_else(crate::web::operator::hidden)?;
    let account = account.unwrap_or(&actor.id).to_string();
    if account != actor.id && !live.owner {
        return Err(crate::web::operator::hidden());
    }
    let target = accounts
        .get(&account)
        .await
        .map_err(|e| error(&e))?
        .filter(|target| target.tenant == live.tenant && target.disabled_at.is_none())
        .ok_or_else(crate::web::operator::hidden)?;
    Ok(Context {
        actor,
        accounts,
        account,
        account_name: target.name,
        owner: live.owner,
    })
}

fn error(e: &devagotchi_contracts::BridgeError) -> Response {
    let status = match e.code.as_str() {
        "not_found" | "not_granted" => return crate::web::operator::hidden(),
        "version_conflict" | "in_use" | "request_conflict" => StatusCode::CONFLICT,
        "request_retired" => StatusCode::GONE,
        "invalid" | "self_lockout" | "policy_denied" | "agent_disabled" | "limit"
        | "humans_only" => StatusCode::BAD_REQUEST,
        _ => {
            eprintln!("agent administration: {e}");
            return unavailable();
        }
    };
    let site = pages::site();
    let c = &site.messages.app.agents;
    pages::app_page(
        status,
        &c.title,
        &format!(
            "<h1>{}</h1><p role=\"alert\">{} {}</p><a href=\"/app/agents\">{}</a>",
            escape(&c.title),
            escape(&c.refused),
            escape(&e.message),
            escape(&c.back)
        ),
        Tab::Me,
    )
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let ctx = match context(&state, &headers, shown.account.as_deref(), false).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match ctx
        .accounts
        .principal_roster(
            &ctx.actor.id,
            shown.account.as_deref(),
            shown.after.as_deref(),
            shown.role.as_deref(),
        )
        .await
    {
        Ok(agents) => render::list(&ctx, &agents, &shown),
        Err(e) => error(&e),
    }
}

async fn new(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let ctx = match context(&state, &headers, shown.account.as_deref(), false).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !ctx.owner {
        return crate::web::operator::hidden();
    }
    if let Err(e) = ctx.accounts.agents(&ctx.actor.id, &ctx.account, None).await {
        return error(&e);
    }
    render::edit(&ctx, &form::AgentForm::new(&ctx.account), None, None, None).await
}

async fn detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    let ctx = match context(&state, &headers, shown.account.as_deref(), false).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let agent = match ctx.accounts.agent(&ctx.actor.id, &ctx.account, &id).await {
        Ok(v) => v,
        Err(e) => return error(&e),
    };
    let work = match render::work(&state, &ctx, &agent, &shown).await {
        Ok(v) => v,
        Err(e) => return error(&e),
    };
    let bindings = match render::bindings(&ctx, &agent).await {
        Ok(v) => v,
        Err(e) => return error(&e),
    };
    render::edit(
        &ctx,
        &form::AgentForm::from_agent(&agent),
        shown.saved.as_deref(),
        None,
        Some(&(work + &bindings)),
    )
    .await
}

async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let input = match form::AgentForm::parse(pairs) {
        Ok(input) => input,
        Err(e) => return error(&e),
    };
    let ctx = match context(&state, &headers, Some(&input.account), true).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let edit = match input.edit() {
        Ok(v) => v,
        Err(e) => return render::edit(&ctx, &input, None, Some(&e), None).await,
    };
    match ctx
        .accounts
        .put_agent_requested(
            &ctx.actor.id,
            &ctx.account,
            &edit,
            input.request_id.as_deref(),
        )
        .await
    {
        Ok(agent) => super::requests::redirect(
            &format!(
                "/app/agents/{}?account={}&saved=1",
                encode(&agent.value.id),
                encode(&ctx.account)
            ),
            agent.replayed,
        ),
        Err(e)
            if matches!(
                e.code.as_str(),
                "invalid" | "version_conflict" | "in_use" | "policy_denied"
            ) =>
        {
            render::edit(&ctx, &input, None, Some(&e), None).await
        }
        Err(e) => error(&e),
    }
}

#[derive(Deserialize)]
struct BindingForm {
    account: String,
    client: String,
    request_id: Option<String>,
    version: i64,
    #[serde(default)]
    enabled: bool,
}

async fn bind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<BindingForm>,
) -> Response {
    let ctx = match context(&state, &headers, Some(&input.account), true).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let binding = OAuthBinding {
        client: input.client,
        agent_id: id.clone(),
        version: input.version,
        enabled: input.enabled,
    };
    match ctx
        .accounts
        .bind_agent_oauth_requested(
            &ctx.actor.id,
            &ctx.account,
            &binding,
            input.request_id.as_deref(),
        )
        .await
    {
        Ok(binding) => super::requests::redirect(
            &format!(
                "/app/agents/{}?account={}&saved=1",
                encode(&id),
                encode(&ctx.account)
            ),
            binding.replayed,
        ),
        Err(e) => error(&e),
    }
}
