//! The Sign-in cards on `/admin`. Google, GitHub, Shippin (Nomen) and any other OpenID
//! Connect provider. Shippin's issuer is whatever the owner types, or whatever `install.sh
//! --nomen` wrote; this card never fills a host in. Its switch offers the button and, off,
//! takes the button away without deleting the issuer.

use super::{pages::escape, providers::Provider};
use crate::{AppState, identity::accounts::sign_in_settings::StoredSignIn};

/// Beside Continue with Shippin. The operator wrote this (free-edition plan, D1a).
pub(super) const SHIPPIN_NOTICE: &str = "This gives you an account on Shippin.ai, to use on your site and to activate across the Shippin network suite of services.";

/// What a person follows to get a provider's id and secret, and its name on the page.
struct Guide {
    name: &'static str,
    /// `google`, `github` or `oidc`.
    kind: &'static str,
    title: &'static str,
    /// An OpenID Connect provider's issuer, filled in for the person to keep or change.
    /// Shippin's stays empty: an issuer in this table would be a hardcoded host.
    issuer: Option<&'static str>,
    /// What its button says on the sign-in page.
    label: &'static str,
    open_label: &'static str,
    open_url: &'static str,
    /// Shown under the steps. Shippin's says what the button creates.
    notice: Option<&'static str>,
    steps: [&'static str; 3],
    callback_label: &'static str,
}

const GUIDES: [Guide; 4] = [
    Guide {
        name: "google",
        kind: "google",
        title: "Google",
        issuer: None,
        label: "Google",
        open_label: "Open Google Cloud credentials",
        open_url: "https://console.cloud.google.com/apis/credentials",
        notice: None,
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
        notice: None,
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
        title: "Shippin",
        issuer: None,
        label: "Shippin",
        open_label: "",
        open_url: "",
        notice: Some(SHIPPIN_NOTICE),
        steps: [
            "Type your Nomen's https address as the issuer. This hub does not fill one in.",
            "Paste the client ID from that Nomen (or from devagotchi hub connect-nomen). Leave the secret empty for a public client.",
            "The switch offers Continue with Shippin. Turn it off and the button goes; the issuer you typed stays.",
        ],
        callback_label: "Redirect URI",
    },
    Guide {
        name: "oidc",
        kind: "oidc",
        title: "OpenID Connect",
        issuer: None,
        label: "",
        open_label: "",
        open_url: "",
        notice: None,
        steps: [
            "In your identity provider, create an OpenID Connect client for a web application.",
            "Paste the address below as its redirect URI.",
            "Copy its issuer, client ID and secret into the fields below.",
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
    notice: None,
    steps: [
        "In your identity provider (Okta, Microsoft Entra ID, Auth0, Keycloak, Authentik and others), create an OpenID Connect client for a web application.",
        "Paste this hub's redirect address, shown after you save, as its redirect URI.",
        "Copy its issuer, client ID and secret into the fields below.",
    ],
    callback_label: "Redirect URI",
};

/// `(name, kind)` when `path` is one of the guided cards.
pub(super) fn guide(path: &str) -> Option<(&'static str, &'static str)> {
    GUIDES
        .iter()
        .find(|guide| guide.name == path)
        .map(|guide| (guide.name, guide.kind))
}

pub(super) fn is_guide(name: &str) -> bool {
    GUIDES.iter().any(|guide| guide.name == name)
}

/// Every Sign-in card, Shippin included, on a hub that has saved nothing yet.
pub(super) fn render(
    state: &AppState,
    stored: &[StoredSignIn],
    providers: &super::providers::Providers,
) -> String {
    let mut cards: String = GUIDES
        .iter()
        .map(|guide| {
            let configured = providers.get(guide.name);
            sign_in_card(
                state,
                guide,
                guide.name,
                stored.iter().find(|row| row.name == guide.name),
                configured.as_deref(),
                configured.is_some(),
            )
        })
        .collect();
    for other in stored.iter().filter(|row| !is_guide(&row.name)) {
        let configured = providers.get(&other.name);
        cards.push_str(&sign_in_card(
            state,
            &OTHER,
            &other.name,
            Some(other),
            configured.as_deref(),
            configured.is_some(),
        ));
    }
    cards.push_str(&new_provider_card());
    cards
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
    configured: Option<&Provider>,
    active: bool,
) -> String {
    let title = if guide.title.is_empty() {
        stored
            .map(|row| {
                if row.label.is_empty() {
                    row.name.as_str()
                } else {
                    row.label.as_str()
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
    let notice = guide
        .notice
        .map(|text| format!("<p class=\"note\">{}</p>", escape(text)))
        .unwrap_or_default();
    let configured_label = configured
        .map(|provider| provider.config.label())
        .unwrap_or_default();
    let client_id = stored
        .map(|row| row.client_id.as_str())
        .filter(|text| !text.is_empty())
        .or_else(|| configured.map(Provider::client_id))
        .unwrap_or("");
    let secret_hint = if stored.is_some() || configured.is_some() {
        "Saved. Leave empty to keep it."
    } else if guide.kind == "oidc" {
        "Leave empty for a public client"
    } else {
        "Paste it here"
    };
    // The switch defaults to on. A hub with no row yet offers the provider once the
    // owner saves an issuer; unchecking the switch is what turns Shippin off.
    let enabled = stored.map(|row| row.enabled).unwrap_or(true);
    let oidc = if guide.kind == "oidc" {
        let issuer = stored
            .map(|row| row.issuer.as_str())
            .filter(|text| !text.is_empty())
            .or_else(|| configured.and_then(|provider| provider.config.issuer()))
            .or(guide.issuer)
            .unwrap_or("");
        let label = stored
            .map(|row| row.label.as_str())
            .filter(|text| !text.is_empty())
            .unwrap_or(if configured_label.is_empty() {
                guide.label
            } else {
                configured_label.as_str()
            });
        format!(
            "<label>Issuer<input type=\"url\" name=\"issuer\" autocomplete=\"off\" spellcheck=\"false\" value=\"{}\" placeholder=\"https://id.example.com\" required></label>\
<label>Button label<input type=\"text\" name=\"label\" maxlength=\"40\" value=\"{}\" placeholder=\"Continue with …\"></label>",
            escape(issuer),
            escape(label)
        )
    } else {
        String::new()
    };
    format!(
        "<details class=\"connector\" id=\"sign-in-{name}\"{open_attr}><summary>{title} {status}</summary>\
<ol class=\"connect-steps\">{open}{steps}</ol>{notice}\
<p class=\"meta\">{callback_label}</p><div class=\"command\"><pre><code>{callback}</code></pre></div>\
<form class=\"prefs\" method=\"post\" action=\"/admin/sign-in/{name}\">{oidc}\
<label>Client ID<input type=\"text\" name=\"client_id\" autocomplete=\"off\" spellcheck=\"false\" value=\"{client_id}\" required></label>\
<label>Client secret<input type=\"password\" name=\"client_secret\" autocomplete=\"new-password\" placeholder=\"{secret_hint}\"></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\"{checked}><span>Offer {title} on the sign-in page</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Save</button></p></form></details>",
        name = escape(name),
        open_attr = if active { "" } else { " open" },
        title = escape(title),
        notice = notice,
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
<label>Issuer<input type=\"url\" name=\"issuer\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"https://id.example.com\" required></label>\
<label>Client ID<input type=\"text\" name=\"client_id\" autocomplete=\"off\" spellcheck=\"false\" required></label>\
<label>Client secret<input type=\"password\" name=\"client_secret\" autocomplete=\"new-password\" placeholder=\"Leave empty for a public client\"></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\" checked><span>Offer it on the sign-in page</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Add</button></p></form></details>"
    )
}
