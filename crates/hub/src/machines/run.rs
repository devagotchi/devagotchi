//! `run`: one call that starts a command on a machine, waits a bounded time for it, and
//! returns its output (box 1 of the 25 Oct order). It is the broker's own operation
//! underneath, so a command that outlasts the wait keeps running and stays observable with
//! the operation tools; nothing here owns a job.

use devagotchi_contracts::describe;

use devagotchi_contracts::{BridgeError, OperationState};
use devagotchi_mcp::run::{self as shared, Failed};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::mcp::Hub;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// The command line, as you would type it in a terminal on the machine: bash or sh on Linux,
    /// PowerShell on Windows.
    pub command: String,
    /// Directory to run in (absolute, `~/...`, or relative to the workspace); defaults to the workspace (your home directory unless
    /// set otherwise).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Seconds to wait for the result, 1 to 50 (default 30). A command still running then
    /// keeps running; you get its operation_id.
    #[serde(default)]
    pub wait_seconds: Option<u64>,
    /// Seconds the command may run before the broker stops it, 1 to 86400 (default 3600: an hour).
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RunOutput {
    pub machine: String,
    /// The broker's operation: follow it with operation_read, stop it with operation_cancel.
    pub operation_id: String,
    /// succeeded, failed, timed_out, cancelled, unknown; or running when the wait ended first.
    pub state: OperationState,
    pub exit_code: Option<i32>,
    /// stdout and stderr together, in the order the command wrote them. At most 32 KiB:
    /// when longer, the start is cut and `truncated` is true.
    pub output: String,
    pub truncated: bool,
    /// Present while the command is still running: how to follow it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// A broker from before waits existed rejects the request it cannot parse; say what to do.
fn outdated(error: BridgeError, machine: &str) -> BridgeError {
    if error.code == "invalid_request" && error.message.contains("operation_wait") {
        return BridgeError::new(
            "broker_outdated",
            format!(
                "{machine} runs a broker older than this hub; update it to use run. The command was started; follow it with operation_get and operation_read."
            ),
        );
    }
    error
}

#[tool_router(router = run_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "run",
        description = describe::RUN,
        title = "Run a command on a machine",
        annotations(
            title = "Run a command on a machine",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn run(
        &self,
        Parameters(input): Parameters<RunInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<RunOutput>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let wait = match shared::bounded(
            input.wait_seconds,
            shared::DEFAULT_WAIT_S,
            shared::MAX_WAIT_S,
            "wait_seconds",
        ) {
            Ok(value) => value,
            Err(error) => {
                return self
                    .recorded(&subject, "run", &input.machine, "", Err(error))
                    .await;
            }
        };
        let timeout = match shared::bounded(
            input.timeout_seconds,
            shared::DEFAULT_TIMEOUT_S,
            shared::MAX_TIMEOUT_S,
            "timeout_seconds",
        ) {
            Ok(value) => value,
            Err(error) => {
                return self
                    .recorded(&subject, "run", &input.machine, "", Err(error))
                    .await;
            }
        };
        let route = match self.route(&subject, &input.machine).await {
            Ok(route) => route,
            Err(Json(error)) => {
                self.record_completion(&subject, "run", &input.machine, "", &error.code)
                    .await;
                return Err(Json(error));
            }
        };
        let start = shared::start(
            format!("run-{}", hex::encode(crate::machines::random::<16>())),
            input.command,
            input.cwd,
            timeout,
            Self::caller(&subject, &context),
        );
        let ran = shared::run(
            |body| self.send_for_request("run", &route, body, &context),
            start,
            wait,
            context.ct.cancelled(),
        )
        .await;
        let ran = match ran {
            Ok(ran) => ran,
            Err(Failed::NotStarted(error)) => {
                self.record_completion(&subject, "run", &input.machine, "", &error.code)
                    .await;
                return Err(Json(error));
            }
            Err(Failed::Started {
                operation_id,
                error,
            }) => {
                let error = outdated(error, &input.machine);
                self.record_completion(&subject, "run", &input.machine, &operation_id, &error.code)
                    .await;
                return Err(Json(error));
            }
            Err(Failed::Cancelled { operation_id }) => {
                self.record_completion(
                    &subject,
                    "run",
                    &input.machine,
                    &operation_id,
                    "request_cancelled",
                )
                .await;
                return Err(Json(BridgeError::new(
                    "request_cancelled",
                    format!(
                        "stopped waiting; operation {operation_id} keeps running on {}",
                        input.machine
                    ),
                )));
            }
        };
        let running = !ran.state.terminal();
        self.record_completion(
            &subject,
            "run",
            &input.machine,
            &ran.operation_id,
            if running { "running" } else { "ok" },
        )
        .await;
        Ok(Json(RunOutput {
            next: running.then(|| {
                format!(
                    "Still running after {wait} s. Follow it with operation_wait or operation_read (machine {}, operation_id {}, cursor {}) or stop it with operation_cancel.",
                    input.machine, ran.operation_id, ran.cursor
                )
            }),
            machine: input.machine,
            operation_id: ran.operation_id,
            state: ran.state,
            exit_code: ran.exit_code,
            output: ran.output,
            truncated: ran.truncated,
        }))
    }
}
