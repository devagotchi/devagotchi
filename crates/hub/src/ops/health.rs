//! `/healthz`: whether the hub's brokers answer and its audit chain verifies (both counted),
//! and the state of everything reported beside them: links, the identity provider, downloads,
//! the store's row-level security, mail, the ClickHouse export and the signal sinks.

use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::AppState;

pub(crate) async fn healthz(State(state): State<AppState>) -> Response {
    let mut brokers = Vec::new();
    let mut all_up = true;
    for broker in &state.hub.config.brokers {
        match &broker.socket {
            Some(socket) => {
                let reachable = reachable(socket).await;
                all_up &= reachable;
                brokers.push(serde_json::json!({ "name": broker.name, "reachable": reachable }));
            }
            // A laptop that is asleep does not make the hub unhealthy; it is reported.
            None => brokers.push(serde_json::json!({
                "name": broker.name,
                "link": true,
                "connected": state.hub.links.connected(&broker.name),
            })),
        }
    }
    let audit = state.hub.audit.verify().await;
    let alert = state.hub.audit.write_failure_alert();
    let chain_ok = audit.as_ref().map(|status| status.ok).unwrap_or(false) && alert.is_none();
    // The identity provider is reported, not counted: an outage there leaves the hub, its
    // links and confirmed sign-ins working (see identity/nomen/).
    let mut body = serde_json::json!({
        "ok": all_up && chain_ok,
        "brokers": brokers,
        "audit": audit.ok(),
        "operator_alerts": alert.into_iter().collect::<Vec<_>>(),
        "issuer": state.verifier.issuer_health(),
        "downloads": match (&state.hub.downloads, &state.hub.config.downloads) {
            (Some(downloads), _) => serde_json::json!({ "version": downloads.version, "files": downloads.files().count() }),
            (None, Some(_)) => serde_json::json!({ "off": "the downloads directory did not check out; see the hub's log" }),
            (None, None) => serde_json::Value::Null,
        },
    });
    // The hub's own accounts: whether the database holds its queries to one tenant's rows
    // (Postgres row-level security, under a role that cannot bypass it).
    if let Some(passkeys) = &state.passkeys {
        body["store"] = serde_json::json!({ "tenant_rls": passkeys.tenant_rls() });
        // Whether it can send mail and how the last send went: reported, not counted, like
        // the issuer. Without mail people still sign in by password, passkey and provider.
        body["mail"] = passkeys.mail_status();
    }
    // The export to ClickHouse: reported, not counted. While ClickHouse is down the boards
    // work, the events wait in Postgres, and this says how many and since when.
    body["analytics"] = match state.hub.boards.warehouse() {
        Some(warehouse) => warehouse.health().await,
        None => serde_json::json!({ "configured": false }),
    };
    // The signal sinks: reported, not counted. One that refuses falls behind; nothing else does.
    body["signals"] = serde_json::Value::Array(
        state
            .hub
            .boards
            .sinks()
            .iter()
            .map(|(sink, _)| sink.health())
            .collect(),
    );
    let status = if all_up && chain_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, axum::Json(body)).into_response()
}

/// A broker is reachable when it answers a real request over its own IPC: the same
/// owner checks, framing and endpoint a routed call uses, on every platform.
pub(crate) async fn reachable(socket: &str) -> bool {
    let body = devagotchi_contracts::RequestBody::OperationList(devagotchi_contracts::ListInput {
        offset: 0,
        limit: 1,
        caller: None,
        scope: None,
    });
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            devagotchi_ipc::request(std::path::Path::new(socket), body)
        )
        .await,
        Ok(Ok(_))
    )
}
