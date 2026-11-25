//! Tenancy's pages: the way into a hosted hub (`hosted`), the hosted hub's operator page
//! (`/operator`, `operator`), organizations and their invites (`orgs`, on a hosted hub) and a
//! single environment's local people (`people`). The tenants and their rules are in
//! `crate::tenancy` and `crate::identity::accounts`; these pages show them and act through them.

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub(crate) mod hosted;
pub(crate) mod operator;
pub(crate) mod orgs;
pub(crate) mod people;

#[cfg(test)]
pub(crate) mod tests;

/// The operator page and its forms.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/operator", get(operator::page))
        .route("/operator/admit", post(operator::admit))
        .route("/operator/plan", post(operator::plan))
}
