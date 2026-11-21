//! The board's tools on the hub's `/mcp`, one per protocol verb, grouped by what they do
//! to a board. Each file has its own router; [`Hub::board_router`] joins them for the MCP
//! surface (`mcp/tools/mod.rs`), which lists tools by name.

use rmcp::handler::server::tool::ToolRouter;

use crate::mcp::Hub;

mod issues;
mod leases;
mod messages;
mod read;
mod settings;
mod tickets;

impl Hub {
    /// Every `board_*` and `approval_*` tool.
    pub(crate) fn board_router() -> ToolRouter<Hub> {
        Self::board_read_router()
            + Self::board_issue_router()
            + Self::board_ticket_router()
            + Self::board_lease_router()
            + Self::board_message_router()
            + Self::board_settings_router()
    }
}
