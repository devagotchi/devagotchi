//! Every page's `<head>` and its Content-Security-Policy, by what kind of page it is: what
//! may load is what the page needs, from this hub only. The pages a phone installs the app
//! from (the app, the sign-in page, the landing page) carry its manifest, icon and colour.

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use super::{BAR, Bar, Tab, bar_html, escape, site, tabs};
use crate::site::Site;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Plain,
    /// The landing page: laid out wide as full-viewport pages under the fixed bar
    /// (`body.landing`, `landing.css`), with one script of the hub's own that moves its setup
    /// walkthrough along (`/assets/landing.js`).
    Landing,
    /// A page in the phone app's frame (`app_page`), on `tab`; `push` when it can turn on
    /// this phone's notifications.
    App {
        tab: Tab,
        push: bool,
    },
    Script(&'static str),
    Workflow {
        passkey: bool,
        push: bool,
    },
    /// A sign-in page: where a phone that is not signed in lands, so it installs the app
    /// from here too. `script` is the hub's passkey script on the one-owner hub's page.
    SignIn {
        script: Option<&'static str>,
    },
}

/// What a phone reads to install the app: the manifest, the icon and the bar's colour.
fn installable(site: &Site) -> String {
    let icon = site
        .named(site.info.assets.icon.as_ref())
        .map(|icon| format!("<link rel=\"icon\" href=\"{}\">", escape(&icon.url())))
        .unwrap_or_default();
    format!(
        "<link rel=\"manifest\" href=\"/manifest.webmanifest\">{icon}\
<meta name=\"theme-color\" content=\"{}\">",
        escape(site.theme_color())
    )
}

/// A page: `meta` is what search engines and link previews read in its head, on the
/// public pages only.
pub(super) fn render(
    status: StatusCode,
    title: &str,
    body: &str,
    kind: Kind,
    meta: &str,
) -> Response {
    let site = site();
    let head = match kind {
        Kind::Plain => String::new(),
        Kind::Landing => format!(
            "{}{}",
            installable(&site),
            super::super::landing::head(&site)
        ),
        Kind::App { push, .. } => format!(
            "{}<script src=\"/assets/reveal.js\"></script>\
<script src=\"/assets/app.js\" defer></script>{}",
            installable(&site),
            if push {
                "<script src=\"/assets/push.js\" defer></script>"
            } else {
                ""
            }
        ),
        Kind::Script(script) => format!("<script src=\"{script}\" defer></script>"),
        Kind::Workflow { passkey, push } => {
            let mut head = String::from("<script src=\"/assets/workflow.js\" defer></script>");
            if passkey {
                head.push_str("<script src=\"/assets/passkey.js\" defer></script>");
            }
            if push {
                head.push_str(&format!(
                    "<link rel=\"manifest\" href=\"/manifest.webmanifest\">\
<meta name=\"theme-color\" content=\"{}\"><script src=\"/assets/push.js\" defer></script>",
                    escape(site.theme_color())
                ));
            }
            head
        }
        Kind::SignIn { script } => {
            let mut head = installable(&site);
            if let Some(script) = script {
                head.push_str(&format!("<script src=\"{script}\" defer></script>"));
            }
            head
        }
    };
    let (tabs, banner) = match kind {
        Kind::App { tab, push } => tabs(tab, push, &site),
        _ => (String::new(), String::new()),
    };
    let framed = matches!(kind, Kind::App { .. });
    // How the person likes the app to look, when they said.
    let theme = BAR
        .try_with(|bar| match bar {
            Bar::SignedIn { me: Some(me), .. }
                if framed && me.theme != crate::identity::preferences::Theme::System =>
            {
                format!(" data-theme=\"{}\"", me.theme.as_str())
            }
            _ => String::new(),
        })
        .unwrap_or_default();
    let page = format!(
        "<!doctype html>\n<html lang=\"en\"{theme}><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1{cover}\">\
<title>{} · {}</title>{meta}<link rel=\"stylesheet\" href=\"/assets/hub.css\">{head}</head>\
<body{class}>{bar}<main>{banner}{body}</main>{tabs}</body></html>\n",
        escape(title),
        escape(site.name()),
        // The frame reaches the phone's edges and keeps clear of them itself (`hub.css`).
        cover = if framed { ", viewport-fit=cover" } else { "" },
        class = match kind {
            Kind::Landing => " class=\"landing\"",
            Kind::App { .. } => " class=\"app\"",
            _ => "",
        },
        bar = BAR
            .try_with(|bar| bar_html(bar, &site, framed))
            .unwrap_or_else(|_| bar_html(&Bar::Plain, &site, framed)),
    );
    let mut response = (status, page).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::CONTENT_SECURITY_POLICY, policy(kind, &site));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

/// The page's Content-Security-Policy: what it needs, from this hub only.
fn policy(kind: Kind, site: &Site) -> HeaderValue {
    let policy = match kind {
        Kind::App { .. } | Kind::Workflow { .. } => {
            "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; worker-src 'self'; \
manifest-src 'self'; img-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
        }
        Kind::Script(_) => {
            "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; \
form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
        }
        Kind::SignIn { script: Some(_) } => {
            "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; \
manifest-src 'self'; img-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
        }
        Kind::SignIn { script: None } => {
            "default-src 'none'; style-src 'self'; manifest-src 'self'; img-src 'self'; form-action 'self'; \
frame-ancestors 'none'; base-uri 'none'"
        }
        Kind::Plain => {
            "default-src 'none'; style-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
        }
        // The walkthrough's script, the site's pictures and the app's manifest, from this
        // hub only.
        Kind::Landing => {
            "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self'; manifest-src 'self'; \
form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
        }
    };
    // A site that serves its own fonts: from this hub only.
    let policy = if site.has_fonts() {
        format!("{policy}; font-src 'self'")
    } else {
        policy.to_string()
    };
    HeaderValue::from_str(&policy)
        .unwrap_or_else(|_| HeaderValue::from_static("default-src 'none'"))
}
