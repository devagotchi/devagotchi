//! Settings: the hub's own settings page, `/admin`, for its owner, and the settings it keeps
//! in Postgres: what each card sets, how it is saved, and the values in force after a save,
//! with no restart. What each setting is and how the values in force are worked out is
//! `crate::config::live`; the providers, mail server and signal sinks a card saves run in
//! their own domains.
//!
//! # Owns
//!
//! In the hub's Postgres: `hub_settings` (migration 0034, every setting of `/admin` that
//! `crate::config::live` lists, `store`), and the saved mail server in it with its password
//! sealed (`store::StoredMail`). The mail server in force, shared by every sender
//! (`web::mail_cell`).
//!
//! # Serves
//!
//! - `/admin`: the page (`web::admin`), its sign-in provider and ClickHouse cards
//!   (`/admin/sign-in/{name}`, `/admin/analytics`), the live settings' cards
//!   (`/admin/settings/{group}`, `web::live`), Email (`/admin/mail`, `web::mail`) and Signal
//!   sinks (`/admin/sinks/{name}`, `/admin/sinks/{name}/remove`, `web::sinks`). Only the owner
//!   sees it (`web::owner`); anyone else gets an unknown path's answer.
//!
//! # Settings
//!
//! Every setting here is set on `/admin`. A setting the config file or its environment
//! variable sets is shown there, not changed.
//!
//! # Depends on
//!
//! `crate::config` (what each setting is), `crate::identity` (the accounts store, the sign-in
//! providers and the sealing key), `crate::notify` (mail and signal sinks), `crate::ops`
//! (ClickHouse and the audit chain), `crate::store`, `crate::web` (the page kit).

pub mod store;
pub(crate) mod web;
