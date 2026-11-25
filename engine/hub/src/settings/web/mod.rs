//! `/admin`, the hub's settings page: the page and its sign-in and ClickHouse cards (`admin`),
//! who may open it (`owner`), the live settings' cards (`live`), Email (`mail`, with the mail
//! server in force in `mail_cell`) and Signal sinks (`sinks`).

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub(crate) mod admin;
pub(crate) mod live;
pub(crate) mod mail;
pub(crate) mod mail_cell;
pub(crate) mod owner;
pub(crate) mod sinks;

#[cfg(test)]
mod tests;

/// `/admin`: the page, its sign-in and ClickHouse cards, the live settings' cards, Email and
/// Signal sinks.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(admin::page))
        .route("/admin/sign-in/{name}", post(admin::save_sign_in))
        .route("/admin/analytics", post(admin::save_analytics))
        .route("/admin/settings/{group}", post(live::save))
        .route("/admin/mail", post(mail::save))
        .route("/admin/sinks/{name}", post(sinks::save))
        .route("/admin/sinks/{name}/remove", post(sinks::remove))
}
