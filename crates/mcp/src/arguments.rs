//! rmcp reports deserialization failures as protocol invalid params. For a known tool,
//! MCP requires a coded tool execution error. Unknown/hidden tools stay protocol errors.
use devagotchi_contracts::BridgeError;
use rmcp::model::{CallToolResponse, CallToolResult, ErrorCode};

pub fn coded(
    known: bool,
    result: Result<CallToolResponse, rmcp::ErrorData>,
) -> Result<CallToolResponse, rmcp::ErrorData> {
    match result {
        Err(error) if known && error.code == ErrorCode::INVALID_PARAMS => {
            Ok(CallToolResult::structured_error(
                serde_json::to_value(BridgeError::new("invalid_arguments", error.message))
                    .expect("BridgeError serializes"),
            )
            .into())
        }
        other => other,
    }
}
