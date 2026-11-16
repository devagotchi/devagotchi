//! Who a broker is told is calling (the signed-in subject, in this MCP session), and the
//! tools that depend on it: listing a machine's operations (the caller's account's, or this
//! conversation's), typing into an interactive one and resizing its terminal.

use devagotchi_contracts::{
    BridgeError, Caller, InputSent, ListInput, OperationView, OperationsPage, RequestBody,
    ResizeInput, ResponseBody, SendInput,
};
use devagotchi_mcp::{INPUT_DESCRIPTION, RESIZE_DESCRIPTION};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::surface::{Hub, Route, unexpected_response};
use crate::identity::Subject;

/// The header a Streamable HTTP client repeats on every request of its MCP session.
const SESSION_HEADER: &str = "mcp-session-id";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListRouted {
    /// Name of the enrolled broker that owns the target machine.
    pub broker: String,
    pub input: ListInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InputRouted {
    /// Name of the enrolled broker that owns the target machine.
    pub broker: String,
    pub input: SendInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResizeRouted {
    /// Name of the enrolled broker that owns the target machine.
    pub broker: String,
    pub input: ResizeInput,
}

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

#[tool_router(router = callers_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "operation_list",
        description = "List the named broker's operations, newest first, with their state: what your account's conversations started there, running now or before (also before a restart of the AI client or the machine). With scope `mine`, only what this conversation started. Another account's operations never show.",
        title = "List operations",
        annotations(
            title = "List operations",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_list(
        &self,
        Parameters(routed): Parameters<ListRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OperationsPage>, Json<BridgeError>> {
        let body = RequestBody::OperationList(routed.input);
        match self
            .call("operation_list", &routed.broker, body, context)
            .await?
        {
            ResponseBody::Operations(page) => Ok(Json(page)),
            other => Err(unexpected_response("operations", &other)),
        }
    }

    #[tool(
        name = "operation_input",
        description = INPUT_DESCRIPTION,
        title = "Type into an operation",
        annotations(
            title = "Type into an operation",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn operation_input(
        &self,
        Parameters(routed): Parameters<InputRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<InputSent>, Json<BridgeError>> {
        let body = RequestBody::OperationInput(routed.input);
        match self
            .call("operation_input", &routed.broker, body, context)
            .await?
        {
            ResponseBody::InputSent(sent) => Ok(Json(sent)),
            other => Err(unexpected_response("input_sent", &other)),
        }
    }

    #[tool(
        name = "operation_resize",
        description = RESIZE_DESCRIPTION,
        title = "Resize an operation's terminal",
        annotations(
            title = "Resize an operation's terminal",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_resize(
        &self,
        Parameters(routed): Parameters<ResizeRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OperationView>, Json<BridgeError>> {
        let body = RequestBody::OperationResize(routed.input);
        match self
            .call("operation_resize", &routed.broker, body, context)
            .await?
        {
            ResponseBody::Operation(view) => Ok(Json(view)),
            other => Err(unexpected_response("operation", &other)),
        }
    }
}

#[cfg(test)]
mod tests;
