//! `auth`, `trusted_issuers` and `web`: how callers are authenticated (the hub's own
//! accounts, Nomen, an OpenID Connect issuer, or dev tokens for tests), the other issuers
//! whose tokens it accepts, and the Nomen browser sign-in.

use std::{collections::BTreeSet, path::Path};

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Deserialize;

use super::{EmailConfig, HubConfig, ProviderConfig, check_rp_id, loopback_http, url_host};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebConfig {
    /// The Nomen application people sign in through on the hub's pages: a User Agent app
    /// with PKCE and no secret, redirect URI `<public_url>/auth/callback`. It must also be
    /// in `auth.clients`: the hub checks the tokens it gets like any other token.
    pub client_id: String,
}

/// One more OpenID Connect issuer the hub trusts (`trusted_issuers`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedIssuer {
    /// Issuer URL exactly as the provider states it (`iss` claim). https only.
    pub issuer: String,
    /// Accepted `aud` values for this issuer's tokens.
    pub audiences: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, tag = "mode", rename_all = "snake_case")]
pub enum AuthConfig {
    /// Bearer tokens are JWTs issued by an OpenID Connect provider (Nomen).
    /// Discovery runs at start; keys come from the provider's JWKS.
    Oidc {
        /// Issuer URL exactly as the provider states it (`iss` claim).
        issuer: String,
        /// Accepted `aud` values: the hub's own client/project ids at the provider,
        /// and the dynamic-registration project when hosted clients register there.
        audiences: Vec<String>,
    },
    /// Shippin sign-in. Nomen (Shippin's identity provider) issues opaque access
    /// tokens, so the hub asks Nomen about each one (token introspection, RFC 7662),
    /// signing in to Nomen as its own API application with a private-key JWT.
    Nomen {
        /// The issuer URL exactly as that provider states it, e.g. `https://id.example.com`.
        /// Tokens are checked at `<issuer>/oauth/v2/introspect`.
        issuer: String,
        /// Nomen project ids whose tokens this hub accepts. Nomen reports a token active
        /// only when the hub's application is in its audience; the hub also requires one
        /// of these ids in `aud`.
        audiences: Vec<String>,
        /// Nomen client ids (applications) whose sign-ins the hub accepts, such as
        /// mcp.shippin.cloud's and shippin-bridge's. Any application in Nomen can ask for the
        /// hub's audience, and a person already signed in to Nomen can be handed a token by
        /// any of them, so the hub also requires the token's `client_id` to be one of these.
        clients: Vec<String>,
        /// Absolute path to the hub's API application key, the JSON file Nomen downloads
        /// (`type`, `keyId`, `key`, `clientId`). Owner-only (chmod 0600).
        key_file: String,
    },
    /// Static tokens from a file the hub owner controls (mode 0600). For tests only
    /// (D21): accepted when `public_url` is loopback http, refused for a hub people reach.
    Dev {
        /// Absolute path to a JSON object `{ "<token>": "<subject>" }`.
        tokens_file: String,
    },
    /// The hub's own accounts (D25, §14): people sign in on its pages with passkeys, and
    /// the hub works with nothing else deployed. Its owner is made on the hub's host with
    /// `devagotchi hub owner`. Nomen is an optional extra way in.
    Hub {
        /// The passkeys' relying party: the hub's host (the default) or a parent domain of
        /// it, so passkeys made here work on its sibling hosts.
        #[serde(default)]
        rp_id: Option<String>,
        /// Shippin sign-in as well: Nomen tokens at `/mcp` from accounts it knows.
        #[serde(default)]
        nomen: Option<NomenAuth>,
        /// The hub's own AI connections (32b): how long their tokens live, how many
        /// clients it keeps. Each has a default; set only what differs.
        #[serde(default)]
        tokens: TokenConfig,
        /// Sign in with Google, GitHub, or any OpenID Connect provider (Shippin's Nomen),
        /// besides a passkey (`config/providers.rs`).
        #[serde(default)]
        providers: Vec<ProviderConfig>,
        /// Sign-in links by email, over this mail server.
        #[serde(default)]
        email: Option<EmailConfig>,
    },
}

