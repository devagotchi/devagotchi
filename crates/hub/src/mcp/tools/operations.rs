//! The broker's handle-based tools, each routed to the machine the caller names: start an operation, get, wait on and read it, and cancel it.

use devagotchi_contracts::{
    BridgeError, OperationView, OutputPage, RequestBody, ResponseBody, StartOutput,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use crate::mcp::{
    hub::{Hub, unexpected_response},
    schema::{GetRouted, ReadRouted, StartRouted, WaitRouted},
};

#[tool_router(router = operation_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "operation_start",
        description = "Start a broker-owned operation on the named broker: a `command` line for the machine's shell (bash or sh on Linux, PowerShell on Windows) on a machine whose broker runs commands as its account, or an `executable` with exact `args` (on other machines it must be operator-granted). The broker validates the deadline and idempotency key; reuse the same key and inputs to reconcile a lost response.",
        title = "Run a program on a machine",
        annotations(
            title = "Run a program on a machine",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn operation_start(
        &self,
        Parameters(routed): Parameters<StartRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<StartOutput>, Json<BridgeError>> {
        match self
            .call(
                "operation_start",
                &routed.broker,
                RequestBody::OperationStart(routed.input),
                context,
            )
            .await?
        {
            ResponseBody::Start(output) => Ok(Json(output)),
            other => Err(unexpected_response("start", &other)),
        }
    }

    #[tool(
        name = "operation_get",
        description = "Read the named broker's retained state for an operation. This does not start, resume or cancel execution.",
        title = "Check an operation",
        annotations(
            title = "Check an operation",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_get(
        &self,
        Parameters(routed): Parameters<GetRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OperationView>, Json<BridgeError>> {
        match self
            .call(
                "operation_get",
                &routed.broker,
                RequestBody::OperationGet(routed.input),
                context,
            )
            .await?
        {
            ResponseBody::Operation(output) => Ok(Json(output)),
            other => Err(unexpected_response("operation", &other)),
        }
    }

    #[tool(
        name = "operation_wait",
        description = "Wait on the named broker until an operation finishes, or (with output_after, your read cursor) until it has new output, or until timeout_ms (at most 50000) passes. The broker answers the moment it changes; use this instead of calling operation_get or operation_read repeatedly. A wait that times out returns the operation unchanged: wait again.",
        title = "Wait for an operation",
        annotations(
            title = "Wait for an operation",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_wait(
        &self,
        Parameters(routed): Parameters<WaitRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OperationView>, Json<BridgeError>> {
        match self
            .call(
                "operation_wait",
                &routed.broker,
                RequestBody::OperationWait(routed.input),
                context,
            )
            .await?
        {
            ResponseBody::Operation(output) => Ok(Json(output)),
            other => Err(unexpected_response("operation", &other)),
        }
    }

    #[tool(
        name = "operation_read",
        description = "Read bounded retained output from the named broker after a caller-owned sequence cursor. Lossless base64 bytes, a text view, retention gaps and the operation state.",
        title = "Read an operation's output",
        annotations(
            title = "Read an operation's output",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_read(
        &self,
        Parameters(routed): Parameters<ReadRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OutputPage>, Json<BridgeError>> {
        match self
            .call(
                "operation_read",
                &routed.broker,
                RequestBody::OperationRead(routed.input),
                context,
            )
            .await?
        {
            ResponseBody::Output(output) => Ok(Json(output)),
            other => Err(unexpected_response("output", &other)),
        }
    }

    #[tool(
        name = "operation_cancel",
        description = "Explicitly request cancellation of a broker-owned operation on the named broker. Disconnecting never cancels.",
        title = "Cancel an operation",
        annotations(
            title = "Cancel an operation",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_cancel(
        &self,
        Parameters(routed): Parameters<GetRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OperationView>, Json<BridgeError>> {
        match self
            .call(
                "operation_cancel",
                &routed.broker,
                RequestBody::OperationCancel(routed.input),
                context,
            )
            .await?
        {
            ResponseBody::Operation(output) => Ok(Json(output)),
            other => Err(unexpected_response("operation", &other)),
        }
    }
}
