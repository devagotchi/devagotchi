//! The bearer check every `/mcp` and API call passes: an API key or a token the verifier
//! accepts, else a `401` that points at the RFC 9728 protected-resource metadata this module
//! also serves. Refusals and identity-provider outages are logged for the operator, never the
//! token.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{AppState, identity::auth::AuthError, site, web};

pub(crate) async fn require_bearer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let origin = state.hub.config.public_url.trim_end_matches('/');
    let (metadata_url, scope) = {
        let scope = match state.hub.config.auth.nomen() {
            _ if state.hub.config.has_accounts() => {
                format!(", scope=\"{}\"", web::oauth::SCOPES.join(" "))
            }
            Some(nomen) => format!(
                ", scope=\"{}\"",
                crate::identity::auth::sign_in_scopes(nomen.audiences)
            ),
            None => String::new(),
        };
        (
            format!("{origin}/.well-known/oauth-protected-resource"),
            scope,
        )
    };
    let metadata = format!("Bearer resource_metadata=\"{metadata_url}\"{scope}");
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .map(str::to_string);
    let Some(token) = token else {
        return challenge(StatusCode::UNAUTHORIZED, &metadata, None);
    };
    // An API key reaches this endpoint only, and only with the limits it was made with.
    if token.starts_with("dgk_") {
        let found = match &state.passkeys {
            Some(hub) => hub.accounts().verify_key(&token).await,
            None => Ok(None),
        };
        return match found {
            Ok(Some((subject, limits))) => {
                request.extensions_mut().insert(subject);
                request.extensions_mut().insert(limits);
                next.run(request).await
            }
            Ok(None) => {
                log_refusal("unknown, revoked or expired API key");
                challenge(StatusCode::UNAUTHORIZED, &metadata, Some("invalid_token"))
            }
            Err(error) => {
                log_unavailable(&error.message);
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::RETRY_AFTER, "5")],
                    "the hub's store did not answer",
                )
                    .into_response()
            }
        };
    }
    match state.verifier.verify(&token).await {
        Ok(subject) => {
            request.extensions_mut().insert(subject);
            next.run(request).await
        }
        Err(AuthError::Invalid(why)) => {
            log_refusal(why);
            challenge(StatusCode::UNAUTHORIZED, &metadata, Some("invalid_token"))
        }
        Err(AuthError::Unavailable(why)) => {
            log_unavailable(&why);
            // Clients retry on their own; say when, and why, so an outage reads as one.
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "5")],
                format!("identity provider unavailable: {why}"),
            )
                .into_response()
        }
    }
}

/// An identity-provider outage, for the operator: the verifier logs when it starts and
/// ends; per request, at most one line every 30 s with how many were left out.
pub(crate) fn log_unavailable(why: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    static SUPPRESSED: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let last = LAST.load(Ordering::Relaxed);
    if now >= last + 30
        && LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        let suppressed = SUPPRESSED.swap(0, Ordering::Relaxed);
        if suppressed > 0 {
            eprintln!(
                "devagotchi hub: issuer unavailable: {why} ({suppressed} more since the last line)"
            );
        } else {
            eprintln!("devagotchi hub: issuer unavailable: {why}");
        }
    } else {
        SUPPRESSED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Why a token was refused, for the operator: a wrong audience or client in the config
/// looks to the caller exactly like a bad token. At most one line a second, never the token.
pub(crate) fn log_refusal(why: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    static SUPPRESSED: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let last = LAST.load(Ordering::Relaxed);
    if now > last
        && LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        let quiet = SUPPRESSED.swap(0, Ordering::Relaxed);
        let more = if quiet > 0 {
            format!(" ({quiet} more refused since the last line)")
        } else {
            String::new()
        };
        eprintln!("devagotchi hub: refused a bearer token: {why}{more}");
    } else {
        SUPPRESSED.fetch_add(1, Ordering::Relaxed);
    }
}

fn challenge(status: StatusCode, metadata: &str, error: Option<&str>) -> Response {
    let value = match error {
        Some(error) => format!("{metadata}, error=\"{error}\""),
        None => metadata.to_string(),
    };
    let mut response = (status, "authentication required").into_response();
    if let Ok(header_value) = HeaderValue::from_str(&value) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, header_value);
    }
    response
}

pub(crate) async fn protected_resource(State(state): State<AppState>) -> Response {
    let resource = state
        .hub
        .config
        .public_url
        .trim_end_matches('/')
        .to_string();
    resource_metadata(&state, resource)
}

/// RFC 9728 §3.1: the metadata at `/.well-known/oauth-protected-resource/mcp` is for the
/// resource `<public_url>/mcp`, the URL an MCP client was given.
pub(crate) async fn protected_resource_mcp(State(state): State<AppState>) -> Response {
    let resource = format!("{}/mcp", state.hub.config.public_url.trim_end_matches('/'));
    resource_metadata(&state, resource)
}

fn resource_metadata(state: &AppState, resource: String) -> Response {
    let Some(issuer) = state.verifier.issuer() else {
        return (StatusCode::NOT_FOUND, "no authorization server in dev mode").into_response();
    };
    let mut body = serde_json::json!({
        "resource": resource,
        "authorization_servers": [issuer],
        "bearer_methods_supported": ["header"],
    });
    resource_documents(
        &mut body,
        &state.hub.config.site(),
        &state.hub.config.public_url,
    );
    if state.hub.config.has_accounts() {
        // The hub is its own authorization server (32b).
        body["scopes_supported"] = serde_json::json!(web::oauth::SCOPES);
    } else if let Some(nomen) = state.hub.config.auth.nomen() {
        // MCP clients ask for exactly these (RFC 9728 `scopes_supported`), so the account's
        // org reaches the hub with every call.
        body["scopes_supported"] = crate::identity::auth::sign_in_scopes(nomen.audiences)
            .split(' ')
            .collect::<Vec<_>>()
            .into();
    }
    let mut response = axum::Json(body).into_response();
    web::oauth::allow_any_origin(&mut response);
    response
}

/// RFC 9728's human-readable fields: the product's name, and where its docs, privacy
/// policy and terms are on this hub.
pub(crate) fn resource_documents(
    body: &mut serde_json::Value,
    site: &site::Site,
    public_url: &str,
) {
    let hub = public_url.trim_end_matches('/');
    body["resource_name"] = site.name().into();
    body["resource_documentation"] = format!("{hub}/docs").into();
    body["resource_policy_uri"] = format!("{hub}/privacy").into();
    body["resource_tos_uri"] = format!("{hub}/terms").into();
}
