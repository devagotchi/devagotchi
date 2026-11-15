//! `/app/connections`: the outside services this account's agents call through the hub. Add
//! one (pick the service, paste its key: it is checked against the service before it is kept,
//! sealed, and never shown again), see when each was last used, remove it. Agents list them
//! with `connections` and call them with `connection_call` (`crate::connections`). Laid out for
//! the phone, and as two columns in a wide browser window (`hub.css`, `.connections`).

use crate::connections::catalog::{self, CATALOG};
use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use serde::Deserialize;

use super::person;
use crate::{
    AppState,
    auth::Subject,
    connections::Connections,
    web::{
        approvals::same_origin,
        pages::{self, Tab, escape},
        routes::{see_other, unavailable},
    },
};

#[derive(Deserialize, Default)]
pub struct Shown {
    saved: Option<String>,
    removed: Option<String>,
}

#[derive(Deserialize)]
pub struct AddForm {
    #[serde(default)]
    service: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    key: String,
}

fn not_here() -> Response {
    pages::message(
        StatusCode::NOT_FOUND,
        "Connections",
        "This hub does not keep its own accounts, so it holds no connections.",
        Some(("/app/me", "Back")),
    )
}

fn parts<'a>(state: &'a AppState, subject: &Subject) -> Option<(&'a Connections, String)> {
    Some((state.hub.connections.as_deref()?, subject.tenant.clone()?))
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let subject = match person(&state, &headers, "/app/connections").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let Some((connections, tenant)) = parts(&state, &subject) else {
        return not_here();
    };
    let stored = match connections.accounts().connections(&tenant).await {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("devagotchi hub: connections: {error}");
            return unavailable();
        }
    };
    let flash = match (shown.saved, shown.removed) {
        (Some(name), _) => format!(
            "<p class=\"note\" id=\"flash\" role=\"status\">Connected {}. Your agents can call it now.</p>",
            escape(&name)
        ),
        (_, Some(name)) => format!(
            "<p class=\"note\" id=\"flash\" role=\"status\">Removed {}.</p>",
            escape(&name)
        ),
        _ => String::new(),
    };
    let rows: String = stored
        .iter()
        .map(|c| {
            let title = catalog::service(&c.service).map_or(c.service.as_str(), |s| s.title);
            let used = c
                .last_used_at
                .map(pages::ago)
                .unwrap_or_else(|| "not yet".into());
            format!(
                "<li id=\"connection-{name}\"><strong>{name}</strong> <span class=\"tag\">{title}</span>\
<p class=\"meta\">Key {hint} · added by {by} · last called {used}</p>\
<form method=\"post\" action=\"/app/connections/{name}/remove\"><button class=\"deny\" type=\"submit\">Remove</button></form></li>",
                name = escape(&c.name),
                title = escape(title),
                hint = escape(&c.hint),
                by = escape(&c.created_by),
            )
        })
        .collect();
    let list = if rows.is_empty() {
        "<p class=\"empty\">Nothing connected yet. Add a service, and your agents can call it without ever seeing its key.</p>".to_string()
    } else {
        format!("<ul class=\"rows\" id=\"connections\">{rows}</ul>")
    };
    let services: String = CATALOG
        .iter()
        .map(|s| format!("<option value=\"{}\">{}</option>", s.id, escape(s.title)))
        .collect();
    let guides: String = CATALOG
        .iter()
        .map(|s| {
            format!(
                "<li><strong>{}</strong>: make a key at <a href=\"{}\" rel=\"noreferrer\" target=\"_blank\">{}</a> ({})</li>",
                escape(s.title),
                s.keys_at,
                escape(s.keys_at.trim_start_matches("https://")),
                escape(s.key_hint),
            )
        })
        .collect();
    let mcp = format!("{}/mcp", state.hub.config.public_url.trim_end_matches('/'));
    let body = format!(
        "<h1>Connections</h1><p class=\"meta\">Services your agents call through this hub. They never see the keys: \
the hub adds the key, sends the call only to that service, and records it.</p>{flash}\
<div class=\"connections\"><section class=\"card\"><h2>Connected</h2>{list}\
<p class=\"note help\">Agents: <code>connections</code> lists these, <code>connection_call</code> calls one, at <code>{mcp}</code>.</p></section>\
<form class=\"card prefs\" id=\"add-connection\" method=\"post\" action=\"/app/connections\"><h2>Connect a service</h2>\
<label>Service<select name=\"service\" required>{services}</select></label>\
<label>Name<input type=\"text\" name=\"name\" maxlength=\"63\" pattern=\"[a-z0-9][a-z0-9-]{{0,62}}\" autocapitalize=\"none\" autocorrect=\"off\" spellcheck=\"false\" placeholder=\"the service's name\"></label>\
<p class=\"meta\">Leave it empty for the service's own name. Use another (like <code>resend-staging</code>) for a second key.</p>\
<label>Key<input type=\"password\" name=\"key\" required autocomplete=\"off\" maxlength=\"4096\"></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Check and connect</button></p>\
<details><summary>Where each key is made</summary><ul class=\"connect-steps\">{guides}</ul></details></form></div>",
        mcp = escape(&mcp),
    );
    let mut response = pages::app_page(StatusCode::OK, "Connections", &body, Tab::Me);
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

