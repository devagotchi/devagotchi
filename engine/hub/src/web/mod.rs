//! The shell of the hub's pages in a browser: the router that merges each domain's pages
//! (`routes.rs`; every domain keeps its own in its `web/`, such as `crate::identity::web`),
//! the page kit (`pages`), the phone app's frame and home (`app/`), the shared scripts and
//! styles (`assets/`, `style/`, `hub.css`), the bar every page wears and the checks every
//! page shares (`from_this_hub`, `safe_return_to`).
//!
//! People sign in, and see who they are signed in as and the machines they can reach, each
//! online or offline. A hub in `auth.mode hub` signs people in itself with passkeys
//! (`crate::identity::web::passkeys`); in `auth.mode nomen` with a `web` section, through
//! Nomen as below (`crate::identity::web::nomen`).
//!
//! Nomen sign-in is the OAuth 2.0 authorization code flow with PKCE (RFC 7636) against Nomen, as
//! a public client: no secret ships with the hub for it. The access token Nomen returns is
//! checked by the same introspection as every bearer token (accepted clients, audience,
//! exact account ID), so a browser session is never weaker than an MCP call.
//!
//! - The browser holds only a random session id; the hub keeps that id's SHA-256 and,
//!   sealed on disk, Nomen's tokens (`crate::identity::web::sessions`), so a restart signs nobody out.
//! - The hub asks for `offline_access`. With a refresh token from Nomen, the session lasts
//!   up to 90 days and ends after 30 days unused. The hub renews the access token through
//!   Nomen as it runs out, one refresh at a time, keeping the rotated refresh token.
//!   Nomen refusing a refresh ends the session. Without a refresh token, a session never
//!   outlives its access token or [`crate::identity::web::nomen::NO_REFRESH_MAX`].
//! - Every page re-checks the access token (Nomen's answer is reused for up to a minute),
//!   so an account disabled in Nomen loses its page within a minute.
//! - Signing out removes the session and revokes its refresh token at Nomen.
//! - `state` is bound to the browser with a short-lived cookie (login CSRF), is single-use,
//!   and the page to return to is kept on the hub, only ever a path on this hub.
//! - Cookies carry the `__Host-` prefix, `Secure`, `HttpOnly` and `SameSite=Lax`
//!   (plain names without `Secure` only when the hub runs on loopback http, for tests).
//! - Redirects are not followed when talking to Nomen.

pub(crate) mod app;
mod one_user;
pub(crate) mod pages;
pub(crate) mod routes;
pub(crate) mod style;
#[cfg(test)]
pub(crate) mod tests;

use axum::http::{HeaderMap, header};

/// Unix seconds as `YYYY-MM-DD HH:MM UTC`, as the hub's pages and log show times.
pub(crate) fn utc(secs: u64) -> String {
    pages::utc(secs)
}

pub use routes::router;

/// The person signed in on this browser, for routes outside the web pages (`orgs::api`).
pub(crate) async fn session_subject(
    state: &crate::AppState,
    headers: &axum::http::HeaderMap,
) -> Option<crate::identity::Subject> {
    match routes::who(state, headers).await {
        routes::Who::Person(subject) => Some(subject),
        _ => None,
    }
}

/// A browser change must name this hub's exact origin; fetch metadata cannot override it.
pub(crate) fn from_this_hub(headers: &axum::http::HeaderMap, public_url: &str) -> bool {
    let origin = crate::identity::web::passkeys::origin_of(public_url);
    origin.is_some()
        && headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) == origin.as_deref()
        && headers
            .get("sec-fetch-site")
            .is_none_or(|v| v == "same-origin")
}

/// Every web page's top bar says whether the browser is signed in: the account and a
/// one-tap Sign out, or Sign in back to this page (`pages::Bar`). On a hub waiting to be
/// claimed, nobody can sign in, so it offers nothing. The page is drawn with the hub's site
/// (`pages::SITE`).
///
/// It also marks a request from the signed-in operator of a hosted hub (`OperatorVisit`),
/// so the operator's own visits are not counted (`counted.rs`).
pub async fn bar(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let unclaimed = match &state.passkeys {
        Some(hub) => matches!(hub.claimable().await, Ok(true)),
        None => false,
    };
    let signed_in = match &state.passkeys {
        // An account nobody secured gets a line under the bar on every page.
        Some(hub) if !unclaimed => match hub.who(request.headers()).await {
            Ok(Some(subject)) => {
                if state.hub.config.multi_tenant() && state.hub.config.is_owner(&subject.id) {
                    request
                        .extensions_mut()
                        .insert(crate::ops::analytics::OperatorVisit);
                }
                Some(subject.provisional_until.is_some())
            }
            _ => None,
        },
        _ if routes::signed_in(&state, request.headers()).await => Some(false),
        _ => None,
    };
    // A workflow's page (`/go/<token>`) is the link alone: no Sign in, no nudge to.
    let workflow = request.uri().path().starts_with("/go/");
    // The app's own pages say who is signed in. The offline page is kept on the phone, so it
    // names nobody.
    let path = request.uri().path();
    let framed = path == "/approvals" || path == "/app" || path.starts_with("/app/");
    let named = path != "/app/offline" && path != "/app/attention";
    let me = if framed && named && signed_in.is_some() {
        match routes::who(&state, request.headers()).await {
            routes::Who::Person(subject) => {
                Some(crate::identity::web::me::identity(&state, &subject).await)
            }
            _ => None,
        }
    } else {
        None
    };
    let bar = if (state.web.is_none() && state.passkeys.is_none()) || unclaimed || workflow {
        pages::Bar::Plain
    } else if let Some(unsecured) = signed_in {
        pages::Bar::SignedIn { unsecured, me }
    } else {
        pages::Bar::SignedOut {
            return_to: safe_return_to(
                request
                    .uri()
                    .path_and_query()
                    .map(|p| p.as_str())
                    .unwrap_or("/account"),
            )
            .to_string(),
        }
    };
    let site = state.hub.config.site();
    pages::BAR
        .scope(bar, pages::SITE.scope(site, next.run(request)))
        .await
}

/// A path on this hub to return to after sign-in; anything else becomes `/account`.
pub fn safe_return_to(path: &str) -> &str {
    // Long enough for an OAuth authorization request coming back after sign-in.
    let ok = path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 4096
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.~?=&%".contains(&b));
    if ok { path } else { "/account" }
}

pub(crate) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    cookie_value_up_to(headers, name, 128)
}

/// A cookie's value when it is at most `max` bytes (a sealed sign-in's state is longer
/// than a session id).
pub(crate) fn cookie_value_up_to(headers: &HeaderMap, name: &str, max: usize) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.is_empty() && value.len() <= max)
}

/// Percent-encode a query value (RFC 3986 unreserved characters pass through).
pub(crate) fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