/// Lifetimes and limits of what a hub in `auth.mode hub` hands out itself (`auth.tokens`):
/// its AI connections (the plan's decision 4: a connection lasts 7 days unused) and the
/// claim code a hub with no owner prints when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TokenConfig {
    /// How long an access token is good for.
    pub access_secs: u64,
    /// How long a refresh token lasts unused: how long an AI connection lives without use.
    pub refresh_secs: u64,
    /// How long an authorization code may wait to be exchanged.
    pub code_secs: u64,
    /// Registered clients kept at once; past it, those never used make room first.
    pub max_clients: u64,
    /// How long the claim code a hub with no owner prints when it starts stays good.
    /// Restarting the hub prints a new one.
    pub claim_secs: u64,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            access_secs: 15 * 60,
            refresh_secs: 7 * 24 * 3600,
            code_secs: 2 * 60,
            max_clients: 10_000,
            claim_secs: 24 * 3600,
        }
    }
}

impl TokenConfig {
    fn check(&self) -> Result<(), String> {
        if !(60..=24 * 3600).contains(&self.access_secs) {
            return Err("auth.tokens.access_secs is 60 to 86400".into());
        }
        if self.refresh_secs < self.access_secs || self.refresh_secs > 366 * 24 * 3600 {
            return Err(
                "auth.tokens.refresh_secs is at least access_secs and at most a year".into(),
            );
        }
        if !(10..=600).contains(&self.code_secs) {
            return Err("auth.tokens.code_secs is 10 to 600".into());
        }
        if self.max_clients == 0 {
            return Err("auth.tokens.max_clients is at least 1".into());
        }
        if !(60..=7 * 24 * 3600).contains(&self.claim_secs) {
            return Err("auth.tokens.claim_secs is 60 to 604800".into());
        }
        Ok(())
    }
}

/// Nomen in hub mode: the same fields, checked the same way, as `auth.mode nomen`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NomenAuth {
    pub issuer: String,
    pub audiences: Vec<String>,
    pub clients: Vec<String>,
    pub key_file: String,
}

/// Nomen's settings, from either mode that has them.
pub struct NomenRef<'a> {
    pub issuer: &'a str,
    pub audiences: &'a [String],
    pub clients: &'a [String],
    pub key_file: &'a str,
}

impl AuthConfig {
    /// Nomen's settings: `auth.mode nomen`, or the `nomen` block of `auth.mode hub`.
    pub fn nomen(&self) -> Option<NomenRef<'_>> {
        match self {
            AuthConfig::Nomen {
                issuer,
                audiences,
                clients,
                key_file,
            }
            | AuthConfig::Hub {
                nomen:
                    Some(NomenAuth {
                        issuer,
                        audiences,
                        clients,
                        key_file,
                    }),
                ..
            } => Some(NomenRef {
                issuer,
                audiences,
                clients,
                key_file,
            }),
            _ => None,
        }
    }
}

/// A subject is the account ID the identity provider issues, which never changes. An
/// email or a display name can be shared by two accounts or change hands (Shippin once
/// had two active accounts with the same name), so neither is accepted where an ID belongs.
pub(crate) fn valid_subject(subject: &str) -> Result<(), String> {
    if subject.is_empty() || subject.len() > 256 {
        return Err(format!("{subject:?} must be 1..=256 characters"));
    }
    if subject.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "{subject:?} has spaces; use the account ID (sub), not a name"
        ));
    }
    if subject.contains('@') {
        return Err(format!(
            "{subject:?} is an email; use the account ID (sub), which never changes. `whoami` shows it"
        ));
    }
    Ok(())
}

