//! Tenancy: many people's spaces on one hub, run for others as a hosted service: who is let
//! in and on which plan, organisations with their members, teams, project roles and invites,
//! the machines a tenant holds, and what the operator reads about each tenant
//! (structure-cleanup S4, `.ai/patterns/layout.md`). Free-edition R4 makes this its own
//! crate outside the default build; a one-owner hub (`tenancy.mode single`) uses none of it.
//! The tenant itself and its row-level security scope stay with the accounts
//! (`crate::accounts::tenants`), since every hub has its home tenant.
//!
//! # Owns
//!
//! In the hub's Postgres: `tenancy_access` (migration 0008, each tenant's admission and plan,
//! `access`), the organisations' `tenancy_*` tables (`orgs`), and each tenant's chain
//! `tenancy_events` (written by `crate::accounts::tenants`).
//!
//! # Serves
//!
//! - The organisations API (`orgs::api`): organisations, members, teams, project roles,
//!   invites.
//! - The gate every MCP tool passes on a hosted hub: a waiting tenant is answered, a plan
//!   caps what a tenant holds (`gate`, `machines`).
//! - `/operator`: tenants, early access and plans (`operator`, `crate::web::operator`).
//!
//! # Settings
//!
//! - `tenancy.*` (`crate::config::tenancy`): mode, domain, unsecured quotas, idle days.
//! - Early access and plans: the site (`site.json`, `crate::site::plans`).
//!
//! # Depends on
//!
//! `crate::accounts` (accounts and tenants), `crate::store`, `crate::site`, `crate::config`,
//! `crate::machines`.

pub mod access;
pub mod gate;
pub mod machines;
pub mod operator;
pub mod orgs;

#[cfg(test)]
pub(crate) mod tests;
