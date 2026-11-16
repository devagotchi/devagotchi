//! MCP: the hub's tools as AI clients call them, at `/mcp` (Streamable HTTP): the machines
//! and operations tools routed to each machine's broker, `whoami` and `machines`, with each
//! domain's own tools mounted on the same router (AgentHub's boards, workflows, offers,
//! connections) (free-edition R2, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! No tables. The router (`surface`), the tool list and each tool's output schema.
//!
//! # Serves
//!
//! - `/mcp`: every tool, authenticated by `crate::identity::auth`, each call audited
//!   (`crate::ops::audit`).
//! - The tool list the plugins in `plugins/` are held to (`tests`).
//!
//! # Settings
//!
//! - The brokers the hub reaches on its own host, and per-tool limits: the hub's config.
//!
//! # Depends on
//!
//! `crate::machines` (the link to each broker), `crate::identity` (who is calling),
//! `crate::ops` (audit, milestones), `crate::agenthub`, `crate::workflows`, `crate::offers`,
//! `crate::connections` (their tools), `crate::config`.

mod callers;
mod surface;

/// What the rest of the hub uses of the router.
pub use surface::{Hub, Machine, MachineUpdate};
#[cfg(test)]
pub(crate) use surface::{Route, tool_names, tools};
pub(crate) use surface::{join_line, unexpected_response};

#[cfg(test)]
pub(crate) mod output_schema;
#[cfg(test)]
mod tests;
