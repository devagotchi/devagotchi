//! AgentHub's screens in the phone app: a board (`/app/boards/{project}`), a ticket
//! (`/app/tickets/{project}/{id}`) and what a person does to one, and `/approvals`, the
//! requests agents wait on. The board itself and its rules are in `crate::agenthub`; these
//! only show it and act through it.

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub(crate) mod approvals;
pub(crate) mod audit;
pub(crate) mod board;
mod board_cards;
pub(crate) mod configuration;
pub(crate) mod create;
pub(crate) mod projects;
pub(crate) mod ticket;
pub(crate) mod ticket_actions;

#[cfg(test)]
mod tests;

/// AgentHub's pages: the boards, tickets, their configuration and audit, and `/approvals`.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .merge(create::routes())
        .merge(projects::routes())
        .merge(audit::routes())
        .merge(configuration::routes())
        .route("/app/boards/{project}", get(board::page))
        .route("/app/tickets/{project}/{id}", get(ticket::page))
        .route(
            "/app/tickets/{project}/{id}/{action}",
            post(ticket_actions::act),
        )
        .route("/approvals", get(approvals::list))
        .route("/approvals/decide", post(approvals::decide))
        .route("/approvals/test", post(approvals::test))
        .route("/approvals/standing", post(approvals::grant))
        .route("/approvals/standing/revoke", post(approvals::revoke))
}
