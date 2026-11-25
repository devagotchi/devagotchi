//! Stable retry identity for browser mutation forms. These keys grant no authority.
use crate::web::pages::escape;

/// One key per rendered form; validation redisplays retain it until a successful redirect.
pub(crate) fn request_id() -> String {
    format!("form-{}", hex::encode(crate::machines::random::<16>()))
}

pub(crate) fn request_field(key: Option<&str>) -> String {
    key.map(|key| {
        format!(
            "<input type=\"hidden\" name=\"request_id\" value=\"{}\">",
            escape(key)
        )
    })
    .unwrap_or_default()
}

/// The original result is returned without a second mutation; expose source HTTP parity.
pub(crate) fn redirect(location: &str, replayed: bool) -> axum::response::Response {
    let mut response = crate::web::routes::see_other(location, &[]);
    if replayed {
        response.headers_mut().insert(
            "idempotent-replayed",
            axum::http::HeaderValue::from_static("true"),
        );
    }
    response
}
