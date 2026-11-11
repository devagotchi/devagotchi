//! `/admin`: the hub's own settings, for its owner, grouped by what they are for and guided
//! step by step. Sign-in providers are set here and stored in Postgres with their secrets
//! sealed, and take effect without a restart. The sections below them say what is set and where
//! to change it. Anyone but the owner gets an unknown path's answer.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use serde::Deserialize;

use super::{
    approvals::same_origin,
    operator::hidden,
    pages::{self, Tab, escape},
    routes::{see_other, unavailable},
};
use crate::{AppState, accounts::sign_in_settings::StoredSignIn, auth::Subject};

/// What a person follows to get a provider's id and secret, and its name on the page.
struct Guide {
    name: &'static str,
    title: &'static str,
    open_label: &'static str,
    open_url: &'static str,
    steps: [&'static str; 3],
    callback_label: &'static str,
}

const GUIDES: [Guide; 2] = [
    Guide {
        name: "google",
        title: "Google",
        open_label: "Open Google Cloud credentials",
        open_url: "https://console.cloud.google.com/apis/credentials",
        steps: [
            "Choose Create credentials, then OAuth client ID.",
            "For Application type choose Web application.",
            "Paste the address below under Authorized redirect URIs and create it.",
        ],
        callback_label: "Authorized redirect URI",
    },
    Guide {
        name: "github",
        title: "GitHub",
        open_label: "Open GitHub OAuth apps",
        open_url: "https://github.com/settings/developers",
        steps: [
            "Choose New OAuth app.",
            "Give it a name and use this hub's address as the homepage URL.",
            "Paste the address below as the Authorization callback URL and register it.",
        ],
        callback_label: "Authorization callback URL",
    },
];

#[derive(Deserialize)]
pub struct Shown {
    saved: Option<String>,
}

#[derive(Deserialize)]
pub struct SignInForm {
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    client_secret: String,
    #[serde(default)]
    enabled: Option<String>,
}

/// The hub's owner behind this request.
async fn owner(state: &AppState, headers: &HeaderMap) -> Option<Subject> {
    let hub = state.passkeys.as_ref()?;
    let subject = hub.who(headers).await.ok().flatten()?;
    state.hub.config.is_owner(&subject.id).then_some(subject)
}

/// Whether this person may see the settings: what the You page asks.
pub(super) fn is_owner(state: &AppState, subject: &Subject) -> bool {
    state.passkeys.is_some() && state.hub.config.is_owner(&subject.id)
}

fn sign_in_card(
    state: &AppState,
    guide: &Guide,
    stored: Option<&StoredSignIn>,
    active: bool,
) -> String {
    let callback = format!(
        "{}/auth/provider/{}/callback",
        state.hub.config.public_url.trim_end_matches('/'),
        guide.name
    );
    let status = match (active, stored.is_some()) {
        (true, true) => "<span class=\"tag kind\">On</span>",
        (true, false) => "<span class=\"tag kind\">On, from this hub's config file</span>",
        (false, true) => "<span class=\"tag\">Off</span>",
        (false, false) => "<span class=\"tag\">Not set up</span>",
    };
    let steps: String = guide
        .steps
        .iter()
        .map(|step| format!("<li>{step}</li>"))
        .collect();
    let client_id = stored.map(|s| s.client_id.as_str()).unwrap_or_default();
    let secret_hint = if stored.is_some() {
        "Saved. Leave empty to keep it."
    } else {
        "Paste it here"
    };
    let enabled = stored.map(|s| s.enabled).unwrap_or(true);
    format!(
        "<details class=\"connector\" id=\"sign-in-{name}\"{open}><summary>{title} {status}</summary>\
<ol class=\"connect-steps\"><li><a href=\"{url}\" rel=\"noreferrer\" target=\"_blank\">{open_label}</a></li>{steps}</ol>\
<p class=\"meta\">{callback_label}</p><div class=\"command\"><pre><code>{callback}</code></pre></div>\
<form class=\"prefs\" method=\"post\" action=\"/admin/sign-in/{name}\">\
<label>Client ID<input type=\"text\" name=\"client_id\" autocomplete=\"off\" spellcheck=\"false\" value=\"{client_id}\" required></label>\
<label>Client secret<input type=\"password\" name=\"client_secret\" autocomplete=\"new-password\" placeholder=\"{secret_hint}\"></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\"{checked}><span>Offer {title} on the sign-in page</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Save</button></p></form></details>",
        name = guide.name,
        open = if active { "" } else { " open" },
        title = guide.title,
        url = guide.open_url,
        open_label = guide.open_label,
        callback_label = guide.callback_label,
        callback = escape(&callback),
        client_id = escape(client_id),
        checked = if enabled { " checked" } else { "" },
    )
}

fn row(label: &str, value: &str, how: &str) -> String {
    format!(
        "<li><strong>{}</strong> <span class=\"meta\">{}</span><p class=\"meta\">{}</p></li>",
        escape(label),
        escape(value),
        how
    )
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    if owner(&state, &headers).await.is_none() {
        return hidden();
    }
    let Some(hub) = &state.passkeys else {
        return hidden();
    };
    let stored = match hub.accounts().sign_in_settings().await {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("devagotchi hub: sign-in settings: {error}");
            return unavailable();
        }
    };
    let cards: String = GUIDES
        .iter()
        .map(|guide| {
            sign_in_card(
                &state,
                guide,
                stored.iter().find(|s| s.name == guide.name),
                hub.providers.get(guide.name).is_some(),
            )
        })
        .collect();
    let flash = shown
        .saved
        .as_deref()
        .map(|name| {
            format!(
                "<p class=\"note\" id=\"flash\" role=\"status\">Saved {}. It applies now.</p>",
                escape(name)
            )
        })
        .unwrap_or_default();
    let analytics = if state.hub.boards.warehouse().is_some() {
        "Connected"
    } else {
        "Not connected"
    };
    let sinks = state.hub.boards.signal_sink_count();
    let email = if hub.providers.email.is_some() {
        "On"
    } else {
        "Off"
    };
    let install = "Set when the hub is installed. Re-run <code>install.sh</code> to change it.";
    let body = format!(
        "<h1>Hub settings</h1><p class=\"meta\">For this hub's owner. Changes apply as soon as you save.</p>{flash}\
<nav class=\"chips\" aria-label=\"Sections\"><a class=\"chip\" href=\"#sign-in\">Sign-in</a>\
<a class=\"chip\" href=\"#access\">AI access</a><a class=\"chip\" href=\"#people\">People</a>\
<a class=\"chip\" href=\"#data\">Data and signals</a></nav>\
<section class=\"card\" id=\"sign-in\"><h2>Sign-in</h2><p class=\"note help\">How people sign in to this hub. \
Secrets are encrypted in this hub's Postgres.</p>{cards}\
<ul class=\"rows\">{mail}</ul></section>\
<section class=\"card\" id=\"access\"><h2>AI access</h2><ul class=\"rows\">{connect}{keys}</ul></section>\
<section class=\"card\" id=\"people\"><h2>People</h2><ul class=\"rows\">{orgs}</ul></section>\
<section class=\"card\" id=\"data\"><h2>Data and signals</h2><ul class=\"rows\">{stats}{signals}</ul></section>",
        mail = row("Email sign-in links", email, install),
        connect = row(
            "Connect an AI",
            "17 clients",
            "<a href=\"/connect\">Exact steps for each client</a>"
        ),
        keys = row(
            "API keys",
            "For agents that cannot sign in",
            "<a href=\"/app/keys\">Manage keys</a>"
        ),
        orgs = row(
            "Organisations",
            "Members, teams, roles and invites",
            "<a href=\"/orgs\">Manage organisations</a>"
        ),
        stats = row("Statistics (ClickHouse)", analytics, install),
        signals = row("Signal sinks", &sinks.to_string(), install),
    );
    let mut response = pages::app(StatusCode::OK, "Hub settings", &body, Tab::Me);
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

pub async fn save_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Form(form): Form<SignInForm>,
) -> Response {
    if !GUIDES.iter().any(|guide| guide.name == name) {
        return hidden();
    }
    let Some(person) = owner(&state, &headers).await else {
        return hidden();
    };
    let refused = |status: StatusCode, why: &str| {
        pages::message(status, "Not saved", why, Some(("/admin", "Hub settings")))
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return refused(
            StatusCode::FORBIDDEN,
            "Change this from the hub's own settings page.",
        );
    }
    let Some(hub) = &state.passkeys else {
        return hidden();
    };
    let client_id = form.client_id.trim();
    let secret = form.client_secret.trim();
    let bad = |text: &str, max: usize| text.len() > max || text.chars().any(char::is_control);
    if client_id.is_empty() || bad(client_id, 512) || bad(secret, 2048) {
        return refused(
            StatusCode::BAD_REQUEST,
            "Paste the client ID from the provider.",
        );
    }
    let accounts = hub.accounts();
    let current = match accounts.sign_in_settings().await {
        Ok(current) => current,
        Err(error) => {
            eprintln!("devagotchi hub: sign-in settings: {error}");
            return unavailable();
        }
    };
    let sealed = if secret.is_empty() {
        match current.iter().find(|s| s.name == name) {
            Some(existing) => existing.sealed_secret.clone(),
            None => return refused(StatusCode::BAD_REQUEST, "Paste the client secret too."),
        }
    } else {
        hub.providers.sealer().seal(&name, secret)
    };
    if let Err(error) = accounts
        .save_sign_in(
            &person.id,
            &name,
            client_id,
            &sealed,
            form.enabled.is_some(),
        )
        .await
    {
        eprintln!("devagotchi hub: saving a sign-in: {error}");
        return unavailable();
    }
    if let Err(error) = hub.reload_providers().await {
        eprintln!("devagotchi hub: applying sign-ins: {error}");
        return unavailable();
    }
    see_other(&format!("/admin?saved={name}"), &[])
}
