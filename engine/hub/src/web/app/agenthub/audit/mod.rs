//! Board audit pages. Owns no tables; reads AgentHub's board_events through Board.
//! Serves history and change waits. Settings: none. Depends on the app session/board guards.
use super::super::{open_board, person, words};
use crate::{
    AppState,
    agenthub::{Board, EventFilter},
    web::{
        encode,
        pages::{self, Tab, escape},
    },
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;

mod render;
#[cfg(test)]
mod tests;

#[derive(Clone, Default, Deserialize)]
struct Shown {
    actor: Option<String>,
    action: Option<String>,
    ticket: Option<String>,
    before: Option<i64>,
    #[serde(default)]
    verify: bool,
}

impl Shown {
    fn href(&self, project: &str, before: Option<i64>, verify: bool) -> String {
        let mut query = Vec::new();
        for (key, value) in [
            ("actor", &self.actor),
            ("action", &self.action),
            ("ticket", &self.ticket),
        ] {
            if let Some(value) = value.as_deref().filter(|v| !v.is_empty()) {
                query.push(format!("{key}={}", encode(value)));
            }
        }
        if let Some(before) = before {
            query.push(format!("before={before}"));
        }
        if verify {
            query.push("verify=true".into());
        }
        format!("/app/boards/{}/audit?{}", encode(project), query.join("&"))
    }

    fn filter(&self) -> EventFilter {
        let nonempty = |v: &Option<String>| v.clone().filter(|v| !v.is_empty());
        EventFilter {
            actor: nonempty(&self.actor),
            action_prefix: nonempty(&self.action),
            ticket: nonempty(&self.ticket),
            before_seq: self.before,
            descending: true,
            limit: Some(101),
            ..Default::default()
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/boards/{project}/audit", get(page))
        .route("/app/boards/{project}/audit/changes", get(changes))
}

async fn access(
    state: &AppState,
    headers: &HeaderMap,
    project: &str,
) -> Result<(Arc<Board>, String), Response> {
    let here = format!("/app/boards/{}/audit", encode(project));
    let subject = person(state, headers, &here).await?;
    match open_board(state, &subject, project) {
        Ok(Some(board)) => Ok((board, subject.id)),
        Ok(None) => {
            let site = words();
            let copy = &site.messages.app;
            Err(pages::app_page(
                StatusCode::NOT_FOUND,
                &copy.board_unknown,
                &format!(
                    "<h1>{}</h1><p>{}</p>",
                    escape(&copy.board_unknown),
                    escape(&copy.board_unknown_text)
                ),
                Tab::Boards,
            ))
        }
        Err(error) => {
            eprintln!("devagotchi hub: audit board access: {error}");
            Err(crate::web::routes::unavailable())
        }
    }
}

async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    let (board, me) = match access(&state, &headers, &project).await {
        Ok(board) => board,
        Err(response) => return response,
    };
    let site = words();
    let copy = &site.messages.app.audit;
    if shown.before.is_some_and(|seq| seq < 1) {
        return pages::app_page(
            StatusCode::BAD_REQUEST,
            &copy.title,
            &escape(&copy.invalid_cursor),
            Tab::Boards,
        );
    }
    let filter = shown.filter();
    let verify = shown.verify;
    let read = tokio::task::spawn_blocking(move || {
        let latest = board.events_matching(&EventFilter {
            descending: true,
            limit: Some(1),
            ..Default::default()
        })?;
        let events = board.events_matching(&filter)?;
        let chain = if verify {
            Some(board.verify_chain()?)
        } else {
            None
        };
        devagotchi_contracts::BridgeResult::Ok((latest.first().map_or(0, |e| e.seq), events, chain))
    })
    .await;
    let (head, events, chain) = match read {
        Ok(Ok(read)) => read,
        error => {
            eprintln!("devagotchi hub: read board audit: {error:?}");
            return crate::web::routes::unavailable();
        }
    };
    pages::app_page(
        StatusCode::OK,
        &copy.title,
        &render::body(&project, &shown, head, &events, chain.as_ref(), &site, &me),
        Tab::Boards,
    )
}

#[derive(Deserialize)]
struct After {
    after: i64,
}

async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(input): Query<After>,
) -> Response {
    let board = match access(&state, &headers, &project).await {
        Ok((board, _)) => board,
        Err(response) => return response,
    };
    if input.after < 0 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
    let mut changed = board.changes();
    loop {
        changed.borrow_and_update();
        // Check again after every wake: a session or board grant can have ended while waiting.
        let current = match access(&state, &headers, &project).await {
            Ok((board, _)) => board,
            Err(response) => return response,
        };
        match current.hosted().events_page(input.after, 1).await {
            Ok(events) if !events.is_empty() => {
                return (
                    [("cache-control", "no-store")],
                    Json(serde_json::json!({"changed":true})),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("devagotchi hub: wait for board audit: {error}");
                return crate::web::routes::unavailable();
            }
        }
        if !matches!(
            tokio::time::timeout_at(deadline, changed.changed()).await,
            Ok(Ok(()))
        ) {
            return (
                [("cache-control", "no-store")],
                Json(serde_json::json!({"changed":false})),
            )
                .into_response();
        }
    }
}
