//! Ops: what the hub records about itself and reports to its operator: the audit chain of
//! every routed call, first-party analytics, and AgentHub's statistics from ClickHouse
//! (structure-cleanup S4, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! In the hub's Postgres: `audit_entries` (migration 0027, the hash chain, `audit`);
//! `analytics_events`, `analytics_views` and `analytics_daily` (migration 0009,
//! `analytics`); `analytics_settings` (the ClickHouse connection set on `/admin`, kept
//! through the account store, `analytics_settings`). The `hub_settings` table (every other
//! setting of `/admin`) is `crate::settings`'. Board events reach ClickHouse through
//! AgentHub's own outbox (`devagotchi_agenthub::clickhouse`, re-exported by `warehouse`).
//!
//! # Serves
//!
//! - `/healthz`: whether the audit chain verifies, and the ClickHouse export's state.
//! - `/api/v1/projects/{project}/stats`: a board's statistics from ClickHouse (`warehouse`).
//! - `/operator/analytics` and `/operator/analytics.csv`: the operator's numbers
//!   (`crate::ops::web::analytics`, counted by `crate::ops::web::counted`).
//! - `devagotchi hub analytics-resend`: every board event marked to be sent again.
//!
//! # Settings
//!
//! - ClickHouse (URL, database, user, password): `/admin` → AgentHub (`crate::settings::web::admin`).
//! - `analytics.retention_days`: `/admin` → Visits (`crate::config::live`, saved in
//!   `hub_settings`, migration 0034, by `crate::settings::store`). `analytics.enabled` is set at deploy.
//!
//! # Depends on
//!
//! `crate::store`, `crate::accounts` (tenants' milestones and the sealed ClickHouse password),
//! `crate::agenthub` (the boards whose events are exported), `crate::config`.

pub mod analytics;
pub mod analytics_settings;
pub mod audit;
pub(crate) mod health;
pub mod warehouse;

#[cfg(test)]
mod tests;
pub(crate) mod web;
