//! A bounded ask of the broker itself, shared by `devagotchi mcp` (`broker_capabilities`) and
//! the hub (`machine_inspect`). A live link, or an answer of another kind or API version, never
//! stands in for the broker's own capabilities; its errors pass through as they came.

use std::{future::Future, time::Duration};

use devagotchi_contracts::{API_VERSION, BridgeError, BridgeResult, ResponseBody};

/// Longest an inspection waits for the broker.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// `request`'s answer when it is the broker's capabilities in this API version, within
/// [`TIMEOUT`]; `inspection_timeout` when it does not come in time.
pub async fn response(
    request: impl Future<Output = BridgeResult<ResponseBody>>,
) -> BridgeResult<ResponseBody> {
    let body = tokio::time::timeout(TIMEOUT, request)
        .await
        .map_err(|_| {
            BridgeError::new(
                "inspection_timeout",
                "The broker did not answer within 3 seconds, so whether it is ready is unknown. Nothing was started.",
            )
        })??;
    match &body {
        ResponseBody::Capabilities(capabilities) if capabilities.api_version == API_VERSION => {
            Ok(body)
        }
        ResponseBody::Capabilities(capabilities) => Err(BridgeError::new(
            "api_version",
            format!(
                "The broker speaks API {} and this side {API_VERSION}, so whether it is ready is unknown.",
                capabilities.api_version
            ),
        )),
        _ => Err(crate::unexpected("capabilities", &body)),
    }
}

#[cfg(test)]
mod tests;