pub async fn add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<AddForm>,
) -> Response {
    let subject = match person(&state, &headers, "/app/connections").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let Some((connections, tenant)) = parts(&state, &subject) else {
        return not_here();
    };
    let refused = |status: StatusCode, why: &str| {
        pages::message(
            status,
            "Not connected",
            why,
            Some(("/app/connections", "Connections")),
        )
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return refused(
            StatusCode::FORBIDDEN,
            "Connect a service from this hub's own page.",
        );
    }
    let Some(service) = catalog::service(form.service.trim()) else {
        return refused(StatusCode::BAD_REQUEST, "Pick a service from the list.");
    };
    let name = match form.name.trim() {
        "" => service.id,
        name => name,
    };
    if !crate::agenthub::valid_name(name) {
        return refused(
            StatusCode::BAD_REQUEST,
            "A name is lowercase letters, digits and dashes, like resend or resend-staging.",
        );
    }
    let key = form.key.trim();
    if key.len() < 8 || key.len() > 4096 || key.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return refused(
            StatusCode::BAD_REQUEST,
            "Paste the whole key, with nothing around it.",
        );
    }
    if let Err(why) = connections.check(service, key).await {
        return refused(
            StatusCode::BAD_REQUEST,
            &format!("Not connected, nothing was kept: {why}"),
        );
    }
    let sealed = connections
        .sealer()
        .seal(&Connections::sealed_as(&tenant, name), key);
    if let Err(error) = connections
        .accounts()
        .save_connection(
            &tenant,
            &subject.id,
            name,
            service.id,
            &sealed,
            &catalog::hint(key),
        )
        .await
    {
        eprintln!("devagotchi hub: saving a connection: {error}");
        return unavailable();
    }
    see_other(
        &format!("/app/connections?saved={name}#connection-{name}"),
        &[],
    )
}

pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    let subject = match person(&state, &headers, "/app/connections").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let Some((connections, tenant)) = parts(&state, &subject) else {
        return not_here();
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Not removed",
            "Remove a connection from this hub's own page.",
            Some(("/app/connections", "Connections")),
        );
    }
    match connections
        .accounts()
        .remove_connection(&tenant, &subject.id, &name)
        .await
    {
        Ok(_) => see_other(
            &format!("/app/connections?removed={}", crate::web::encode(&name)),
            &[],
        ),
        Err(error) => {
            eprintln!("devagotchi hub: removing a connection: {error}");
            unavailable()
        }
    }
}
