//! Ops' pages: the operator's analytics (`/operator/analytics`, `analytics`), counting visits
//! to the public pages as the hub serves them (`counted`), the hub's audit history
//! (`hub_audit`) and the script that refreshes an audit history (`audit_live`). The counts and
//! the chain are in `crate::ops`; these pages show them.

use axum::{Router, routing::get};

use crate::AppState;

pub(crate) mod analytics;
pub(crate) mod audit_live;
pub(crate) mod counted;
pub(crate) mod hub_audit;

#[cfg(test)]
mod tests;

/// The audit history pages and the operator's analytics.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .merge(audit_live::routes())
        .merge(hub_audit::routes())
        .route("/operator/analytics", get(analytics::page))
        .route("/operator/analytics.csv", get(analytics::csv))
}
