//! Closed remote IPC vocabulary; local IPC remains its existing separate request shape.
use super::*;
use crate::{OperationState, RequestBody, ResponseBody};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteWireRequest {
    pub api_version: u32,
    pub remote: RemoteRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteRequest {
    Capabilities {},
    Prepare {
        origin: GrantOrigin,
        actor: GrantActor,
        level: GrantLevel,
        request: RequestBody,
    },
    Execute {
        authority: GrantAuthority,
        client: String,
        request: RequestBody,
    },
    Refresh {
        authority: GrantAuthority,
    },
    /// Read lifecycle evidence for the exact admitted identity. This confers no
    /// execution authority and never extends the broker's monotonic deadline.
    Status {
        binding: GrantBinding,
    },
    Interrupt {
        notice: GrantInterruption,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RemoteReply {
    Capabilities {
        version: u32,
        os: String,
        sandbox_levels: Vec<GrantLevel>,
    },
    Prepared {
        action: GrantAction,
        operation_key: String,
        input_sha256: String,
        call: GrantCall,
    },
    Executed(ResponseBody),
    Refreshed {
        operation_id: String,
    },
    Status {
        operation_id: String,
        state: OperationState,
        exit_code: Option<i32>,
    },
    Interrupted {
        count: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RemoteWireResponse {
    Ok(RemoteReply),
    Error(BridgeError),
}
