use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use devagotchi_contracts::BridgeError;
use serde_json::{Value, json};

pub(super) type Reply = Result<Response, Failure>;
pub(super) struct Failure(pub BridgeError);

pub(super) fn invalid(message: impl Into<String>) -> Failure {
    Failure(BridgeError::new("invalid", message))
}

impl From<BridgeError> for Failure {
    fn from(error: BridgeError) -> Self {
        Self(error)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "invalid" | "invalid_settings" | "unknown_environment" | "usage" | "self_lockout" => {
                StatusCode::BAD_REQUEST
            }
            "unauthenticated" => StatusCode::UNAUTHORIZED,
            "not_granted" | "read_only" | "policy_denied" | "humans_only" | "not_owner"
            | "key_denied" | "early_access" | "people_only" | "agent_disabled" => {
                StatusCode::FORBIDDEN
            }
            "not_found" | "unknown_board" | "not_offered" => StatusCode::NOT_FOUND,
            "request_retired" => StatusCode::GONE,
            "precondition_failed" => StatusCode::PRECONDITION_FAILED,
            "too_large" => StatusCode::PAYLOAD_TOO_LARGE,
            "stale_fence"
            | "version_conflict"
            | "held_by_other"
            | "lease_expired"
            | "not_holder"
            | "wip_limit"
            | "blocked"
            | "distinct_required"
            | "dependency_cycle"
            | "not_held"
            | "source_conflict"
            | "parent_closed"
            | "request_conflict"
            | "already_exists"
            | "in_use"
            | "shared_workflow"
            | "not_claimable"
            | "not_completable"
            | "use_lease_ops"
            | "transition_not_allowed"
            | "approval_decide_only" => StatusCode::CONFLICT,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        };
        let error = if status == StatusCode::SERVICE_UNAVAILABLE {
            // Store errors may carry SQL details; keep them out of public responses.
            BridgeError::new(
                &self.0.code,
                "the board operation is unavailable; preserve the request key and ask the hub operator",
            )
        } else {
            self.0
        };
        (
            status,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"error":error})),
        )
            .into_response()
    }
}

pub(super) fn answer(value: Value, replayed: bool) -> Response {
    let mut response = ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response();
    if replayed {
        response
            .headers_mut()
            .insert("idempotent-replayed", "true".parse().unwrap());
    }
    response
}
