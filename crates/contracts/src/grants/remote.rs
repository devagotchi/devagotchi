//! Closed remote IPC vocabulary; local IPC remains its existing separate request shape.
use super::*;
use crate::{OperationState, RequestBody, ResponseBody};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteWireRequest {
    pub api_version: u32,
    pub remote: RemoteRequest,
}
/// Short-lived permission to observe one immutable admission. This is deliberately
/// a different type from execution authority and cannot refresh a process clock.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantObservation {
    pub binding: GrantBinding,
    pub issued_at: i64,
    pub expires_at: i64,
}
impl GrantObservation {
    pub fn remaining_ms(&self, now: i64) -> BridgeResult<u64> {
        // Share the bounded lifetime validation, never execution-clock mutation.
        GrantAuthority {
            binding: self.binding.clone(),
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        }
        .remaining_ms(now)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteRequest {
    Capabilities {},
    /// Separate negotiation leaves existing process capability replies unchanged.
    FileCapabilities {},
    PrepareFiles {
        origin: GrantOrigin,
        actor: GrantActor,
        level: GrantLevel,
        invocation: FileInvocation,
    },
    ExecuteFiles {
        authority: GrantAuthority,
        client: String,
        invocation: FileInvocation,
        step: FileStep,
    },
    /// End a read invocation; this cannot execute or renew filesystem work.
    FinishFiles {
        binding: GrantBinding,
    },
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
    Observe {
        observation: GrantObservation,
        client: String,
        request: RequestBody,
    },
    Control {
        control: GrantControl,
        client: String,
        request: RequestBody,
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
    FileCapabilities {
        version: u32,
        sandbox_levels: Vec<GrantLevel>,
    },
    FileExecuted {
        operation_id: String,
        outcome: FileOutcome,
        replayed: bool,
    },
    FileFinished {
        operation_id: String,
        error_code: Option<String>,
    },
    FileStatus {
        operation_id: String,
        state: FileExecutionState,
    },
    Prepared {
        action: GrantAction,
        operation_key: String,
        input_sha256: String,
        call: GrantCall,
    },
    Executed(ResponseBody),
    Controlled(ResponseBody),
    Observed {
        response: ResponseBody,
        /// False only when an observation-bounded wait needs another permission check.
        ready: bool,
    },
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
