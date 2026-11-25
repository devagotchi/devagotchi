//! Browser board/task creation, ported from the October AgentHub interface.
//! Owns no tables: binding uses Hub::bind_project; tasks use Board::create.
//! Serves creation forms and POSTs. Copy is site.messages.app.creation.
//! Depends on browser identity, the hub's board grants and AgentHub's transactions.

use crate::web::app::{person, words};
use crate::{
    AppState,
    agenthub::{Actor, Board, NewTicket},
    identity::Subject,
    web::{
        encode,
        pages::{self, Tab, escape},
        routes::see_other,
    },
};
use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use devagotchi_contracts::BridgeError;
use serde::Deserialize;
use std::sync::Arc;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/new-board", get(board_form).post(create_board))
        .route(
            "/app/boards/{project}/new",
            get(task_form).post(create_task),
        )
}

#[derive(Default, Deserialize)]
pub(super) struct BoardForm {
    #[serde(default)]
    project: String,
}

#[derive(Default, Deserialize, serde::Serialize)]
pub(super) struct TaskForm {
    #[serde(skip_serializing)]
    request_id: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    priority: Option<i64>,
    #[serde(default)]
    labels: String,
    #[serde(default)]
    depends_on: String,
}

fn rendered(status: StatusCode, title: &str, body: &str) -> Response {
    let mut response = pages::app_page(status, title, body, Tab::Boards);
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn failure(error: &BridgeError) -> StatusCode {
    match error.code.as_str() {
        "account_store" | "board_store" | "board_damaged" | "runtime" => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        "request_conflict" => StatusCode::CONFLICT,
        "request_retired" => StatusCode::GONE,
        "unknown_board" | "not_found" => StatusCode::NOT_FOUND,
        "not_granted" | "not_owner" | "read_only" | "not_permitted" => StatusCode::FORBIDDEN,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn refused(error: &BridgeError) -> Response {
    let site = words();
    pages::message(
        failure(error),
        &site.messages.app.creation.refused,
        &error.message,
        Some(("/app/boards", &site.messages.app.boards_title)),
    )
}

async fn change(state: &AppState, headers: &HeaderMap, here: &str) -> Result<Subject, Response> {
    let subject = person(state, headers, here).await?;
    let origin = crate::identity::web::passkeys::origin_of(&state.hub.config.public_url);
    if origin.is_none()
        || headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != origin.as_deref()
        || headers
            .get("sec-fetch-site")
            .is_some_and(|v| v != "same-origin")
    {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    if let Some(error) = state.hub.waiting_refusal(&subject) {
        return Err(refused(&error));
    }
    Ok(subject)
}

fn writable(state: &AppState, subject: &Subject, project: &str) -> Result<Arc<Board>, Response> {
    match state.hub.board_access(subject, project) {
        Ok((board, _, role)) if role.is_person() => Ok(board),
        Ok(_) => {
            let site = words();
            Err(pages::message(
                StatusCode::FORBIDDEN,
                &site.messages.app.creation.refused,
                &site.messages.app.creation.read_only,
                Some(("/app/boards", &site.messages.app.boards_title)),
            ))
        }
        Err(error) => Err(refused(&error)),
    }
}

fn board_page(form: &BoardForm, error: Option<&BridgeError>) -> Response {
    let site = words();
    let copy = &site.messages.app.creation;
    let notice = error
        .map(|e| format!("<p role=\"alert\">{}</p>", escape(&e.message)))
        .unwrap_or_default();
    rendered(
        error.map(failure).unwrap_or(StatusCode::OK),
        &copy.board,
        &format!(
            "<h1>{}</h1>{notice}<form method=\"post\" action=\"/app/new-board\" class=\"card stack\">\
<label>{}<input type=\"text\" name=\"project\" value=\"{}\" required maxlength=\"63\" pattern=\"[a-z0-9][a-z0-9-]{{0,62}}\" autocomplete=\"off\" aria-describedby=\"board-name-help\"></label>\
<p id=\"board-name-help\">{}</p><div class=\"actions\"><button type=\"submit\" class=\"button primary\">{}</button>\
<a href=\"/app/boards\">{}</a></div></form>",
            escape(&copy.board),
            escape(&copy.board_name),
            escape(&form.project),
            escape(&copy.board_help),
            escape(&copy.create),
            escape(&copy.cancel)
        ),
    )
}

async fn board_form(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = person(&state, &headers, "/app/new-board").await {
        return response;
    }
    board_page(&BoardForm::default(), None)
}

async fn create_board(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<BoardForm>,
) -> Response {
    let subject = match change(&state, &headers, "/app/new-board").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    match state.hub.bind_project(&subject, form.project.trim()).await {
        Ok(bound) => see_other(&format!("/app/boards/{}", encode(&bound.project)), &[]),
        Err(error) => board_page(&form, Some(&error)),
    }
}

fn task_page(project: &str, form: &TaskForm, error: Option<&BridgeError>) -> Response {
    let site = words();
    let copy = &site.messages.app.creation;
    let notice = error
        .map(|e| format!("<p role=\"alert\">{}</p>", escape(&e.message)))
        .unwrap_or_default();
    let request = crate::web::app::requests::request_field(form.request_id.as_deref());
    rendered(
        error.map(failure).unwrap_or(StatusCode::OK),
        &copy.task,
        &format!(
            "<h1>{}</h1><p>{}</p>{notice}<form method=\"post\" action=\"/app/boards/{}/new\" class=\"card stack\">{request}\
<label>{}<input type=\"text\" name=\"title\" value=\"{}\" required maxlength=\"300\"></label>\
<label>{}<textarea name=\"description\" rows=\"6\">{}</textarea></label>\
<label>{}<input type=\"number\" name=\"priority\" value=\"{}\" required></label>\
<label>{}<input type=\"text\" name=\"labels\" value=\"{}\" aria-describedby=\"labels-help\"></label><p id=\"labels-help\">{}</p>\
<label>{}<input type=\"text\" name=\"depends_on\" value=\"{}\" aria-describedby=\"deps-help\"></label><p id=\"deps-help\">{}</p>\
<div class=\"actions\"><button type=\"submit\" class=\"button primary\">{}</button><a href=\"/app/boards/{}\">{}</a></div></form>",
            escape(&copy.task),
            escape(project),
            encode(project),
            escape(&copy.title),
            escape(&form.title),
            escape(&copy.description),
            escape(&form.description),
            escape(&copy.priority),
            form.priority.map(|p| p.to_string()).unwrap_or_default(),
            escape(&copy.labels),
            escape(&form.labels),
            escape(&copy.labels_help),
            escape(&copy.dependencies),
            escape(&form.depends_on),
            escape(&copy.dependencies_help),
            escape(&copy.create),
            encode(project),
            escape(&copy.cancel)
        ),
    )
}

async fn task_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Response {
    let here = format!("/app/boards/{}/new", encode(&project));
    let subject = match person(&state, &headers, &here).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let board = match writable(&state, &subject, &project) {
        Ok(b) => b,
        Err(r) => return r,
    };
    match board.settings() {
        Ok(settings) => task_page(
            &project,
            &TaskForm {
                priority: Some(settings.default_priority),
                request_id: Some(crate::web::app::requests::request_id()),
                ..Default::default()
            },
            None,
        ),
        Err(error) => refused(&error),
    }
}

async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Form(form): Form<TaskForm>,
) -> Response {
    let here = format!("/app/boards/{}/new", encode(&project));
    let subject = match change(&state, &headers, &here).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let board = match writable(&state, &subject, &project) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let new = NewTicket {
        title: form.title.clone(),
        description: form.description.clone(),
        priority: form.priority,
        labels: form
            .labels
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        depends_on: form
            .depends_on
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|s| s.trim_start_matches('#'))
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        ..Default::default()
    };
    let actor = subject.id.clone();
    let requested = match form
        .request_id
        .as_deref()
        .map(|key| board.requested(&actor, key, "create_task", &form))
        .transpose()
    {
        Ok(requested) => requested,
        Err(error) => return task_page(&project, &form, Some(&error)),
    };
    let result = tokio::task::spawn_blocking(move || {
        let board = requested.as_ref().unwrap_or(&board);
        board
            .create(&Actor::human(&actor), new)
            .map(|ticket| (ticket, board.hosted().replayed()))
    })
    .await
    .unwrap_or_else(|_| Err(BridgeError::new("board_store", "the board did not answer")));
    let (target, outcome) = match &result {
        Ok((ticket, _)) => (ticket.id.as_str(), "ok"),
        Err(error) => ("", error.code.as_str()),
    };
    state
        .hub
        .record_completion(&subject, "board_create", &project, target, outcome)
        .await;
    match result {
        Ok((ticket, replayed)) => crate::web::app::requests::redirect(
            &format!("/app/tickets/{}/{}", encode(&project), encode(&ticket.id)),
            replayed,
        ),
        Err(error) => task_page(&project, &form, Some(&error)),
    }
}

#[cfg(test)]
mod tests;
