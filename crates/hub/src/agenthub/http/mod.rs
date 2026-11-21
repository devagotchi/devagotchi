//! AgentHub's JSON task API.
//! Owns: no tables; board operations and receipts remain with AgentHub/PostgreSQL.
//! Serves: /api/v1/tasks and /api/v1/claim-next, authenticated by the hub's middleware.
//! Settings: board configuration in the existing Board settings page.
//! Depends on: authenticated identity, the shared board resolver, typed MCP inputs and audit.
use crate::AppState;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

mod access;
mod events;
mod input;
mod leases;
mod reads;
mod response;
mod tickets;

use access::Caller;
use input::{Body, Scope};
use response::{Failure, Reply, invalid};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tasks", get(reads::list).post(tickets::create))
        .route(
            "/api/v1/tasks/{id}",
            get(reads::detail).patch(tickets::update),
        )
        .route("/api/v1/tasks/{id}/{operation}", post(leases::operation))
        .route("/api/v1/claim-next", post(leases::claim_next))
        .route("/api/v1/events", get(events::list))
        .route("/api/v1/events/verify", get(events::verify))
        .layer(DefaultBodyLimit::max(1024 * 1024))
}

#[cfg(test)]
mod tests;
