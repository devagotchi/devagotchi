//! How each AI client connects to a Devagotchi hub, as data.
//!
//! One JSON file per client under `catalog/<vendor>/<id>.json`, one schema generated from the
//! types here (`schemas/connector.v1.schema.json`), and one renderer. The hub's `/connect`
//! page, `GET /api/v1/connectors` and `devagotchi connect` all read this crate, so a client is
//! added in one file and appears everywhere. No network and no filesystem access: what a client
//! needs is decided by the data and the hub's address.
//!
//! `verification` says how far to trust an entry. `docs` means it was read from the vendor's
//! documentation on that date. `ci` means a real client connected in this repository's tests.

use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod catalog;
#[cfg(test)]
mod tests;

/// The schema's identifier, carried by every catalog file.
pub const SCHEMA: &str = "devagotchi.connector/v1";
/// What the catalog's text stands in for a server's name.
pub const SERVER_NAME: &str = "devagotchi";

/// Where a client runs, which is how a person looks for it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Surface {
    Cli,
    Ide,
    Desktop,
    Web,
    Api,
    AgentFramework,
}

impl Surface {
    pub const ALL: [Surface; 6] = [
        Surface::Cli,
        Surface::Ide,
        Surface::Desktop,
        Surface::Web,
        Surface::Api,
        Surface::AgentFramework,
    ];

    /// The heading a person looks under.
    pub fn title(self) -> &'static str {
        match self {
            Surface::Cli => "In your terminal",
            Surface::Ide => "In your editor",
            Surface::Desktop => "Desktop apps",
            Surface::Web => "On the web",
            Surface::Api => "Through an API",
            Surface::AgentFramework => "Your own agents",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    StreamableHttp,
    Sse,
    Stdio,
}

/// How a person or an agent sets it up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InstallKind {
    CliCommand,
    ConfigFile,
    UiSteps,
    ApiParameter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConfigFormat {
    Json,
    Toml,
    Yaml,
}

/// `null` everywhere means the vendor's documentation did not say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Auth {
    pub oauth: Option<bool>,
    pub dynamic_client_registration: Option<bool>,
    pub bearer_header: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigPaths {
    pub macos: Option<String>,
    pub linux: Option<String>,
    pub windows: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Install {
    pub kind: InstallKind,
    /// The exact command, with `{name}` and `{url}`.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub config_path: Option<ConfigPaths>,
    #[serde(default)]
    pub project_config_path: Option<String>,
    #[serde(default)]
    pub format: Option<ConfigFormat>,
    /// The exact configuration for a remote server, with `{name}` and `{url}`.
    #[serde(default)]
    pub snippet: Option<String>,
    /// Steps in the client's own interface, with `{name}` and `{url}`.
    #[serde(default)]
    pub ui_steps: Vec<String>,
}

/// How far an entry has been proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Proof {
    /// Read from the vendor's documentation.
    Docs,
    /// A real client connected in this repository's tests.
    Ci,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    pub level: Proof,
    /// `YYYY-MM-DD`.
    pub on: String,
    /// The vendor's pages this was read from.
    pub docs: Vec<String>,
    /// The test that proves it, when `level` is `ci`.
    #[serde(default)]
    pub test: Option<String>,
    /// What the documentation did not confirm.
    #[serde(default)]
    pub gaps: Option<String>,
}

/// One AI client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub schema: String,
    /// `[a-z0-9-]`, the file's name and what `devagotchi connect` takes.
    pub id: String,
    pub name: String,
    /// `[a-z0-9-]`, the folder it lives in.
    pub vendor: String,
    pub surface: Surface,
    /// Whether it can reach a hub over the network by itself. `null`: not documented.
    pub remote_mcp: Option<bool>,
    pub transports: Vec<Transport>,
    pub auth: Auth,
    pub install: Install,
    /// The project instruction file it reads.
    #[serde(default)]
    pub instructions_file: Option<String>,
    pub hooks: Option<bool>,
    /// A plan it needs for custom connectors.
    #[serde(default)]
    pub plan_limits: Option<String>,
    /// The lowercase names a note can be addressed to, and that its OAuth client name is
    /// matched against.
    pub aliases: Vec<String>,
    pub verification: Verification,
}

/// A connector with this hub's address filled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Rendered {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub surface: Surface,
    pub remote_mcp: Option<bool>,
    pub kind: InstallKind,
    pub command: Option<String>,
    pub snippet: Option<String>,
    pub config_path: Option<ConfigPaths>,
    pub project_config_path: Option<String>,
    pub format: Option<ConfigFormat>,
    pub steps: Vec<String>,
    pub instructions_file: Option<String>,
    pub plan_limits: Option<String>,
    pub proof: Proof,
    pub checked_on: String,
    pub docs: Vec<String>,
    pub gaps: Option<String>,
}

impl Connector {
    /// Exactly what a person does to connect this client to the hub at `mcp_url`.
    pub fn render(&self, mcp_url: &str) -> Rendered {
        let fill = |text: &String| {
            text.replace("{name}", SERVER_NAME)
                .replace("{url}", mcp_url)
        };
        Rendered {
            id: self.id.clone(),
            name: self.name.clone(),
            vendor: self.vendor.clone(),
            surface: self.surface,
            remote_mcp: self.remote_mcp,
            kind: self.install.kind,
            command: self.install.command.as_ref().map(fill),
            snippet: self.install.snippet.as_ref().map(fill),
            config_path: self.install.config_path.clone(),
            project_config_path: self.install.project_config_path.clone(),
            format: self.install.format,
            steps: self.install.ui_steps.iter().map(fill).collect(),
            instructions_file: self.instructions_file.clone(),
            plan_limits: self.plan_limits.clone(),
            proof: self.verification.level,
            checked_on: self.verification.on.clone(),
            docs: self.verification.docs.clone(),
            gaps: self.verification.gaps.clone(),
        }
    }
}

/// Every connector, in the catalog's order. A catalog file that does not read fails the
/// repository's tests, so one that reaches here reads.
pub fn all() -> &'static [Connector] {
    static CATALOG: OnceLock<Vec<Connector>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut connectors: Vec<Connector> = catalog::FILES
            .iter()
            .map(|(path, text)| {
                serde_json::from_str(text).unwrap_or_else(|error| panic!("{path}: {error}"))
            })
            .collect();
        connectors.sort_by(|a, b| (a.surface, &a.name).cmp(&(b.surface, &b.name)));
        connectors
    })
}

pub fn find(id: &str) -> Option<&'static Connector> {
    all().iter().find(|connector| connector.id == id)
}

/// The names an AI client answers to, from the name it registered with this hub: the catalog's
/// aliases that appear in that name as whole words, so `Claude Code` answers to `claude` and
/// `claude-code` but not to `claude-desktop`, and an `Authorized` client is not `zed`.
pub fn aliases_for_client(client_name: &str) -> Vec<String> {
    let words: String = client_name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let name = format!("-{words}-");
    let mut names: Vec<String> = Vec::new();
    for connector in all() {
        for alias in &connector.aliases {
            if name.contains(&format!("-{alias}-")) && !names.contains(alias) {
                names.push(alias.clone());
            }
        }
    }
    names
}

/// The JSON Schema every catalog file follows.
pub fn schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Connector)).expect("connector schema")
}
