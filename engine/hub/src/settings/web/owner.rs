//! Who may open `/admin`, and the forms that page posts.

use axum::http::HeaderMap;
use serde::Deserialize;

use crate::{AppState, identity::Subject};

#[derive(Deserialize)]
pub(crate) struct Shown {
    pub(crate) saved: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SignInForm {
    #[serde(default)]
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) client_secret: String,
    #[serde(default)]
    pub(crate) enabled: Option<String>,
    /// An OpenID Connect provider's issuer and button label.
    #[serde(default)]
    pub(crate) issuer: String,
    #[serde(default)]
    pub(crate) label: String,
    /// A new OpenID Connect provider's name (`/admin/sign-in/new`).
    #[serde(default)]
    pub(crate) name: String,
}

#[derive(Deserialize)]
pub(crate) struct AnalyticsForm {
    #[serde(default)]
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) database: String,
    #[serde(default)]
    pub(crate) user: String,
    #[serde(default)]
    pub(crate) password: String,
    #[serde(default)]
    pub(crate) enabled: Option<String>,
}

/// The hub's owner behind this request.
pub(crate) async fn owner(state: &AppState, headers: &HeaderMap) -> Option<Subject> {
    let hub = state.passkeys.as_ref()?;
    let subject = hub.who(headers).await.ok().flatten()?;
    state.hub.config.is_owner(&subject.id).then_some(subject)
}

/// Whether this person may see the settings: what the You page asks.
pub(crate) fn is_owner(state: &AppState, subject: &Subject) -> bool {
    state.passkeys.is_some() && state.hub.config.is_owner(&subject.id)
}

/// Owner-only pages preserve a failed session lookup as 503, rather than hiding it.
pub(crate) async fn required(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Subject, axum::response::Response> {
    use crate::web::routes::{Who, unavailable, who};
    match who(state, headers).await {
        Who::Person(subject) => {
            if is_owner(state, &subject) {
                Ok(subject)
            } else {
                Err(crate::tenancy::web::operator::hidden())
            }
        }
        Who::SignedOut | Who::NotSetUp => Err(crate::tenancy::web::operator::hidden()),
        Who::Unavailable => Err(unavailable()),
    }
}
