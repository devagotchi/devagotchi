//! Owns no tables. Serves the shared audit history refresh asset. Settings: none.
//! Depends on each audit page's authorized change endpoint and rendered refresh URL.
use crate::AppState;
use axum::{Router, response::Response, routing::get};

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/assets/audit-history.js", get(script))
}

async fn script() -> Response {
    super::notify::asset(
        include_str!("audit.js").into(),
        "text/javascript; charset=utf-8",
        "no-cache",
    )
}
