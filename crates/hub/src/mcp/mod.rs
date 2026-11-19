//! MCP: the hub's tools as AI clients call them, at `/mcp` (Streamable HTTP): the machines
//! and operations tools routed to each machine's broker, `whoami` and `machines`, with each
//! domain's own tools mounted on the same router (AgentHub's boards, workflows, offers,
//! connections) (free-edition R2, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! No tables. The `Hub` and its routing (`hub`), the server (`handler`), the tools (`tools/`), what
//! they take and return (`schema`), the tool list and each tool's output schema.
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
mod handler;
mod hub;
mod machines;
pub(crate) mod schema;
mod tools;

/// What the rest of the hub uses of the router.
pub use hub::Hub;
#[cfg(test)]
pub(crate) use hub::Route;
pub(crate) use hub::unexpected_response;
pub(crate) use machines::join_line;
pub use schema::{Machine, MachineUpdate};
#[cfg(test)]
pub(crate) use tools::{tool_names, tools};

#[cfg(test)]
pub(crate) mod output_schema;
#[cfg(test)]
mod tests;
