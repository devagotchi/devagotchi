//! Machines' pages: Add a machine and Revoke on the Machines page (`forms`), one machine's
//! page with Update now (`/app/machines/<name>`, `machine`), and the requests for and grants
//! of access to a machine (`/app/access/*`, `access`). The machines and their rules are in
//! `crate::machines`; these pages show them and act through them.

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub(crate) mod access;
pub(crate) mod forms;
pub(crate) mod machine;

#[cfg(test)]
mod tests;

/// The machines' pages and forms, and `POST /api/v1/machines`.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .merge(access::routes())
        .route("/app/machines/{name}", get(machine::page))
        .route("/app/machines/{name}/update", post(machine::update))
        .route("/api/v1/machines", post(forms::add_json))
        .route("/machines/add", post(forms::add))
        .route("/machines/revoke", post(forms::revoke))
}
