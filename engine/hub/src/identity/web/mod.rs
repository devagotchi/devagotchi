//! Identity's pages: claiming a new hub (`/claim`), signing in (passkeys, a password, an email
//! link, Google, GitHub, OpenID Connect, Nomen), the device grant (`/device`), the OAuth
//! server's browser steps (`/oauth/*`), and the account's own app pages: Me (`/app/me`), API
//! keys (`/app/keys`) and registered agents (`/app/agents`). The accounts and their rules are
//! in `crate::identity::accounts`; these pages show them and act through them.

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub(crate) mod agents;
pub(crate) mod claim;
pub(crate) mod device;
pub(crate) mod email;
pub(crate) mod keys;
pub(crate) mod me;
pub(crate) mod nomen;
pub mod oauth;
pub(crate) mod passkeys;
pub(crate) mod provider_apply;
pub(crate) mod providers;
pub(crate) mod session;
pub(crate) mod sessions;
pub(crate) mod sign_in;
pub(crate) mod sign_in_cards;

#[cfg(test)]
pub(crate) mod tests;

/// Sign-in and the account's own pages.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/owner", get(passkeys::owner_page))
        .route(
            "/owner/password",
            get(passkeys::owner_password_page).post(passkeys::owner_password),
        )
        .route(
            "/passkey/register/options",
            post(passkeys::register_options),
        )
        .route("/passkey/register", post(passkeys::register))
        .route("/passkey/sign-in/options", post(passkeys::sign_in_options))
        .route("/passkey/sign-in", post(passkeys::sign_in))
        .route("/assets/passkey.js", get(passkeys::script))
        .route(
            "/assets/owner-password.js",
            get(passkeys::owner_password_script),
        )
        .route("/auth/provider/link", post(providers::link))
        .route("/auth/provider/{name}", get(providers::start))
        .route("/auth/provider/{name}/callback", get(providers::callback))
        .route("/auth/email", get(email::page).post(email::ask))
        .route("/auth/email/verify", post(email::verify))
        .merge(sign_in::routes())
        .merge(agents::routes())
        .route("/app/me", get(me::page).post(me::save))
        .route("/app/keys", get(keys::page).post(keys::create))
        .route("/app/keys/{id}/revoke", post(keys::revoke))
        .route("/app/keys/{id}/rotate", post(keys::rotate))
}

/// The hub's own authorization server for AI clients and the browser claim of a new hub:
/// `/claim`, `/.well-known/oauth-authorization-server`, `/oauth/*` and `/device`
/// (`web::router`'s `hub_authorization`).
pub(crate) fn authorization_routes() -> Router<AppState> {
    Router::new()
        .route("/claim", get(claim::page).post(claim::claim))
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth::metadata).options(oauth::preflight),
        )
        .route(
            "/oauth/register",
            post(oauth::register).options(oauth::preflight),
        )
        .route(
            "/oauth/authorize",
            get(oauth::authorize).post(oauth::decide),
        )
        .route("/oauth/token", post(oauth::token).options(oauth::preflight))
        .route(
            "/oauth/revoke",
            post(oauth::revoke).options(oauth::preflight),
        )
        .route(
            "/oauth/device",
            post(device::start).options(oauth::preflight),
        )
        .route("/device", get(device::page).post(device::decide))
}
