//! The machine beside its files: the broker's settings, the machine's processes, and a web
//! address read from the machine, each one request to this user's broker.

use devagotchi_contracts::{
    BridgeError, ConfigGetInput, ConfigSetInput, ConfigView, ProcessKillInput, ProcessKilled,
    ProcessListInput, ProcessPage, RequestBody, ResponseBody, UrlContent, UrlReadInput, describe,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::BridgeAdapter;
use crate::unexpected;

#[tool_router(router = system_router, vis = "pub(super)")]
impl BridgeAdapter {
    #[tool(
        name = "get_config",
        description = describe::GET_CONFIG,
        title = "Read the broker's settings",
        annotations(title = "Read the broker's settings", read_only_hint = true, open_world_hint = false)
    )]
    async fn get_config(
        &self,
        Parameters(input): Parameters<ConfigGetInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ConfigView>, Json<BridgeError>> {
        match self.request(RequestBody::ConfigGet(input), context).await? {
            ResponseBody::Config(view) => Ok(Json(view)),
            other => Err(Json(unexpected("config", &other))),
        }
    }

    #[tool(
        name = "set_config_value",
        description = describe::SET_CONFIG_VALUE,
        title = "Change a broker limit",
        annotations(title = "Change a broker limit", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn set_config_value(
        &self,
        Parameters(input): Parameters<ConfigSetInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ConfigView>, Json<BridgeError>> {
        match self.request(RequestBody::ConfigSet(input), context).await? {
            ResponseBody::Config(view) => Ok(Json(view)),
            other => Err(Json(unexpected("config", &other))),
        }
    }

    #[tool(
        name = "list_processes",
        description = describe::LIST_PROCESSES,
        title = "List processes",
        annotations(title = "List processes", read_only_hint = true, open_world_hint = false)
    )]
    async fn list_processes(
        &self,
        Parameters(input): Parameters<ProcessListInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ProcessPage>, Json<BridgeError>> {
        match self
            .request(RequestBody::ProcessList(input), context)
            .await?
        {
            ResponseBody::Processes(page) => Ok(Json(page)),
            other => Err(Json(unexpected("processes", &other))),
        }
    }

    #[tool(
        name = "kill_process",
        description = describe::KILL_PROCESS,
        title = "Stop a process",
        annotations(title = "Stop a process", read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn kill_process(
        &self,
        Parameters(input): Parameters<ProcessKillInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ProcessKilled>, Json<BridgeError>> {
        match self
            .request(RequestBody::ProcessKill(input), context)
            .await?
        {
            ResponseBody::Killed(killed) => Ok(Json(killed)),
            other => Err(Json(unexpected("killed", &other))),
        }
    }

    #[tool(
        name = "read_url",
        description = describe::READ_URL,
        title = "Read a web address",
        annotations(title = "Read a web address", read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true)
    )]
    async fn read_url(
        &self,
        Parameters(input): Parameters<UrlReadInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<UrlContent>, Json<BridgeError>> {
        match self.request(RequestBody::UrlRead(input), context).await? {
            ResponseBody::Url(content) => Ok(Json(content)),
            other => Err(Json(unexpected("url", &other))),
        }
    }
}
