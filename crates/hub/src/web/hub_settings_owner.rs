//! Who may open `/admin`, and the forms that page posts.

use axum::http::HeaderMap;
use serde::Deserialize;

use crate::{AppState, identity::Subject};

#[derive(Deserialize)]
pub(super) struct Shown {
    pub(super) saved: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct SignInForm {
    #[serde(default)]
    pub(super) client_id: String,
    #[serde(default)]
    pub(super) client_secret: String,
    #[serde(default)]
    pub(super) enabled: Option<String>,
    /// An OpenID Connect provider's issuer and button label.
    #[serde(default)]
    pub(super) issuer: String,
    #[serde(default)]
    pub(super) label: String,
    /// A new OpenID Connect provider's name (`/admin/sign-in/new`).
    #[serde(default)]
    pub(super) name: String,
}

#[derive(Deserialize)]
pub(super) struct AnalyticsForm {
    #[serde(default)]
    pub(super) url: String,
    #[serde(default)]
    pub(super) database: String,
    #[serde(default)]
    pub(super) user: String,
    #[serde(default)]
    pub(super) password: String,
    #[serde(default)]
    pub(super) enabled: Option<String>,
}

/// The hub's owner behind this request.
pub(super) async fn owner(state: &AppState, headers: &HeaderMap) -> Option<Subject> {
    let hub = state.passkeys.as_ref()?;
    let subject = hub.who(headers).await.ok().flatten()?;
    state.hub.config.is_owner(&subject.id).then_some(subject)
}

/// Whether this person may see the settings: what the You page asks.
pub(super) fn is_owner(state: &AppState, subject: &Subject) -> bool {
    state.passkeys.is_some() && state.hub.config.is_owner(&subject.id)
}
