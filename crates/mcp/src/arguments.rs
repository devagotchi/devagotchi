//! Arguments serde refuses for a known tool answer as a coded tool execution error,
//! `invalid_arguments` (MCP 2025-11-25, G4), on both surfaces. rmcp 3.9 already turns that
//! failure into a tool error, but as plain text starting `failed to deserialize parameters:`
//! (`handler/server/router/tool.rs`); this gives it the code every other refusal carries. An
//! unknown or hidden tool stays a protocol error.
use devagotchi_contracts::BridgeError;
use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock, ErrorCode};

/// How rmcp begins the message of an argument deserialization failure.
const RMCP_ARGUMENT_FAILURE: &str = "failed to deserialize parameters:";

/// Why the arguments of a known tool were refused, when `result` is that refusal, in either
/// form rmcp may give it.
pub fn refused_arguments(
    known: bool,
    result: &Result<CallToolResponse, rmcp::ErrorData>,
) -> Option<String> {
    if !known {
        return None;
    }
    match result {
        Err(error) if error.code == ErrorCode::INVALID_PARAMS => Some(error.message.to_string()),
        Ok(CallToolResponse::Complete(done))
            if done.is_error == Some(true) && done.structured_content.is_none() =>
        {
            match done.content.as_slice() {
                [only] => only
                    .as_text()
                    .map(|text| text.text.as_str())
                    .filter(|text| text.starts_with(RMCP_ARGUMENT_FAILURE))
                    .map(str::to_owned),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `result`, with refused arguments of a known tool given the `invalid_arguments` code.
pub fn coded(
    known: bool,
    result: Result<CallToolResponse, rmcp::ErrorData>,
) -> Result<CallToolResponse, rmcp::ErrorData> {
    match refused_arguments(known, &result) {
        Some(why) => Ok(CallToolResult::structured_error(
            serde_json::to_value(BridgeError::new("invalid_arguments", why))
                .expect("BridgeError serializes"),
        )
        .into()),
        None => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(answer: &Result<CallToolResponse, rmcp::ErrorData>) -> Option<String> {
        match answer {
            Ok(CallToolResponse::Complete(done)) => done
                .structured_content
                .as_ref()
                .and_then(|value| value["code"].as_str())
                .map(str::to_owned),
            _ => None,
        }
    }

    #[test]
    fn refused_arguments_in_either_rmcp_form_get_the_code_and_nothing_else_does() {
        let as_text: Result<CallToolResponse, rmcp::ErrorData> =
            Ok(CallToolResult::error(vec![ContentBlock::text(
                "failed to deserialize parameters: unknown field `caller`",
            )])
            .into());
        let as_error = Err(rmcp::ErrorData::invalid_params(
            "failed to deserialize parameters: missing field `path`",
            None,
        ));
        for refused in [as_text, as_error] {
            let answer = coded(true, refused);
            assert_eq!(code(&answer).as_deref(), Some("invalid_arguments"));
        }

        // An unknown tool stays a protocol error.
        let unknown = coded(
            false,
            Err(rmcp::ErrorData::invalid_params("tool not found", None)),
        );
        assert!(unknown.is_err());
        // A tool's own refusal, and any other text error, keep what they said.
        let own: Result<CallToolResponse, rmcp::ErrorData> =
            Ok(CallToolResult::error(vec![ContentBlock::text("no such ticket")]).into());
        assert_eq!(code(&coded(true, own)), None);
    }
}
