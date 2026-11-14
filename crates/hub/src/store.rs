//! The hub's store is `devagotchi-store` (`crates/store`): the database the hub and AgentHub
//! share, its migrations and its tenant isolation. Re-exported here so the hub's code names it
//! `crate::store` as it always has.

pub use devagotchi_store::*;
