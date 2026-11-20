//! How `/approvals` lays out its parts (`page.css`): what waits for a decision, as many side by
//! side as fit; what was approved ahead of time and this phone's notifications beside them on
//! a wide screen, under them on a phone.

use axum::{http::StatusCode, response::Response};

use crate::web::pages;

/// The page, from its parts as they were rendered.
pub(super) fn page(flash: &str, asks: &str, ahead: &str, notify: &str) -> Response {
    pages::app(
        StatusCode::OK,
        "Approvals",
        &format!(
            "<div class=\"stack\" data-page=\"approvals\"><h1>Approvals</h1>{flash}\
<div class=\"sidebar\" data-side=\"end\"><div class=\"auto-grid asks\">{asks}</div>\
<aside class=\"stack approvals-side\">{ahead}{notify}</aside></div></div>"
        ),
        pages::Tab::Approvals,
    )
}
