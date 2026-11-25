//! Typing into an interactive operation and resizing its terminal: an operation started
//! with `interactive` (operation_start) keeps its input open for these.

use devagotchi_contracts::{
    BridgeError, InputSent, OperationView, RequestBody, ResizeInput, ResponseBody, SendInput,
    describe,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::BridgeAdapter;
use crate::unexpected;

#[tool_router(router = terminal_router, vis = "pub(super)")]
impl BridgeAdapter {
    #[tool(
        name = "operation_input",
        description = describe::OPERATION_INPUT,
        title = "Type into a job",
        annotations(
            title = "Type into a job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn operation_input(
        &self,
        Parameters(input): Parameters<SendInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<InputSent>, Json<BridgeError>> {
        match self
            .request(RequestBody::OperationInput(input), context)
            .await?
        {
            ResponseBody::InputSent(sent) => Ok(Json(sent)),
            other => Err(Json(unexpected("input_sent", &other))),
        }
    }

    #[tool(
        name = "operation_resize",
        description = describe::OPERATION_RESIZE,
        title = "Resize a job's terminal",
        annotations(
            title = "Resize a job's terminal",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn operation_resize(
        &self,
        Parameters(input): Parameters<ResizeInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<OperationView>, Json<BridgeError>> {
        match self
            .request(RequestBody::OperationResize(input), context)
            .await?
        {
            ResponseBody::Operation(view) => Ok(Json(view)),
            other => Err(Json(unexpected("operation", &other))),
        }
    }
}
