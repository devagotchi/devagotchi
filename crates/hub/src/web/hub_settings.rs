//! `/admin`: the hub's own settings, for its owner, grouped by what they are for and guided
//! step by step. Sign-in providers are set here (Google, GitHub, Sign in with Shippin through
//! Nomen, and any other OpenID Connect provider) and stored in Postgres with their secrets
//! sealed, and take effect without a restart. The sections below them say what is set and
//! where to change it. Anyone but the owner gets an unknown path's answer.

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
use crate::{
    AppState,
    accounts::sign_in_settings::{NewSignIn, StoredSignIn},
    auth::Subject,
    config::providers::ProviderConfig,
};

/// What a person follows to get a provider's id and secret, and its name on the page.
struct Guide {
    name: &'static str,
    /// `google`, `github` or `oidc`.
    kind: &'static str,
    title: &'static str,
    /// An OpenID Connect provider's issuer, filled in for the person to keep or change.
    issuer: Option<&'static str>,
    /// What its button says on the sign-in page.
    label: &'static str,
    open_label: &'static str,
    open_url: &'static str,
    steps: [&'static str; 3],
    callback_label: &'static str,
}

const GUIDES: [Guide; 3] = [
    Guide {
        name: "google",
        kind: "google",
        title: "Google",
        issuer: None,
        label: "Google",
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
        kind: "github",
        title: "GitHub",
        issuer: None,
        label: "GitHub",
        open_label: "Open GitHub OAuth apps",
        open_url: "https://github.com/settings/developers",
        steps: [
            "Choose New OAuth app.",
            "Give it a name and use this hub's address as the homepage URL.",
            "Paste the address below as the Authorization callback URL and register it.",
        ],
        callback_label: "Authorization callback URL",
    },
    Guide {
        name: "nomen",
        kind: "oidc",
        title: "Shippin (Nomen)",
        issuer: Some("https://id.shippin.cloud"),
        label: "Shippin",
        open_label: "Open Shippin",
        open_url: "https://id.shippin.cloud",
        steps: [
            "Create an OpenID Connect client for a web application.",
            "Paste the address below as its redirect URI.",
            "Copy its client ID and secret into the fields below.",
        ],
        callback_label: "Redirect URI",
    },
];

/// The card for an OpenID Connect provider the owner added by name.
const OTHER: Guide = Guide {
    name: "",
    kind: "oidc",
    title: "",
    issuer: None,
    label: "",
    open_label: "",
    open_url: "",
    steps: [
        "In your identity provider (Okta, Microsoft Entra ID, Auth0, Keycloak, Authentik and others), create an OpenID Connect client for a web application.",
        "Paste this hub's redirect address, shown after you save, as its redirect URI.",
        "Copy its issuer, client ID and secret into the fields below.",
    ],
    callback_label: "Redirect URI",
};

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
    /// An OpenID Connect provider's issuer and button label.
    #[serde(default)]
    issuer: String,
    #[serde(default)]
    label: String,
    /// A new OpenID Connect provider's name (`/admin/sign-in/new`).
    #[serde(default)]
    name: String,
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

fn callback_of(state: &AppState, name: &str) -> String {
    format!(
        "{}/auth/provider/{name}/callback",
        state.hub.config.public_url.trim_end_matches('/')
    )
}

fn sign_in_card(
    state: &AppState,
    guide: &Guide,
    name: &str,
    stored: Option<&StoredSignIn>,
    active: bool,
) -> String {
    let title = if guide.title.is_empty() {
        stored
            .map(|s| {
                if s.label.is_empty() {
                    s.name.as_str()
                } else {
                    s.label.as_str()
                }
            })
            .unwrap_or(name)
    } else {
        guide.title
    };
    let status = match (active, stored.is_some()) {
        (true, true) => "<span class=\"tag kind\">On</span>",
        (true, false) => "<span class=\"tag kind\">On, from this hub's config file</span>",
        (false, true) => "<span class=\"tag\">Off</span>",
        (false, false) => "<span class=\"tag\">Not set up</span>",
    };
    let open = if guide.open_url.is_empty() {
        String::new()
    } else {
        format!(
            "<li><a href=\"{}\" rel=\"noreferrer\" target=\"_blank\">{}</a></li>",
            guide.open_url, guide.open_label
        )
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
    let oidc = if guide.kind == "oidc" {
        let issuer = stored
            .map(|s| s.issuer.as_str())
            .filter(|i| !i.is_empty())
            .or(guide.issuer)
            .unwrap_or_default();
        let label = stored
            .map(|s| s.label.as_str())
            .filter(|l| !l.is_empty())
            .unwrap_or(guide.label);
        format!(
            "<label>Issuer<input type=\"url\" name=\"issuer\" autocomplete=\"off\" spellcheck=\"false\" value=\"{}\" placeholder=\"https://\" required></label>\
<label>Button label<input type=\"text\" name=\"label\" maxlength=\"40\" value=\"{}\" placeholder=\"Continue with …\"></label>",
            escape(issuer),
            escape(label)
        )
    } else {
        String::new()
    };
    format!(
        "<details class=\"connector\" id=\"sign-in-{name}\"{open_attr}><summary>{title} {status}</summary>\
<ol class=\"connect-steps\">{open}{steps}</ol>\
<p class=\"meta\">{callback_label}</p><div class=\"command\"><pre><code>{callback}</code></pre></div>\
<form class=\"prefs\" method=\"post\" action=\"/admin/sign-in/{name}\">{oidc}\
<label>Client ID<input type=\"text\" name=\"client_id\" autocomplete=\"off\" spellcheck=\"false\" value=\"{client_id}\" required></label>\
<label>Client secret<input type=\"password\" name=\"client_secret\" autocomplete=\"new-password\" placeholder=\"{secret_hint}\"></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\"{checked}><span>Offer {title} on the sign-in page</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Save</button></p></form></details>",
        name = escape(name),
        open_attr = if active { "" } else { " open" },
        title = escape(title),
        callback_label = guide.callback_label,
        callback = escape(&callback_of(state, name)),
        client_id = escape(client_id),
        checked = if enabled { " checked" } else { "" },
    )
}

/// The form that adds another OpenID Connect provider by name.
fn new_provider_card() -> String {
    let steps: String = OTHER
        .steps
        .iter()
        .map(|step| format!("<li>{step}</li>"))
        .collect();
    format!(
        "<details class=\"connector\" id=\"sign-in-new\"><summary>Add another OpenID Connect provider</summary>\
<ol class=\"connect-steps\">{steps}</ol>\
<form class=\"prefs\" method=\"post\" action=\"/admin/sign-in/new\">\
<label>Name<input type=\"text\" name=\"name\" pattern=\"[a-z0-9][a-z0-9-]{{0,31}}\" maxlength=\"32\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"okta\" required></label>\
<p class=\"meta\">Lowercase letters, digits and dashes. It is part of the redirect address.</p>\
<label>Button label<input type=\"text\" name=\"label\" maxlength=\"40\" placeholder=\"Okta\" required></label>\
<label>Issuer<input type=\"url\" name=\"issuer\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"https://\" required></label>\
<label>Client ID<input type=\"text\" name=\"client_id\" autocomplete=\"off\" spellcheck=\"false\" required></label>\
<label>Client secret<input type=\"password\" name=\"client_secret\" autocomplete=\"new-password\" placeholder=\"Leave empty for a public client\"></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\" checked><span>Offer it on the sign-in page</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Add</button></p></form></details>"
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
    let mut cards: String = GUIDES
        .iter()
        .map(|guide| {
            sign_in_card(
                &state,
                guide,
                guide.name,
                stored.iter().find(|s| s.name == guide.name),
                hub.providers.get(guide.name).is_some(),
            )
        })
        .collect();
    for other in stored
        .iter()
        .filter(|s| !GUIDES.iter().any(|guide| guide.name == s.name))
    {
        cards.push_str(&sign_in_card(
            &state,
            &OTHER,
            &other.name,
            Some(other),
            hub.providers.get(&other.name).is_some(),
        ));
    }
    cards.push_str(&new_provider_card());
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
<section class=\"card\" id=\"sign-in\"><h2>Sign-in</h2><p class=\"note help\">How people sign in to this hub: \
Google, GitHub, Shippin, or your own identity provider. Secrets are encrypted in this hub's Postgres.</p>{cards}\
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
    Path(path): Path<String>,
    Form(form): Form<SignInForm>,
) -> Response {
    let Some(person) = owner(&state, &headers).await else {
        return hidden();
    };
    let Some(hub) = &state.passkeys else {
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
    let accounts = hub.accounts();
    let current = match accounts.sign_in_settings().await {
        Ok(current) => current,
        Err(error) => {
            eprintln!("devagotchi hub: sign-in settings: {error}");
            return unavailable();
        }
    };
    // Which provider: a guided one, a new one by the name given, or one added before.
    let (name, kind) = if let Some(guide) = GUIDES.iter().find(|g| g.name == path) {
        (guide.name.to_string(), guide.kind)
    } else if path == "new" {
        let name = form.name.trim().to_string();
        if GUIDES.iter().any(|g| g.name == name) || current.iter().any(|s| s.name == name) {
            return refused(
                StatusCode::CONFLICT,
                "A provider of that name is set up already; change it in its own card.",
            );
        }
        (name, "oidc")
    } else if current.iter().any(|s| s.name == path && s.kind == "oidc") {
        (path.clone(), "oidc")
    } else {
        return hidden();
    };
    let client_id = form.client_id.trim();
    let secret = form.client_secret.trim();
    let issuer = form.issuer.trim().trim_end_matches('/');
    let label = form.label.trim();
    let bad = |text: &str, max: usize| text.len() > max || text.chars().any(char::is_control);
    if client_id.is_empty()
        || bad(client_id, 512)
        || bad(secret, 2048)
        || bad(issuer, 512)
        || bad(label, 40)
    {
        return refused(
            StatusCode::BAD_REQUEST,
            "Paste the client ID from the provider.",
        );
    }
    if kind == "oidc" {
        // The config file's own rules: a name of [a-z0-9-], an https issuer.
        let described = serde_json::json!({
            "kind": "oidc", "name": name, "issuer": issuer, "client_id": client_id,
        });
        let checked = serde_json::from_value::<ProviderConfig>(described)
            .map_err(|error| error.to_string())
            .and_then(|config| config.check(&|_: &str| None));
        if let Err(why) = checked {
            let why = why
                .trim_start_matches(&format!("auth.providers {name}: "))
                .to_string();
            return refused(StatusCode::BAD_REQUEST, &format!("Not saved: {why}."));
        }
    }
    let sealed = if secret.is_empty() {
        match current.iter().find(|s| s.name == name) {
            Some(existing) => existing.sealed_secret.clone(),
            // An OpenID Connect provider may have a public client (PKCE only).
            None if kind == "oidc" => String::new(),
            None => return refused(StatusCode::BAD_REQUEST, "Paste the client secret too."),
        }
    } else {
        hub.providers.sealer().seal(&name, secret)
    };
    if let Err(error) = accounts
        .save_sign_in(
            &person.id,
            &NewSignIn {
                name: &name,
                kind,
                label,
                issuer: if kind == "oidc" { issuer } else { "" },
                client_id,
                sealed_secret: &sealed,
                enabled: form.enabled.is_some(),
            },
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
    see_other(&format!("/admin?saved={name}#sign-in-{name}"), &[])
}
