//! Browser project lifecycle. All changes use the same account and board owners as HTTP.
use crate::{
    AppState,
    agenthub::{
        SharedWorkflow,
        hosted::{ProjectEdit, ProjectRecord},
    },
    identity::Subject,
    web::{
        app::{person, requests},
        encode,
        pages::{self, Tab, escape},
    },
};
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use devagotchi_contracts::BridgeError;
use serde::Deserialize;
mod form;
mod render;
use form::{Draft, Removal};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/projects", get(list))
        .route("/app/projects/new", get(new).post(create))
        .route("/app/projects/{project}", get(editor).post(save))
        .route("/app/projects/{project}/remove", post(remove))
        .route("/app/projects/archive/{generation}", get(history))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    retired: bool,
    after: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    #[serde(default)]
    after_seq: i64,
}

fn status(error: &BridgeError) -> StatusCode {
    match error.code.as_str() {
        "not_found" | "unknown_board" => StatusCode::NOT_FOUND,
        "not_owner" | "not_granted" | "humans_only" => StatusCode::FORBIDDEN,
        "in_use" | "already_exists" | "version_conflict" | "request_conflict" | "plan_limit"
        | "name_taken" => StatusCode::CONFLICT,
        "request_retired" => StatusCode::GONE,
        "board_store" | "account_store" | "runtime" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}
fn refused(error: &BridgeError) -> Response {
    let site = pages::site();
    let c = &site.messages.app.projects;
    pages::message(
        status(error),
        &c.refused,
        &error.message,
        Some(("/app/projects", &c.back)),
    )
}
async fn access(
    state: &AppState,
    headers: &HeaderMap,
    changing: bool,
) -> Result<Subject, Response> {
    let subject = person(state, headers, "/app/projects").await?;
    if changing && !crate::web::from_this_hub(headers, &state.hub.config.public_url) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(subject)
}
async fn catalog(state: &AppState, subject: &Subject) -> Result<Vec<SharedWorkflow>, BridgeError> {
    let projects = state.hub.owned_projects(subject, false, None).await?;
    let Some(project) = projects.first() else {
        return Ok(Vec::new());
    };
    let (board, _, _) = state.hub.board_access(subject, &project.id)?;
    board.hosted().shared_workflows().await
}
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<Page>,
) -> Response {
    let subject = match access(&state, &headers, false).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    match state
        .hub
        .owned_projects(&subject, query.retired, query.after.as_deref())
        .await
    {
        Ok(projects) => render::list(&projects, query.retired),
        Err(e) => refused(&e),
    }
}
async fn new(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let subject = match access(&state, &headers, false).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    match catalog(&state, &subject).await {
        Ok(items) => render::editor(&Draft::new(), &items, None),
        Err(e) => refused(&e),
    }
}
async fn editor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Response {
    let subject = match access(&state, &headers, false).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let record = match state.hub.owned_project(&subject, &project).await {
        Ok(r) => r,
        Err(e) => return refused(&e),
    };
    match catalog(&state, &subject).await {
        Ok(items) => render::editor(&Draft::from_record(record, &items), &items, None),
        Err(e) => refused(&e),
    }
}
async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(draft): Form<Draft>,
) -> Response {
    change(state, headers, None, draft).await
}
async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Form(draft): Form<Draft>,
) -> Response {
    change(state, headers, Some(project), draft).await
}
async fn change(
    state: AppState,
    headers: HeaderMap,
    project: Option<String>,
    draft: Draft,
) -> Response {
    let subject = match access(&state, &headers, true).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let items = match catalog(&state, &subject).await {
        Ok(items) => items,
        Err(e) => return refused(&e),
    };
    let result = async {
        if project.as_ref().is_some_and(|id| id != &draft.id)
            || project.is_none() && draft.version != 0
            || project.is_some() && draft.version <= 0
        {
            return Err(BridgeError::new(
                "invalid",
                "project address or version disagrees with this form",
            ));
        }
        state
            .hub
            .save_owned_project(&subject, &draft.edit()?, Some(&draft.request_id))
            .await
    }
    .await;
    match result {
        Ok((record, replayed)) => {
            requests::redirect(&format!("/app/projects/{}", encode(&record.id)), replayed)
        }
        Err(e) => render::editor(&draft, &items, Some(&e)),
    }
}
async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Form(draft): Form<Removal>,
) -> Response {
    let subject = match access(&state, &headers, true).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    match state
        .hub
        .remove_owned_project(&subject, &project, draft.version, Some(&draft.request_id))
        .await
    {
        Ok((record, replayed)) => requests::redirect(
            &format!("/app/projects/archive/{}", encode(&record.board_id)),
            replayed,
        ),
        Err(e) => refused(&e),
    }
}
async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(generation): Path<String>,
    Query(query): Query<History>,
) -> Response {
    let subject = match access(&state, &headers, false).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    match state
        .hub
        .owned_project_history(&subject, &generation, query.after_seq)
        .await
    {
        Ok((record, events)) => render::history(&record, &events),
        Err(e) => refused(&e),
    }
}
