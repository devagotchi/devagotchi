//! Identity: who someone is on this hub and how they prove it: accounts, sign-in (email and
//! password, email links, Google, GitHub, OpenID Connect, Sign in with Shippin through Nomen,
//! passkeys), browser sessions, the OAuth server AI apps sign in with, API keys, and what
//! every request is checked against (structure-cleanup S4, `.ai/patterns/layout.md`).
//!
//! # Owns
//!
//! In the hub's Postgres, every `auth_` and `oauth_` table: `auth_accounts`,
//! `auth_account_starts`, `auth_identities`, `auth_passwords`, `auth_passkeys`,
//! `auth_sessions`, `auth_web_sessions`, `auth_links`, `auth_email_links`, `auth_email_sends`,
//! `auth_claim_codes`, `auth_api_keys`, `auth_preferences`, `auth_sign_in_settings` and
//! `auth_nomen_sign_ins` (`accounts`, `nomen`, `preferences`); `oauth_clients`,
//! `oauth_codes`, `oauth_devices`, `oauth_tokens` (`accounts::oauth`). The settings key that
//! seals secrets at rest is in the state directory (`sealed`).
//!
//! # Serves
//!
//! - Every request's subject: bearer tokens, API keys and sessions checked (`auth`).
//! - Sign-in and the account: `/auth/login`, `/account`, `/signed-out`, `/owner`, `/claim`,
//!   `/passkey/*`, `/device` (the device grant), the providers' callbacks
//!   (`crate::web::sign_in`, `crate::web::passkeys`, `crate::web::claim`, `crate::web::device`).
//! - The OAuth 2.1 server for AI apps: `/oauth/*`, `/.well-known/oauth-*` (`accounts::oauth`).
//! - `devagotchi hub owner` and `devagotchi hub connect-nomen` (`crate::lib`).
//!
//! # Settings
//!
//! - Sign-in providers: `/admin` → Sign-in (`accounts::sign_in_settings`); `auth.*` in the
//!   hub's config (mode, email, Nomen, tokens).
//! - `settings.key` (sealed secrets): the state directory, deploy-time.
//!
//! # Depends on
//!
//! `crate::store`, `crate::tenancy` (admission and plans on a hosted hub), `crate::site` (the
//! words), `crate::notify::mail` (sign-in links), `crate::ops` (milestones), `crate::config`.

pub mod accounts;
pub mod auth;
pub mod connect;
pub mod nomen;
pub mod preferences;
pub mod sealed;

/// What the rest of the hub names most: who is calling, the sealer of secrets at rest, and
/// each person's preferences.
pub use auth::Subject;
pub use preferences::PreferenceStore;
pub use sealed::Sealer;

#[cfg(test)]
mod tests;
