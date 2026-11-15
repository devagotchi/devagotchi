//! Machines: the computers, servers and devspaces that reach this hub, and everything the hub
//! does to reach them (structure-cleanup S4, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! In the hub's Postgres (migration 0026, Postgres only): `machine_devices` (each machine's
//! name, key, owner, revocation) and `machine_enroll_codes` (one-time join codes, as SHA-256);
//! `push_subscriptions` (people's phones, `devices/push_subscriptions.rs`) until the notify
//! domain takes them. Nothing in the state directory.
//!
//! # Serves
//!
//! - `/link`: the WebSocket each machine's `devagotchi link` holds open (`link`).
//! - `/join/<code>` and `/join/<code>.ps1`: the one line that installs and joins a machine
//!   (`join`); `/dist`: the builds machines install and update from (`downloads`).
//! - MCP tools, through `crate::hub`: `machines`, `machine_add`, `machine_update`, `run`,
//!   `operation_*` and the file tools (`run`, `files`, `update`).
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

pub mod devices;
pub mod downloads;
pub mod files;
pub mod join;
pub mod link;
pub mod run;
pub mod update;

/// The hub's clock in Unix seconds and its random bytes, used well beyond machines; they
/// live here until a domain of their own takes them.
pub use devices::now_secs;
pub use link::random;
