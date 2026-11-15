//! AgentHub in the hub. The board itself (tickets, leases, workflow, approvals, notes that
//! reach agents, the hosted Postgres board, GitHub issue records) is the `devagotchi-agenthub`
//! crate (`crates/agenthub`), re-exported here so the hub names it `crate::agenthub` as it
//! always has, its `board` command line included (`cli`, reaching the hub through the
//! account client, `account::client::AccountHub`). What stays in the hub is how the hub
//! serves it: its MCP tools (`mcp`), the tenant registry that opens hosted boards
//! (`registry`), the GitHub webhook and issue routes (`issue_lifecycle`). Its pages are in
//! `web/app/agenthub/`. Contract: `docs/agent-board.md`.

pub use devagotchi_agenthub::*;

pub mod issue_lifecycle;
pub mod mcp;
pub(crate) mod registry;

#[cfg(test)]
mod hosted_tests;
#[cfg(test)]
pub(crate) mod inbox_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
pub(crate) mod standing_tests;
#[cfg(test)]
mod tests;
