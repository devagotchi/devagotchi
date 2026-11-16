//! The hub's two doors for machines: `/link`, where each machine's `devagotchi link` holds its
//! WebSocket, and `/enroll`, where a machine binds its key with a one-time code.

use std::net::SocketAddr;

use axum::{
    Router,
    extract::{ConnectInfo, State, ws::WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};

use crate::AppState;

/// `/link` and `/enroll`, for the hub's router.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/enroll", post(enroll_with_code))
        .route("/link", get(link_upgrade))
}

/// Machines dial in here; the device proves its key inside the socket, not with a bearer.
async fn link_upgrade(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let links = state.hub.links.clone();
    // Refuse at the door when too many handshakes are already in flight.
    let Some(permit) = links.handshake_permit() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "too many link handshakes").into_response();
    };
    upgrade
        .max_message_size(devagotchi_contracts::MAX_LINK_MESSAGE_BYTES)
        .max_frame_size(devagotchi_contracts::MAX_LINK_MESSAGE_BYTES)
        .on_upgrade(move |socket| links.serve(socket, peer, permit))
}

#[derive(serde::Deserialize)]
struct EnrollBody {
    name: String,
    code: String,
    public_key: String,
}

/// Bind a machine key to a one-time code from `machine_add`. The code is the secret.
async fn enroll_with_code(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<EnrollBody>,
) -> Response {
    // An account waiting to be let in enrolls nothing (`tenancy/gate.rs`).
    if let Err(error) = state.hub.links.may_enroll(&body.code).await {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": error.code, "message": error.message })),
        )
            .into_response();
    }
    match state
        .hub
        .links
        .enroll_owned(&body.name, &body.code, &body.public_key)
        .await
    {
        Ok(device) => (
            StatusCode::CREATED,
            axum::Json(serde_json::json!({
                "name": device.name,
                "owner": device.owner,
            })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": error.code, "message": error.message })),
        )
            .into_response(),
    }
}
