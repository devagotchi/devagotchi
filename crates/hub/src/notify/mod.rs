//! Notify: how the hub reaches people and other systems when something happens: Web Push to
//! their phones and browsers, email, and the signal stream to the operator's sinks
//! (structure-cleanup S4, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! In the hub's Postgres: `push_subscriptions` (migration 0026, each person's phones and
//! browsers, `push::subscriptions`) and `signal_cursors` (each sink's position per source and
//! scope, `signals`). The VAPID key is in the state directory (0600), its public half served.
//!
//! # Serves
//!
//! - `/push/subscribe` and `/assets/push.js`: turning notifications on (`crate::web::notify`).
//! - Web Push: an approval waiting, a machine joining or going offline (`push`).
//! - Email over SMTP: sign-in links and the mail check (`mail`; `devagotchi hub check-mail`).
//! - Signals: batches of NDJSON to each configured HTTPS sink (`signals`, `docs/signals.md`).
//! - `/healthz`: `mail` and `signals`.
//!
//! # Settings
//!
//! - `auth.email` (the mail server, in the hub's config); checked by `hub check-mail`.
//! - `signals.sinks` (`crate::config`): the hub's config today; its page comes with
//!   free-edition U0.
//!
//! # Depends on
//!
//! `crate::store`, `crate::machines::devices` (the registry that holds the subscriptions'
//! store and moves them with a person's machines), `crate::ops::audit` (one of the signal
//! sources), `crate::config`.

pub mod mail;
pub mod push;
pub mod signal_sinks;
pub mod signals;

#[cfg(test)]
mod tests;
