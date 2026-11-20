//! Owns no tables. Serves board settings and workflow forms. Settings: lease/task defaults
//! and workflow through AgentHub. Depends on browser identity and the live board-role resolver.
use super::super::{person, words};
use crate::{
    AppState,
    agenthub::{Actor, Board},
    web::{
        pages,
        routes::{see_other, unavailable},
    },
};
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use devagotchi_contracts::BridgeError;
use serde::Deserialize;
use std::sync::Arc;

mod form;
mod render;
#[cfg(test)]
mod tests;
use form::{Fields, SettingsDraft, WorkflowDraft};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/app/boards/{project}/settings",
            get(settings_page).post(save_settings),
        )
        .route(
            "/app/boards/{project}/workflow",
            get(workflow_page).post(save_workflow),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024))
}
#[derive(Default, Deserialize)]
struct Shown {
    saved: Option<String>,
}

fn refused(status: StatusCode, message: &str) -> Response {
    let site = words();
    pages::message(
        status,
        &site.messages.app.configuration.refused,
        message,
        Some(("/app/boards", &site.messages.app.boards_title)),
    )
}
fn invalid_form() -> Response {
    refused(
        StatusCode::BAD_REQUEST,
        &words().messages.app.configuration.invalid_form,
    )
}

async fn access(
    state: &AppState,
    headers: &HeaderMap,
    project: &str,
    workflow: bool,
    changing: bool,
) -> Result<(Arc<Board>, String), Response> {
    if changing && !crate::web::from_this_hub(headers, &state.hub.config.public_url) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    let subject = person(state, headers, &render::href(project, workflow)).await?;
    if changing && let Some(error) = state.hub.waiting_refusal(&subject) {
        return Err(refused(StatusCode::FORBIDDEN, &error.message));
    }
    match state.hub.board_access(&subject, project) {
        Ok((board, _, role)) if role.may_configure() => Ok((board, subject.id)),
        Ok(_) => Err(refused(
            StatusCode::FORBIDDEN,
            &words().messages.app.configuration.read_only,
        )),
        Err(e) if matches!(e.code.as_str(), "unknown_board" | "not_granted") => Err(refused(
            StatusCode::NOT_FOUND,
            &words().messages.app.board_unknown_text,
        )),
        Err(e) => {
            eprintln!("devagotchi hub: board configuration access: {e}");
            Err(unavailable())
        }
    }
}
async fn settings_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    page(state, headers, project, shown, false).await
}
async fn workflow_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    page(state, headers, project, shown, true).await
}
async fn page(
    state: AppState,
    headers: HeaderMap,
    project: String,
    shown: Shown,
    workflow: bool,
) -> Response {
    let (board, _) = match access(&state, &headers, &project, workflow, false).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let current = match tokio::task::spawn_blocking(move || board.configuration_state()).await {
        Ok(Ok(c)) => c,
        error => {
            eprintln!("devagotchi hub: read board configuration: {error:?}");
            return unavailable();
        }
    };
    let site = words();
    let notice = if shown.saved.as_deref() == Some("1") {
        site.messages.app.configuration.saved.as_str()
    } else {
        ""
    };
    if workflow {
        render::workflow(
            &project,
            &WorkflowDraft::current(&current),
            StatusCode::OK,
            notice,
        )
    } else {
        render::settings(
            &project,
            &SettingsDraft::current(&current),
            StatusCode::OK,
            notice,
        )
    }
}
fn failure(error: &BridgeError) -> (StatusCode, String) {
    let site = words();
    let c = &site.messages.app.configuration;
    match error.code.as_str() {
        "version_conflict" => (StatusCode::CONFLICT, c.conflict.clone()),
        "account_store" | "board_store" | "board_damaged" | "runtime" => {
            eprintln!("devagotchi hub: save board configuration: {error}");
            (StatusCode::SERVICE_UNAVAILABLE, c.refused.clone())
        }
        "humans_only" | "not_permitted" => (StatusCode::FORBIDDEN, c.read_only.clone()),
        _ => (
            StatusCode::BAD_REQUEST,
            format!("{} {}", c.refused, error.message),
        ),
    }
}
async fn save_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let (board, me) = match access(&state, &headers, &project, false, true).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let draft = match Fields::new(pairs).and_then(SettingsDraft::parse) {
        Ok(d) => d,
        Err(()) => return invalid_form(),
    };
    let edit = match draft.edit() {
        Ok(e) => e,
        Err(()) => {
            return render::settings(
                &project,
                &draft,
                StatusCode::BAD_REQUEST,
                &words().messages.app.configuration.invalid_form,
            );
        }
    };
    let version = draft.version;
    let result = tokio::task::spawn_blocking(move || {
        board.edit_settings(&Actor::human(&me), version, &edit)
    })
    .await
    .unwrap_or_else(|e| Err(BridgeError::new("runtime", e.to_string())));
    match result {
        Ok(_) => see_other(&format!("{}?saved=1", render::href(&project, false)), &[]),
        Err(error) => {
            let (status, message) = failure(&error);
            render::settings(&project, &draft, status, &message)
        }
    }
}
async fn save_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let (board, me) = match access(&state, &headers, &project, true, true).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let (mut draft, action) = match Fields::new(pairs).and_then(WorkflowDraft::parse) {
        Ok(d) => d,
        Err(()) => return invalid_form(),
    };
    if action != "save" {
        let site = words();
        let c = &site.messages.app.configuration;
        if draft.change_rows(&action, &c.new_state).is_err() {
            return render::workflow(&project, &draft, StatusCode::BAD_REQUEST, &c.invalid_form);
        }
        return render::workflow(&project, &draft, StatusCode::OK, &c.draft);
    }
    let saved = draft.clone();
    let result = tokio::task::spawn_blocking(move || {
        board.edit_workflow(&Actor::human(&me), saved.version, &saved.workflow)
    })
    .await
    .unwrap_or_else(|e| Err(BridgeError::new("runtime", e.to_string())));
    match result {
        Ok(_) => see_other(&format!("{}?saved=1", render::href(&project, true)), &[]),
        Err(error) => {
            let (status, message) = failure(&error);
            render::workflow(&project, &draft, status, &message)
        }
    }
}
