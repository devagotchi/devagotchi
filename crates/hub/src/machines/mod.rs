//! Machines: the computers, servers and devspaces that reach this hub, and everything the hub
//! does to reach them (structure-cleanup S4, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! In the hub's Postgres (migration 0026, Postgres only): `machine_devices` (each machine's
//! name, key, owner, revocation) and `machine_enroll_codes` (one-time join codes, as SHA-256).
//! People's phones (`push_subscriptions`) are `crate::notify`'s. Nothing in the state directory.
//!
//! # Serves
//!
//! - `/link`: the WebSocket each machine's `devagotchi link` holds open (`link`).
//! - `/join/<code>` and `/join/<code>.ps1`: the one line that installs and joins a machine
//!   (`join`); `/dist`: the builds machines install and update from (`downloads`).
//! - MCP tools, through `crate::hub`: `machines`, `machine_add`, `machine_update`, `run`,
//!   `operation_*` and the file tools (`run`, `files`, `search`, `update`), and a machine's
//!   settings and processes (`system`), and the caller's own calls to them (`history`, from
//!   the audit); `machine_router` puts the routers here together.
//! - Pages: Machines and a machine's page (`crate::web::machines`, `crate::web::app::machine`).
//!
//! # Settings
//!
//! - `machines.auto_update` (`crate::config::machines`): set in the hub's config today; its
//!   page comes with free-edition U0.
//! - Brokers the hub reaches on its own host: the hub's config (deploy-time).
//!
//! # Depends on
//!
//! `crate::audit` (every call is recorded), `crate::push` (a machine going offline or joining
//! reaches its people's phones), `crate::config`, `crate::site` (the words), the `link` and
//! `contracts` crates.

pub(crate) mod cli;
pub mod devices;
pub mod downloads;
pub mod files;
pub mod history;
pub mod join;
pub mod link;
pub mod run;
pub mod search;
pub mod system;
pub mod update;

impl crate::mcp::Hub {
    /// The tools that work on one named machine: running commands, its files, searching them,
    /// and its settings and processes; and the caller's own calls to machines.
    pub(crate) fn machine_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::run_router()
            + Self::files_router()
            + Self::search_router()
            + Self::system_router()
            + Self::history_router()
    }
}

/// The hub's clock in Unix seconds and its random bytes, used well beyond machines; they
/// live here until a domain of their own takes them.
pub use devices::now_secs;
pub use link::random;
