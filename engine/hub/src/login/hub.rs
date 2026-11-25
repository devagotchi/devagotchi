//! Signing in at a Devagotchi hub (`--hub`): its RFC 8414 metadata, the device login its `/device` page approves, refresh and revoke with the hub's built-in client.

use std::{io::Write, time::Duration};

use base64::Engine;

use super::{
    Account, MAX_WAIT, Refusal, unix_now,
    wire::{DeviceAuthorization, OAuthError, TokenAnswer, read_json},
};

pub(super) const DEVICE_GRANT_HUB: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// The CLI's client id at a hub: built in, no registration (`identity/web/device.rs`).
pub(super) const HUB_CLIENT: &str = "devagotchi-cli";

/// What the CLI asks a hub for: its MCP tools, and a refresh token.
pub(super) const HUB_SCOPE: &str = "mcp offline_access";

pub(super) fn hub_http() -> Result<reqwest::Client, Refusal> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| Refusal::Unavailable(error.to_string()))
}

/// The hub's authorization-server metadata (RFC 8414): where its device authorization,
/// token and revocation endpoints are, whichever auth mode it runs.
pub(super) async fn hub_metadata(
    http: &reqwest::Client,
    hub: &str,
) -> Result<serde_json::Value, Refusal> {
    let response = http
        .get(format!("{hub}/.well-known/oauth-authorization-server"))
        .send()
        .await
        .map_err(|error| Refusal::Unavailable(format!("hub {hub}: {error}")))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(Refusal::Refused(format!(
            "{hub} has no sign-in of its own (no authorization-server metadata)"
        )));
    }
    if !status.is_success() {
        return Err(Refusal::Unavailable(format!(
            "hub metadata answered {status}"
        )));
    }
    let metadata: serde_json::Value = read_json(response).await?;
    // RFC 8414 §3.3: the metadata is the hub's own only when it names that address.
    let issuer = metadata["issuer"]
        .as_str()
        .unwrap_or("")
        .trim_end_matches('/');
    if issuer != hub {
        return Err(Refusal::Refused(format!(
            "{hub} says its address is {issuer:?}; run `devagotchi login --hub {issuer}`"
        )));
    }
    Ok(metadata)
}

/// One endpoint the metadata names: on the hub itself, or the hub is not trusted with it.
pub(super) fn hub_endpoint(metadata: &serde_json::Value, hub: &str, name: &str) -> Option<String> {
    metadata[name]
        .as_str()
        .filter(|url| url.starts_with(&format!("{hub}/")))
        .map(str::to_string)
}

