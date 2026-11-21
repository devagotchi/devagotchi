//! The hub's tools on `/mcp`: its own (machines, operations) and each domain's router added to
//! them. rmcp lists tools by name, so the order they are added in changes nothing.

use rmcp::handler::server::router::tool::ToolRouter;

use super::Hub;

mod access;
mod access_caller;
mod machines;

pub(super) const ACCESS_INSTRUCTIONS: &str = "Machine access: use machines_list and capabilities_list, then request_access with a stable request_id and a reason of at least 20 characters. Use wait_for_access to read the human decision and my_grants for your current terms. A pending request or listed grant is not an execution permit. Check actual broker sandbox availability; unavailable confinement must be resolved before running. Never ask the operator for secrets.";

impl Hub {
    /// Every tool this hub serves: its own, and each domain's router added to them. Their
    /// answers' schemas are open to fields added later (`devagotchi_mcp::outputs`).
    pub(crate) fn router() -> ToolRouter<Self> {
        devagotchi_mcp::outputs::open_outputs(
            Self::machines_router()
                + Self::access_router()
                + Self::machine_router()
                + Self::board_router()
                + Self::workflow_router()
                + Self::offers_router()
                + Self::connections_router()
                + Self::words_router(),
        )
    }
}

/// Every tool the hub serves, as `tools/list` describes it.
#[cfg(test)]
pub(crate) fn tools() -> Vec<rmcp::model::Tool> {
    Hub::router().list_all()
}

/// The names of every tool the hub serves, for tests that hold a page's list to them.
#[cfg(test)]
pub(crate) fn tool_names() -> Vec<String> {
    tools()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}
