//! Who a broker is told is calling (the signed-in subject, in this MCP session), and the
//! tools that depend on it: listing a machine's operations (the caller's account's, or this
//! conversation's), typing into an interactive one and resizing its terminal.

use devagotchi_contracts::{BridgeError, Caller, RequestBody, ResponseBody};
use rmcp::{RoleServer, service::RequestContext};

use super::{Hub, hub::Route};
use crate::identity::Subject;
mod read;
mod start;

/// The header a Streamable HTTP client repeats on every request of its MCP session.
const SESSION_HEADER: &str = "mcp-session-id";

/// `error` from a machine running `build`, when this hub hands machines `offered`: when the
/// two differ, say that an older machine may not understand what this hub sends (a link that
/// cannot read a request drops the call, which reads as the machine going offline).
pub(crate) fn outdated(
    build: Option<&str>,
    offered: Option<&str>,
    mut error: BridgeError,
) -> BridgeError {
    let skew = matches!(
        error.code.as_str(),
        "broker_offline" | "invalid_request" | "link_timeout"
    );
    if let (Some(build), Some(offered)) = (build, offered)
        && build != offered
        && skew
    {
        error.message.push_str(&format!(
            " This machine runs {build} and this hub hands out {offered}: a machine older than its hub may not understand its requests. Update it with machine_update, then try again."
        ));
    }
    error
}

impl Hub {
    /// `send`, saying when a failure is likely the machine's build being older than the hub's.
    pub(crate) async fn send_hinted(
        &self,
        route: &Route,
        body: RequestBody,
    ) -> Result<ResponseBody, BridgeError> {
        let build = match route {
            Route::Link(device) => self.links.build(device),
            Route::Local(_) => None,
        };
        let offered = self
            .downloads
            .as_ref()
            .map(|downloads| downloads.version.as_str());
        self.send(route, body)
            .await
            .map_err(|error| outdated(build.as_deref(), offered, error))
    }

    /// The caller a broker is told about: `subject`'s account on this hub, and this MCP
    /// session as the one conversation within it. Never what a tool call said.
    pub(crate) fn caller(subject: &Subject, context: &RequestContext<RoleServer>) -> Caller {
        let session = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.headers.get(SESSION_HEADER))
            .and_then(|value| value.to_str().ok())
            .unwrap_or("no-session");
        Caller::hub(&subject.id, session)
    }
}

#[cfg(test)]
mod tests;