/// Sign in at a hub: an account at once where the hub opens them for agents
/// (`auth.mode devagotchi`, claimed later), otherwise device authorization (RFC 8628) at
/// the endpoints the hub's metadata names: the person approves the code on the hub's
/// `/device` page, signed in to their account (`auth.mode hub`).
pub(super) async fn hub_device_login(hub: &str, tell: &mut dyn Write) -> Result<Account, Refusal> {
    let hub = hub.trim_end_matches('/');
    let http = hub_http()?;
    let agent = http
        .post(format!("{hub}/auth/agent/start"))
        .form(&[("client_id", HUB_CLIENT), ("agent", "cli")])
        .send()
        .await
        .map_err(|error| Refusal::Unavailable(format!("hub {hub}: {error}")))?;
    if agent.status().is_success()
        && let Ok(started) = read_json::<serde_json::Value>(agent).await
        && let Some(access) = started["access_token"].as_str()
    {
        let subject = jwt_sub(access)
            .ok_or_else(|| Refusal::Unavailable("hub access token has no subject".into()))?;
        let claim = started["claim_code"].as_str().unwrap_or("");
        let url = started["claim_url"].as_str().unwrap_or("");
        let _ = writeln!(
            tell,
            "Signed in. To keep this account, open {url}\nCode: {claim}"
        );
        return Ok(Account {
            issuer: hub.to_string(),
            client_id: HUB_CLIENT.into(),
            subject,
            username: None,
            email: None,
            hub: Some(hub.to_string()),
            access_token: access.to_string(),
            refresh_token: started["refresh_token"].as_str().map(str::to_string),
            expires_at: unix_now() + started["expires_in"].as_u64().unwrap_or(3600),
            signed_in_at: unix_now(),
        });
    }
    let metadata = hub_metadata(&http, hub).await?;
    let (Some(device), Some(token)) = (
        hub_endpoint(&metadata, hub, "device_authorization_endpoint"),
        hub_endpoint(&metadata, hub, "token_endpoint"),
    ) else {
        return Err(Refusal::Refused(format!(
            "{hub} offers no sign-in from a terminal (no device authorization endpoint)"
        )));
    };
    let response = http
        .post(&device)
        .form(&[("client_id", HUB_CLIENT), ("scope", HUB_SCOPE)])
        .send()
        .await
        .map_err(|error| Refusal::Unavailable(format!("hub device authorization: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let error: OAuthError = read_json(response).await.unwrap_or_default();
        return Err(Refusal::Refused(if error.error.is_empty() {
            format!("hub device authorization answered {status}")
        } else {
            error.error
        }));
    }
    let auth: DeviceAuthorization = read_json(response).await?;
    match &auth.verification_uri_complete {
        Some(complete) => {
            let _ = writeln!(
                tell,
                "To sign in, open {complete}\n  and check it shows the code {} (or open {} and enter it)",
                auth.user_code, auth.verification_uri
            );
        }
        None => {
            let _ = writeln!(
                tell,
                "To sign in, open {} and enter the code {}",
                auth.verification_uri, auth.user_code
            );
        }
    }
    let _ = writeln!(tell, "Waiting for you to approve it there…");
    let started = unix_now();
    let mut interval = Duration::from_secs(auth.interval.unwrap_or(5).max(1));
    let deadline = Duration::from_secs(auth.expires_in).min(MAX_WAIT);
    loop {
        tokio::time::sleep(interval).await;
        if Duration::from_secs(unix_now().saturating_sub(started)) > deadline {
            return Err(Refusal::Refused(
                "the code expired before it was approved; run `devagotchi login` again".into(),
            ));
        }
        let response = http
            .post(&token)
            .form(&[
                ("grant_type", DEVICE_GRANT_HUB),
                ("device_code", auth.device_code.as_str()),
                ("client_id", HUB_CLIENT),
            ])
            .send()
            .await
            .map_err(|error| Refusal::Unavailable(format!("hub token: {error}")))?;
        let status = response.status();
        if status.is_success() {
            let tokens: TokenAnswer = read_json(response).await?;
            return hub_account(&http, hub, tokens).await;
        }
        let error: OAuthError = read_json(response).await.unwrap_or_default();
        match error.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            "access_denied" => {
                return Err(Refusal::Refused("you cancelled the sign-in".into()));
            }
            "expired_token" => {
                return Err(Refusal::Refused(
                    "the code expired before it was approved; run `devagotchi login` again".into(),
                ));
            }
            "" => return Err(Refusal::Unavailable(format!("hub token answered {status}"))),
            other => return Err(Refusal::Refused(other.to_string())),
        }
    }
}

/// The account a hub's tokens speak for: a signed token names it (`auth.mode devagotchi`);
/// an opaque one (`auth.mode hub`) is asked about at the hub's `/api/v1/me`.
pub(super) async fn hub_account(
    http: &reqwest::Client,
    hub: &str,
    tokens: TokenAnswer,
) -> Result<Account, Refusal> {
    let now = unix_now();
    let (subject, username) = match jwt_sub(&tokens.access_token) {
        Some(subject) => (subject, None),
        None => {
            let response = http
                .get(format!("{hub}/api/v1/me"))
                .bearer_auth(&tokens.access_token)
                .send()
                .await
                .map_err(|error| Refusal::Unavailable(format!("hub /api/v1/me: {error}")))?;
            if !response.status().is_success() {
                return Err(Refusal::Unavailable(format!(
                    "hub /api/v1/me answered {}",
                    response.status()
                )));
            }
            let me: serde_json::Value = read_json(response).await?;
            let subject = me["subject"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Refusal::Unavailable("the hub named no account".into()))?;
            (
                subject.to_string(),
                me["name"]
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .map(str::to_string),
            )
        }
    };
    Ok(Account {
        issuer: hub.to_string(),
        client_id: HUB_CLIENT.into(),
        subject,
        username,
        email: None,
        hub: Some(hub.to_string()),
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: now + tokens.expires_in.unwrap_or(3600),
        signed_in_at: now,
    })
}

/// A hub's token endpoint from its metadata; `/token` (`auth.mode devagotchi`) when the
/// metadata cannot be read.
pub(super) async fn hub_token_endpoint(http: &reqwest::Client, hub: &str) -> String {
    match hub_metadata(http, hub).await {
        Ok(metadata) => hub_endpoint(&metadata, hub, "token_endpoint"),
        Err(_) => None,
    }
    .unwrap_or_else(|| format!("{hub}/token"))
}

pub(super) async fn hub_refresh(account: &Account) -> Result<Account, Refusal> {
    let Some(refresh) = account.refresh_token.as_deref() else {
        return Err(Refusal::Refused("no refresh token".into()));
    };
    let hub = account.issuer.trim_end_matches('/');
    let http = hub_http()?;
    let token = hub_token_endpoint(&http, hub).await;
    let body: serde_json::Value = http
        .post(token)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", HUB_CLIENT),
        ])
        .send()
        .await
        .map_err(|error| Refusal::Unavailable(error.to_string()))?
        .error_for_status()
        .map_err(|error| Refusal::Unavailable(error.to_string()))?
        .json()
        .await
        .map_err(|_| Refusal::Unavailable("hub refresh was not JSON".into()))?;
    let access = body["access_token"]
        .as_str()
        .ok_or_else(|| Refusal::Unavailable("hub sent no access token".into()))?
        .to_string();
    Ok(Account {
        issuer: account.issuer.clone(),
        client_id: account.client_id.clone(),
        subject: jwt_sub(&access).unwrap_or_else(|| account.subject.clone()),
        username: account.username.clone(),
        email: account.email.clone(),
        hub: account.hub.clone(),
        access_token: access,
        refresh_token: body["refresh_token"]
            .as_str()
            .map(str::to_string)
            .or_else(|| account.refresh_token.clone()),
        expires_at: unix_now() + body["expires_in"].as_u64().unwrap_or(3600),
        signed_in_at: account.signed_in_at,
    })
}

/// End a hub account's tokens (RFC 7009) at the revocation endpoint its hub names: the
/// refresh token first, which ends the whole family on a hub with its own accounts.
pub(super) async fn hub_revoke(account: &Account) -> Result<(), Refusal> {
    let hub = account.issuer.trim_end_matches('/');
    let http = hub_http()?;
    let metadata = hub_metadata(&http, hub).await?;
    let Some(revoke) = hub_endpoint(&metadata, hub, "revocation_endpoint") else {
        return Err(Refusal::Refused(
            "the hub names no revocation endpoint".into(),
        ));
    };
    for token in [
        account.refresh_token.as_deref(),
        Some(account.access_token.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        let response = http
            .post(&revoke)
            .form(&[("token", token), ("client_id", HUB_CLIENT)])
            .send()
            .await
            .map_err(|error| Refusal::Unavailable(format!("hub revoke: {error}")))?;
        if !response.status().is_success() {
            return Err(Refusal::Unavailable(format!(
                "hub revoke answered {}",
                response.status()
            )));
        }
    }
    Ok(())
}

pub(super) fn jwt_sub(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()?
        .get("sub")?
        .as_str()
        .map(str::to_string)
}
