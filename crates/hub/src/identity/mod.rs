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
pub(crate) mod bearer;
pub(crate) mod cli;
pub mod connect;
pub mod nomen;
pub mod preferences;
pub mod sealed;

/// What the rest of the hub names most: who is calling, the sealer of secrets at rest, and
/// each person's preferences.
pub use auth::Subject;
pub use preferences::PreferenceStore;
pub use sealed::Sealer;

/// The accounts of a hub whose state is in `state_dir`: in the Postgres `storage` names (see
/// `store/mod.rs`: the environment's URL, else the config's).
pub(crate) async fn accounts_at(
    state_dir: &std::path::Path,
    config: &crate::config::HubConfig,
) -> devagotchi_contracts::BridgeResult<accounts::Accounts> {
    crate::config::prepare_state_dir(state_dir)?;
    let url = crate::store::postgres_url(config.storage_url())?;
    let schema = crate::store::schema_for(config.storage_schema());
    let store = crate::store::open_in(&url, schema.as_deref()).await?;
    configured_accounts(store, config).await
}

pub(crate) async fn configured_accounts(
    store: crate::store::Store,
    config: &crate::config::HubConfig,
) -> devagotchi_contracts::BridgeResult<accounts::Accounts> {
    // A hosted hub on Postgres keeps its tenants apart in the database as well: its queries
    // run as a role row-level security holds, or it does not start.
    if config.multi_tenant() && !store.enforced() {
        return Err(devagotchi_contracts::BridgeError::new(
            "account_store",
            "a hosted hub (tenancy.mode multi) on Postgres runs its queries as the role devagotchi_hub, held to row-level security, and its login role cannot switch to that role; see crates/hub/README.md, \"The store\"",
        ));
    }
    Ok(accounts::Accounts::open(store)
        .await?
        .with_tokens(config.token_config())
        .with_tenancy(config.tenancy.clone(), config.operators.clone())
        .with_site(config.site_cell(), &config.public_url)
        .with_analytics(config.counts_visits())
        .with_extensions(config.extensions.clone()))
}

#[cfg(test)]
mod tests;
