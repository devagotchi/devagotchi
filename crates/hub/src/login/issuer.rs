//! Signing in at Nomen directly (`--issuer`): device authorization, refresh and revoke at an OpenID issuer.

use std::{io::Write, time::Duration};

use super::{
    Account, MAX_WAIT, Refusal, unix_now,
    wire::{DeviceAuthorization, OAuthError, TokenAnswer, UserInfo, read_json},
};
use crate::config::valid_subject;

pub(super) const SCOPE: &str =
    "openid profile email offline_access urn:nomen:iam:user:resourceowner";

pub(super) const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// The endpoints of one issuer, in Nomen's layout.
#[derive(Clone)]
pub struct Issuer {
    issuer: String,
    client_id: String,
    http: reqwest::Client,
}

impl Issuer {
    pub fn new(issuer: &str, client_id: &str) -> Result<Self, String> {
        let issuer = issuer.trim_end_matches('/');
        if !(issuer.starts_with("https://") || issuer.starts_with("http://127.0.0.1")) {
            return Err("issuer must be an https:// URL".into());
        }
        if client_id.trim().is_empty() {
            return Err("client id must not be empty".into());
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            issuer: issuer.to_string(),
            client_id: client_id.to_string(),
            http,
        })
    }

    /// Sign in: show the code, wait for the person, land the tokens. `tell` gets the
    /// lines a person reads (the code and the URL) as they are known.
    pub async fn login(&self, tell: &mut dyn Write) -> Result<Account, Refusal> {
        self.login_with_scope(SCOPE, tell).await
    }

    /// `login` asking for `scope` instead of the CLI's own: `hub connect-nomen` asks for
    /// Nomen's management API too, with the same code-on-any-device flow.
    pub async fn login_with_scope(
        &self,
        scope: &str,
        tell: &mut dyn Write,
    ) -> Result<Account, Refusal> {
        let started = unix_now();
        let auth: DeviceAuthorization = self
            .post(
                "/oauth/v2/device_authorization",
                &[("client_id", self.client_id.as_str()), ("scope", scope)],
            )
            .await?;
        let _ = writeln!(
            tell,
            "To sign in, open {} and enter the code {}",
            auth.verification_uri, auth.user_code
        );
        if let Some(complete) = &auth.verification_uri_complete {
            let _ = writeln!(tell, "  (or open {complete})");
        }
        let _ = writeln!(tell, "Waiting for you to confirm there…");
        let mut interval = Duration::from_secs(auth.interval.unwrap_or(5).max(1));
        let deadline = Duration::from_secs(auth.expires_in).min(MAX_WAIT);
        loop {
            tokio::time::sleep(interval).await;
            if Duration::from_secs(unix_now().saturating_sub(started)) > deadline {
                return Err(Refusal::Refused(
                    "the code expired before it was confirmed; run `devagotchi login` again".into(),
                ));
            }
            let answer: Result<TokenAnswer, Refusal> = self
                .post(
                    "/oauth/v2/token",
                    &[
                        ("grant_type", DEVICE_GRANT),
                        ("device_code", auth.device_code.as_str()),
                        ("client_id", self.client_id.as_str()),
                    ],
                )
                .await;
            match answer {
                Ok(tokens) => return self.account_from(tokens, unix_now()).await,
                Err(Refusal::Refused(code)) if code == "authorization_pending" => {}
                Err(Refusal::Refused(code)) if code == "slow_down" => {
                    interval += Duration::from_secs(5);
                }
                Err(Refusal::Refused(code)) if code == "access_denied" => {
                    return Err(Refusal::Refused("you declined the sign-in".into()));
                }
                Err(Refusal::Refused(code)) if code == "expired_token" => {
                    return Err(Refusal::Refused(
                        "the code expired before it was confirmed; run `devagotchi login` again"
                            .into(),
                    ));
                }
                Err(other) => return Err(other),
            }
        }
    }

    /// A fresh access token from the refresh token; the account keeps its identity.
    pub async fn refresh(&self, account: &Account) -> Result<Account, Refusal> {
        let Some(refresh_token) = account.refresh_token.as_deref() else {
            return Err(Refusal::Refused(
                "no refresh token on file; run `devagotchi login` again".into(),
            ));
        };
        let tokens: TokenAnswer = self
            .post(
                "/oauth/v2/token",
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                    ("client_id", self.client_id.as_str()),
                    ("scope", SCOPE),
                ],
            )
            .await?;
        let now = unix_now();
        Ok(Account {
            access_token: tokens.access_token,
            refresh_token: tokens
                .refresh_token
                .or_else(|| account.refresh_token.clone()),
            expires_at: now + tokens.expires_in.unwrap_or(3600),
            ..account.clone()
        })
    }

    /// Tell Nomen the tokens are done with (RFC 7009); best effort, the file goes either way.
    pub async fn revoke(&self, account: &Account) -> Result<(), Refusal> {
        for token in [
            Some(account.access_token.as_str()),
            account.refresh_token.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let response = self
                .http
                .post(format!("{}/oauth/v2/revoke", self.issuer))
                .form(&[("token", token), ("client_id", self.client_id.as_str())])
                .send()
                .await
                .map_err(|error| Refusal::Unavailable(format!("Nomen revoke: {error}")))?;
            if !response.status().is_success() {
                return Err(Refusal::Unavailable(format!(
                    "Nomen revoke answered {}",
                    response.status()
                )));
            }
        }
        Ok(())
    }

    async fn account_from(&self, tokens: TokenAnswer, now: u64) -> Result<Account, Refusal> {
        let response = self
            .http
            .get(format!("{}/oidc/v1/userinfo", self.issuer))
            .bearer_auth(&tokens.access_token)
            .send()
            .await
            .map_err(|error| Refusal::Unavailable(format!("Nomen userinfo: {error}")))?;
        if !response.status().is_success() {
            return Err(Refusal::Unavailable(format!(
                "Nomen userinfo answered {}",
                response.status()
            )));
        }
        let info: UserInfo = read_json(response).await?;
        valid_subject(&info.sub)
            .map_err(|why| Refusal::Unavailable(format!("Nomen's account id {why}")))?;
        Ok(Account {
            issuer: self.issuer.clone(),
            client_id: self.client_id.clone(),
            subject: info.sub,
            username: info.preferred_username.filter(|u| !u.is_empty()),
            email: info.email.filter(|e| !e.is_empty()),
            hub: None,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: now + tokens.expires_in.unwrap_or(3600),
            signed_in_at: now,
        })
    }

    /// A form POST to the issuer; an OAuth error body becomes `Refused(error code)`.
    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: &[(&str, &str)],
    ) -> Result<T, Refusal> {
        let response = self
            .http
            .post(format!("{}{path}", self.issuer))
            .form(form)
            .send()
            .await
            .map_err(|error| Refusal::Unavailable(format!("Nomen {path}: {error}")))?;
        let status = response.status();
        if status.is_success() {
            return read_json(response).await;
        }
        if status.is_client_error() {
            let error: OAuthError = read_json(response).await.unwrap_or_default();
            if error.error.is_empty() {
                return Err(Refusal::Unavailable(format!(
                    "Nomen {path} answered {status}"
                )));
            }
            return Err(Refusal::Refused(match error.error_description {
                Some(why)
                    if !matches!(
                        error.error.as_str(),
                        "authorization_pending" | "slow_down" | "access_denied" | "expired_token"
                    ) =>
                {
                    format!("{} ({why})", error.error)
                }
                _ => error.error,
            }));
        }
        Err(Refusal::Unavailable(format!(
            "Nomen {path} answered {status}"
        )))
    }
}