impl NomenRef<'_> {
    /// The checks `auth.mode nomen` has always made, for its fields under `prefix`.
    fn check(&self, prefix: &str) -> Result<(), String> {
        if !self.issuer.starts_with("https://") && !loopback_http(self.issuer) {
            return Err(format!(
                "{prefix}.issuer must be an https URL (plain http only on this machine's loopback, for tests)"
            ));
        }
        if self.audiences.is_empty() || self.audiences.iter().any(|a| a.trim().is_empty()) {
            return Err(format!(
                "{prefix}.audiences must name at least one Nomen project id"
            ));
        }
        if self.clients.is_empty() || self.clients.iter().any(|c| c.trim().is_empty()) {
            return Err(format!(
                "{prefix}.clients must name the Nomen applications whose sign-ins the hub accepts"
            ));
        }
        if !Path::new(self.key_file).is_absolute() {
            return Err(format!("{prefix}.key_file must be an absolute path"));
        }
        Ok(())
    }
}

impl HubConfig {
    /// `auth` for its mode, then `trusted_issuers` against it.
    pub(super) fn check_auth(&self, env: &dyn Fn(&str) -> Option<String>) -> BridgeResult<()> {
        let invalid = |message: String| BridgeError::new("invalid_config", message);
        match &self.auth {
            AuthConfig::Oidc { issuer, audiences } => {
                if !issuer.starts_with("https://") {
                    return Err(invalid("auth.issuer must be an https URL".into()));
                }
                if audiences.is_empty() || audiences.iter().any(|a| a.trim().is_empty()) {
                    return Err(invalid(
                        "auth.audiences must name at least one audience".into(),
                    ));
                }
            }
            AuthConfig::Nomen { .. } => {
                if let Some(nomen) = self.auth.nomen() {
                    nomen.check("auth").map_err(invalid)?;
                }
                if self.owner.is_none() {
                    return Err(invalid(
                        "Nomen sign-in needs an owner: the account ID (sub) that can always reach every broker"
                            .into(),
                    ));
                }
            }
            AuthConfig::Dev { tokens_file } => {
                if !Path::new(tokens_file).is_absolute() {
                    return Err(invalid("auth.tokens_file must be an absolute path".into()));
                }
                // D21: dev tokens are for tests. A hub with an address people reach signs
                // them in with Nomen or an OpenID Connect issuer; a static token would be
                // a password in a file, on the network, with no one to revoke it.
                if !loopback_http(&self.public_url) {
                    return Err(invalid(format!(
                        "auth.mode dev is for tests (public_url on loopback http); a hub at {} signs people in with auth.mode nomen or oidc",
                        self.public_url
                    )));
                }
            }
            AuthConfig::Hub {
                rp_id,
                tokens,
                providers,
                email,
                ..
            } => {
                tokens.check().map_err(invalid)?;
                let mut names = BTreeSet::new();
                for provider in providers {
                    provider.check(env).map_err(invalid)?;
                    if !names.insert(provider.name()) {
                        return Err(invalid(format!(
                            "auth.providers: {} is listed twice",
                            provider.name()
                        )));
                    }
                }
                if let Some(email) = email {
                    email.check(env).map_err(invalid)?;
                }
                // Passkeys work only in a secure context: https, or this machine's loopback.
                if !self.public_url.starts_with("https://") && !loopback_http(&self.public_url) {
                    return Err(invalid(
                        "auth.mode hub needs an https public_url (plain http only on loopback, for tests)"
                            .into(),
                    ));
                }
                let host = url_host(&self.public_url)
                    .ok_or_else(|| invalid("public_url has no host".into()))?;
                if let Some(rp_id) = rp_id {
                    check_rp_id(rp_id, &host).map_err(invalid)?;
                }
                if let Some(nomen) = self.auth.nomen() {
                    nomen.check("auth.nomen").map_err(invalid)?;
                }
                if self.web.is_some() {
                    return Err(invalid(
                        "auth.mode hub serves its own sign-in pages; leave out web (its Nomen app is for Sign in with Shippin, which comes later)"
                            .into(),
                    ));
                }
                if let Some(owner) = &self.owner
                    && !crate::identity::accounts::valid_account_id(owner)
                {
                    return Err(invalid(format!(
                        "owner {owner:?} is not an account id: acct_ and 26 characters, or the subject an earlier identity provider issued (letters, digits, - and _)"
                    )));
                }
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for trusted in &self.trusted_issuers {
            let issuer = trusted.issuer.trim_end_matches('/');
            if !trusted.issuer.starts_with("https://") {
                return Err(invalid(format!(
                    "trusted_issuers: {} must be an https URL",
                    trusted.issuer
                )));
            }
            if trusted.audiences.is_empty() || trusted.audiences.iter().any(|a| a.trim().is_empty())
            {
                return Err(invalid(format!(
                    "trusted_issuers: {} must name at least one audience",
                    trusted.issuer
                )));
            }
            if matches!(&self.auth, AuthConfig::Dev { .. }) {
                return Err(invalid(
                    "trusted_issuers needs auth mode nomen or oidc: dev tokens are for tests"
                        .into(),
                ));
            }
            // A hub with only its own accounts takes no other issuer's tokens: their `sub`
            // would be compared with the hub's own account ids.
            if matches!(&self.auth, AuthConfig::Hub { nomen: None, .. }) {
                return Err(invalid(
                    "trusted_issuers needs auth.nomen in auth mode hub: until the hub issues its own tokens (32b), another issuer's would name its accounts"
                        .into(),
                ));
            }
            let primary = match &self.auth {
                AuthConfig::Oidc { issuer, .. } | AuthConfig::Nomen { issuer, .. } => {
                    Some(issuer.trim_end_matches('/'))
                }
                AuthConfig::Hub { .. } => self.auth.nomen().map(|n| n.issuer.trim_end_matches('/')),
                AuthConfig::Dev { .. } => None,
            };
            if primary == Some(issuer) || seen.contains(&issuer) {
                return Err(invalid(format!(
                    "trusted_issuers: {} is listed twice (or is auth.issuer)",
                    trusted.issuer
                )));
            }
            seen.push(issuer);
        }
        Ok(())
    }

    /// `web` needs Nomen sign-in, one of its clients, and https.
    pub(super) fn check_web(&self) -> BridgeResult<()> {
        let invalid = |message: String| BridgeError::new("invalid_config", message);
        if let Some(web) = &self.web {
            match &self.auth {
                AuthConfig::Nomen { clients, .. } => {
                    if web.client_id.trim().is_empty() || !clients.contains(&web.client_id) {
                        return Err(invalid(
                            "web.client_id must be one of auth.clients, so the tokens it issues are accepted"
                                .into(),
                        ));
                    }
                }
                _ => {
                    return Err(invalid("web sign-in needs auth mode nomen".into()));
                }
            }
            // Session cookies are Secure: over plain http they would never come back.
            if !self.public_url.starts_with("https://") && !loopback_http(&self.public_url) {
                return Err(invalid(
                    "web sign-in needs an https public_url (plain http only on loopback, for tests)"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    /// The sign-in providers and the email link's server, in hub mode.
    pub fn sign_ins(&self) -> (&[ProviderConfig], Option<&EmailConfig>) {
        match &self.auth {
            AuthConfig::Hub {
                providers, email, ..
            } => (providers, email.as_ref()),
            _ => (&[], None),
        }
    }

    /// Whether people sign in with the hub's own accounts (`auth.mode hub`).
    pub fn has_accounts(&self) -> bool {
        matches!(self.auth, AuthConfig::Hub { .. })
    }

    /// The lifetimes and limits of the hub's own AI connections (`auth.tokens`), in hub
    /// mode; the defaults otherwise.
    pub fn token_config(&self) -> TokenConfig {
        match &self.auth {
            AuthConfig::Hub { tokens, .. } => *tokens,
            _ => TokenConfig::default(),
        }
    }

    /// The passkeys' relying party, in hub mode: `auth.rp_id`, or the hub's own host.
    pub fn rp_id(&self) -> Option<String> {
        match &self.auth {
            AuthConfig::Hub {
                rp_id: Some(rp_id), ..
            } => Some(rp_id.clone()),
            AuthConfig::Hub { rp_id: None, .. } => url_host(&self.public_url),
            _ => None,
        }
    }
}
