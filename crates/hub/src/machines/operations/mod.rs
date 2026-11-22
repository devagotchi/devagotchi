//! The broker's handle-based tools, each routed to the machine the caller names: start an operation, get, wait on and read it, and cancel it.

use devagotchi_contracts::describe;

use devagotchi_contracts::{
    BridgeError, GetInput, InputSent, ListInput, OperationView, OperationsPage, OutputPage,
    ReadInput, RequestBody, ResizeInput, ResponseBody, SendInput, StartInput, StartOutput,
    WaitInput,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use crate::mcp::{Hub, unexpected_response};

mod inputs;
use inputs::*;

#[tool_router(router = operations_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "operation_heartbeat",
        description = describe::OPERATION_HEARTBEAT,
        title = "Keep an interactive operation authorized",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn operation_heartbeat(
        &self,
        Parameters(input): Parameters<GetRouted>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<devagotchi_contracts::HeartbeatOutput>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result = tokio::select! {
            biased;
            _ = context.ct.cancelled() => Err(BridgeError::new("request_cancelled", "stopped waiting; a heartbeat already sent may have renewed permission")),
            result = self.heartbeat_granted(&input.machine, &input.operation_id, &context) => result,
        };
        let code = result
            .as_ref()
            .map_or_else(|error| error.code.as_str(), |_| "ok");
        self.record_completion(
            &subject,
            "operation_heartbeat",
            &input.machine,
            &input.operation_id,
            code,
        )
        .await;
        result.map(Json).map_err(Json)
    }

    #[tool(
        name = "operation_start",
        description = describe::OPERATION_START,
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
                &routed.machine,
                RequestBody::OperationStart(StartInput {
                    idempotency_key: routed.idempotency_key,
                    executable: routed.executable,
                    args: routed.args,
                    command: routed.command,
                    cwd: routed.cwd,
                    deadline_ms: routed.deadline_ms,
                    interactive: routed.interactive,
                    combined_output: routed.combined_output,
                    caller: None,
                }),
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
        description = describe::OPERATION_GET,
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
                &routed.machine,
                RequestBody::OperationGet(GetInput {
                    operation_id: routed.operation_id,
                    caller: None,
                }),
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
        description = describe::OPERATION_WAIT,
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
                &routed.machine,
                RequestBody::OperationWait(WaitInput {
                    operation_id: routed.operation_id,
                    output_after: routed.output_after,
                    timeout_ms: routed.timeout_ms,
                    caller: None,
                }),
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
        description = describe::OPERATION_READ,
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
                &routed.machine,
                RequestBody::OperationRead(ReadInput {
                    operation_id: routed.operation_id,
                    cursor: routed.cursor,
                    max_bytes: routed.max_bytes,
                    caller: None,
                }),
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
        description = describe::OPERATION_CANCEL,
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
                &routed.machine,
                RequestBody::OperationCancel(GetInput {
                    operation_id: routed.operation_id,
                    caller: None,
                }),
                context,
            )
            .await?
        {
            ResponseBody::Operation(output) => Ok(Json(output)),
            other => Err(unexpected_response("operation", &other)),
        }
    }
    #[tool(
        name = "operation_list",
        description = describe::OPERATION_LIST,
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
        let body = RequestBody::OperationList(ListInput {
            offset: routed.offset,
            limit: routed.limit,
            scope: routed.scope,
            caller: None,
        });
        match self
            .call("operation_list", &routed.machine, body, context)
            .await?
        {
            ResponseBody::Operations(page) => Ok(Json(page)),
            other => Err(unexpected_response("operations", &other)),
        }
    }

    #[tool(
        name = "operation_input",
        description = describe::OPERATION_INPUT,
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
        let body = RequestBody::OperationInput(SendInput {
            operation_id: routed.operation_id,
            text: routed.text,
            keys: routed.keys,
            take_over: routed.take_over,
            caller: None,
        });
        match self
            .call("operation_input", &routed.machine, body, context)
            .await?
        {
            ResponseBody::InputSent(sent) => Ok(Json(sent)),
            other => Err(unexpected_response("input_sent", &other)),
        }
    }

    #[tool(
        name = "operation_resize",
        description = describe::OPERATION_RESIZE,
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
        let body = RequestBody::OperationResize(ResizeInput {
            operation_id: routed.operation_id,
            cols: routed.cols,
            rows: routed.rows,
            take_over: routed.take_over,
            caller: None,
        });
        match self
            .call("operation_resize", &routed.machine, body, context)
            .await?
        {
            ResponseBody::Operation(view) => Ok(Json(view)),
            other => Err(unexpected_response("operation", &other)),
        }
    }
}
