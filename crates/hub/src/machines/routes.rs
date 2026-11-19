//! The hub's one door for machines: `/link`, where each machine's `devagotchi link` holds its
//! WebSocket. A machine binds its key there too, by signing the hub's challenge with its
//! one-time code in the hello (`machines/link/enroll.rs` `admit`); there is no other way in.

use std::net::SocketAddr;

use axum::{
    Router,
    extract::{ConnectInfo, State, ws::WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};

use crate::AppState;

/// `/link`, for the hub's router.
pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/link", get(link_upgrade))
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
