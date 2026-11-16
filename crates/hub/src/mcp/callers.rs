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

use super::surface::{Hub, unexpected_response};
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

impl Hub {
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
