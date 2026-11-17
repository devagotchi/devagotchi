//! The OAuth answers both sign-ins read (RFC 8628 device authorization, the token, an error, userinfo) and reading one with a size cap.

use serde::Deserialize;

use super::Refusal;

#[derive(Deserialize)]
pub(super) struct DeviceAuthorization {
    pub(super) device_code: String,
    pub(super) user_code: String,
    pub(super) verification_uri: String,
    #[serde(default)]
    pub(super) verification_uri_complete: Option<String>,
    pub(super) expires_in: u64,
    #[serde(default)]
    pub(super) interval: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct TokenAnswer {
    pub(super) access_token: String,
    #[serde(default)]
    pub(super) refresh_token: Option<String>,
    #[serde(default)]
    pub(super) expires_in: Option<u64>,
}

#[derive(Deserialize, Default)]
pub(super) struct OAuthError {
    #[serde(default)]
    pub(super) error: String,
    #[serde(default)]
    pub(super) error_description: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct UserInfo {
    pub(super) sub: String,
    #[serde(default)]
    pub(super) preferred_username: Option<String>,
    #[serde(default)]
    pub(super) email: Option<String>,
}

pub(super) const MAX_ANSWER_BYTES: usize = 64 * 1024;

pub(super) async fn read_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, Refusal> {
    let body = response
        .bytes()
        .await
        .map_err(|error| Refusal::Unavailable(format!("the issuer: {error}")))?;
    if body.len() > MAX_ANSWER_BYTES {
        return Err(Refusal::Unavailable(
            "the issuer's answer is too large".into(),
        ));
    }
    serde_json::from_slice(&body).map_err(|_| {
        Refusal::Unavailable("the issuer's answer is not the JSON an issuer sends".into())
    })
}
