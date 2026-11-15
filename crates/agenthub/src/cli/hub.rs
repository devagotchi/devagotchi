//! The hub the command line reaches as the signed-in account: what `board bind` asks and a
//! bound project's commands run on (B11). AgentHub defines it and the host gives it: the
//! `devagotchi` binary passes the hub's account client (`account.json`, the device sign-in).
//! Without one, a bound project is refused as not signed in.

use std::path::Path;

use devagotchi_contracts::{BridgeError, BridgeResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

/// How the command line finds and reaches a hub.
pub trait Hub: Send + Sync {
    /// The hub a command means when it names none: the environment's, else the last sign-in's.
    fn default_hub(&self, account_file: &Path) -> Option<String>;

    /// A session on `hub` as the account in `account_file`, as its agent `seat` when given.
    fn connect(
        &self,
        hub: &str,
        account_file: &Path,
        seat: Option<&str>,
    ) -> BridgeResult<Box<dyn Session>>;
}

/// One session on a hub: its MCP tools by name.
pub trait Session {
    /// The hub's address.
    fn hub(&self) -> &str;

    /// Call the tool `name` with `arguments`; its structured answer.
    fn call_tool(&mut self, name: &str, arguments: Value) -> BridgeResult<Value>;
}

/// Call a tool and read its answer as `T`.
pub fn call<T: DeserializeOwned>(
    session: &mut dyn Session,
    name: &str,
    arguments: Value,
) -> BridgeResult<T> {
    let value = session.call_tool(name, arguments)?;
    serde_json::from_value(value).map_err(|error| {
        BridgeError::new(
            "hub_error",
            format!("{name}: the hub's answer is not the record expected: {error}"),
        )
    })
}

/// What `board_attach` answers: the configured board a project is attached to, by name, with
/// the prefix its ticket ids carry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Attached {
    pub project: String,
    pub prefix: String,
}
