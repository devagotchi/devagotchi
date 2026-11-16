//! A machine beside its files: its broker's settings (`get_config`, `set_config_value`) and its
//! processes (`list_processes`, `kill_process`). The broker on the machine decides what each
//! may do; the hub only names the machine and passes the request on.

use devagotchi_contracts::{
    BridgeError, ConfigGetInput, ConfigSetInput, ConfigView, ProcessKillInput, ProcessKilled,
    ProcessListInput, ProcessPage, RequestBody, ResponseBody, describe,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::{Hub, unexpected_response};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetConfigInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// One of `output_limit_bytes`, `max_operations`, `max_concurrent`.
    pub key: String,
    /// The new value, a whole number.
    pub value: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListProcessesInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Only processes whose name or command line contains this (case ignored).
    #[serde(default)]
    pub filter: Option<String>,
    /// Most processes to return, 1 to 1000 (default 200), busiest first.
    #[serde(default)]
    pub max_results: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KillProcessInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    pub pid: u32,
    /// Kill at once (SIGKILL) instead of asking it to stop (SIGTERM).
    #[serde(default)]
    pub force: bool,
}

#[tool_router(router = system_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "get_config",
        description = describe::GET_CONFIG,
        title = "Read a machine's broker settings",
        annotations(title = "Read a machine's broker settings", read_only_hint = true, open_world_hint = false)
    )]
    async fn get_config(
        &self,
        Parameters(input): Parameters<MachineInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ConfigView>, Json<BridgeError>> {
        let body = RequestBody::ConfigGet(ConfigGetInput {});
        match self
            .call("get_config", &input.machine, body, context)
            .await?
        {
            ResponseBody::Config(view) => Ok(Json(view)),
            other => Err(unexpected_response("config", &other)),
        }
    }

    #[tool(
        name = "set_config_value",
        description = describe::SET_CONFIG_VALUE,
        title = "Change a machine's broker limit",
        annotations(title = "Change a machine's broker limit", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn set_config_value(
        &self,
        Parameters(input): Parameters<SetConfigInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ConfigView>, Json<BridgeError>> {
        let body = RequestBody::ConfigSet(ConfigSetInput {
            key: input.key,
            value: input.value,
        });
        match self
            .call("set_config_value", &input.machine, body, context)
            .await?
        {
            ResponseBody::Config(view) => Ok(Json(view)),
            other => Err(unexpected_response("config", &other)),
        }
    }

    #[tool(
        name = "list_processes",
        description = describe::LIST_PROCESSES,
        title = "List a machine's processes",
        annotations(title = "List a machine's processes", read_only_hint = true, open_world_hint = false)
    )]
    async fn list_processes(
        &self,
        Parameters(input): Parameters<ListProcessesInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ProcessPage>, Json<BridgeError>> {
        let body = RequestBody::ProcessList(ProcessListInput {
            filter: input.filter,
            max_results: input.max_results.unwrap_or(200),
        });
        match self
            .call("list_processes", &input.machine, body, context)
            .await?
        {
            ResponseBody::Processes(page) => Ok(Json(page)),
            other => Err(unexpected_response("processes", &other)),
        }
    }

    #[tool(
        name = "kill_process",
        description = describe::KILL_PROCESS,
        title = "Stop a process on a machine",
        annotations(title = "Stop a process on a machine", read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn kill_process(
        &self,
        Parameters(input): Parameters<KillProcessInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ProcessKilled>, Json<BridgeError>> {
        let body = RequestBody::ProcessKill(ProcessKillInput {
            pid: input.pid,
            force: input.force,
        });
        match self
            .call("kill_process", &input.machine, body, context)
            .await?
        {
            ResponseBody::Killed(killed) => Ok(Json(killed)),
            other => Err(unexpected_response("killed", &other)),
        }
    }
}
