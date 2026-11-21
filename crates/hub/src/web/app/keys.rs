//! `/app/keys`: a person's API keys. Make one with what it may do, where, and for how long; see
//! when each was last used; end or replace it at once. The key itself is shown one time, on the
//! answer that made it. Only a signed-in person gets here: a key cannot make keys.

use axum::{
    extract::{Path, Query, RawForm, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};

use super::{board_names, person};
use crate::{
    AppState,
    identity::Subject,
    identity::accounts::api_keys::{ApiKey, DURATIONS, NewKey, Scope},
    web::{
        approvals::same_origin,
        pages::{self, Tab, escape},
        routes::{see_other, unavailable},
    },
};

fn no_keys() -> Response {
    pages::message(
        StatusCode::NOT_FOUND,
        "API keys",
        "This hub does not keep its own accounts, so it cannot issue API keys.",
        Some(("/app/me", "Back")),
    )
}

fn tenant(subject: &Subject) -> Option<&str> {
    subject.tenant.as_deref()
}

fn shown_key(key: &ApiKey, secret: &str, public_url: &str) -> String {
    format!(
        "<section class=\"card key-once\" id=\"new-key\"><h2>Copy this key now</h2>\
<p class=\"note\">It is shown this one time. {name} ends {ends}.</p>\
<div class=\"command\"><pre><code id=\"key-secret\">{secret}</code></pre></div>\
<p class=\"meta\">Connect at <code>{url}/mcp</code> with the header \
<code>Authorization: Bearer</code> and this key. <a href=\"/connect\">Steps for each client</a></p></section>",
        name = escape(&key.name),
        ends = pages::utc(key.expires_at),
        secret = escape(secret),
        url = escape(public_url.trim_end_matches('/')),
    )
}

fn row(key: &ApiKey, now: u64) -> String {
    let id = escape(&key.id);
    let state = match (key.revoked_at, key.live(now)) {
        (Some(_), _) => "<span class=\"tag\">Revoked</span>".to_string(),
        (None, false) => "<span class=\"tag\">Expired</span>".to_string(),
        (None, true) => format!("Ends {}", pages::utc(key.expires_at)),
    };
    let used = key
        .last_used_at
        .map(pages::ago)
        .unwrap_or_else(|| "never".to_string());
    let scopes: String = key
        .scopes
        .iter()
        .map(|scope| format!("<span class=\"tag\">{}</span>", scope.as_str()))
        .collect();
    let limited = |what: &str, names: &[String]| {
        if names.is_empty() {
            format!("every {what}")
        } else {
            escape(&names.join(", "))
        }
    };
    let site = pages::site();
    let c = &site.messages.app.agents;
    let binding = key
        .agent_id
        .as_ref()
        .map(|agent| {
            format!(
                "{}: <a href=\"/app/agents/{}\">{}</a>",
                escape(&c.key_agent),
                crate::web::encode(agent),
                escape(agent)
            )
        })
        .unwrap_or_else(|| escape(&c.key_unbound));
    let actions = if key.live(now) {
        format!(
            "<div class=\"ticket-actions\"><form method=\"post\" action=\"/app/keys/{id}/rotate\">\
<button type=\"submit\">Replace</button></form>\
<form method=\"post\" action=\"/app/keys/{id}/revoke\"><button class=\"deny\" type=\"submit\">Revoke</button></form></div>"
        )
    } else {
        String::new()
    };
    format!(
        "<li id=\"{id}\"><strong>{name}</strong> <span class=\"meta\">…{hint}</span>\
<p>{binding}</p><p class=\"ask-tags\">{scopes}</p>\
<p class=\"meta\">{state} · last used {used}</p>\
<p class=\"meta\">Projects: {projects} · Machines: {machines}</p>{actions}</li>",
        name = escape(&key.name),
        hint = escape(&key.hint),
        projects = limited("project", &key.projects),
        machines = limited("machine", &key.machines),
    )
}

fn checks(field: &str, values: &[String]) -> String {
    values
        .iter()
        .map(|value| {
            format!(
                "<label class=\"check\"><input type=\"checkbox\" name=\"{field}\" value=\"{v}\"><span>{v}</span></label>",
                v = escape(value)
            )
        })
        .collect()
}

async fn agent_choice(
    accounts: &crate::identity::accounts::Accounts,
    subject: &Subject,
    selected: Option<&str>,
) -> devagotchi_contracts::BridgeResult<String> {
    let site = pages::site();
    let c = &site.messages.app.agents;
    let mut options = format!("<option value=\"\">{}</option>", escape(&c.key_unbound));
    let mut after = None;
    let mut found = selected.is_none();
    loop {
        let agents = accounts
            .agents(&subject.id, &subject.id, after.as_deref())
            .await?;
        for agent in agents.iter().take(100).filter(|agent| agent.enabled) {
            let is_selected = selected == Some(agent.id.as_str());
            found |= is_selected;
            options.push_str(&format!(
                "<option value=\"{}\"{}>{}</option>",
                escape(&agent.id),
                if is_selected { " selected" } else { "" },
                escape(&agent.name)
            ));
        }
        if agents.len() <= 100 {
            break;
        }
        after = Some(agents[99].id.clone());
    }
    if !found {
        return Err(devagotchi_contracts::BridgeError::new(
            "not_found",
            "selected agent is unavailable",
        ));
    }
    Ok(format!(
        "<label>{}<select name=\"agent_id\">{options}</select></label><p class=\"note\">{}</p>",
        escape(&c.key_agent),
        escape(&c.key_help)
    ))
}

async fn draw(
    state: &AppState,
    subject: &Subject,
    once: Option<(ApiKey, String)>,
    selected: Option<&str>,
) -> Response {
    let (Some(hub), Some(tenant)) = (&state.passkeys, tenant(subject)) else {
        return no_keys();
    };
    let now = (crate::agenthub::system_clock()() / 1000).max(0) as u64;
    let list = match hub.accounts().keys(tenant, &subject.id).await {
        Ok(keys) => {
            let rows: String = keys.iter().map(|key| row(key, now)).collect();
            if rows.is_empty() {
                "<p class=\"empty\">No keys yet.</p>".to_string()
            } else {
                format!("<ul class=\"rows\" id=\"keys\">{rows}</ul>")
            }
        }
        Err(error) => {
            eprintln!("devagotchi hub: api keys: {error}");
            if once.is_none() {
                return unavailable();
            }
            "<p role=\"alert\">Your key was issued, but the key list could not be loaded. \
Copy your new key above now. Reload this page later to see your keys.</p>"
                .to_string()
        }
    };
    let scopes: String = Scope::ALL
        .iter()
        .map(|scope| {
            format!(
                "<label class=\"check\"><input type=\"checkbox\" name=\"scope\" value=\"{}\"><span>{}</span></label>",
                scope.as_str(),
                escape(scope.title())
            )
        })
        .collect();
    let durations: String = DURATIONS
        .iter()
        .map(|(hours, label)| {
            let selected = if *hours == 720 { " selected" } else { "" };
            format!("<option value=\"{hours}\"{selected}>{label}</option>")
        })
        .collect();
    let projects = board_names(state, subject).unwrap_or_default();
    let machines: Vec<String> = state
        .hub
        .machine_status(subject)
        .await
        .into_iter()
        .map(|machine| machine.name)
        .collect();
    let limit = |title: &str, field: &str, values: &[String]| {
        if values.is_empty() {
            String::new()
        } else {
            format!(
                "<h3>{title}</h3><p class=\"note help\">Tick none for every one.</p>{}",
                checks(field, values)
            )
        }
    };
    let shown = once
        .map(|(key, secret)| shown_key(&key, &secret, &state.hub.config.public_url))
        .unwrap_or_default();
    let form = match agent_choice(hub.accounts(), subject, selected).await {
        Ok(agents) => format!(
            "<form class=\"card prefs\" id=\"new-key-form\" method=\"post\" action=\"/app/keys\"><h2>Make a key</h2>\
<label>Name<input type=\"text\" name=\"name\" required maxlength=\"60\" placeholder=\"hermes\"></label>\
{agents}\
<label>Lasts<select name=\"hours\">{durations}</select></label>\
<h3>What it may do</h3>{scopes}{projects}{machines}\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Make key</button></p></form>",
            projects = limit("Only these projects", "project", &projects),
            machines = limit("Only these machines", "machine", &machines),
        ),
        Err(e) => {
            eprintln!("devagotchi hub: key agent choices: {e}");
            format!(
                "<p role=\"alert\">{}</p>",
                escape(&pages::site().messages.app.agents.choices_unavailable)
            )
        }
    };
    let body = format!(
        "<h1>API keys</h1><p class=\"meta\">For agents and tools that cannot sign in themselves. A key can never approve anything.</p>{shown}<section class=\"card\"><h2>Your keys</h2>{list}</section>{form}"
    );
    let mut response = pages::app_page(StatusCode::OK, "API keys", &body, Tab::Me);
    let headers = response.headers_mut();
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

#[derive(serde::Deserialize)]
pub struct Shown {
    agent: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    match person(&state, &headers, "/app/keys").await {
        Ok(subject) => draw(&state, &subject, None, shown.agent.as_deref()).await,
        Err(response) => response,
    }
}

fn refused(why: &str) -> Response {
    pages::message(
        StatusCode::BAD_REQUEST,
        "Not done",
        why,
        Some(("/app/keys", "API keys")),
    )
}

async fn gate(state: &AppState, headers: &HeaderMap) -> Result<Subject, Response> {
    if !same_origin(headers, &state.hub.config.public_url) {
        return Err(pages::message(
            StatusCode::FORBIDDEN,
            "Not from this hub",
            "Manage keys from this hub's own page.",
            Some(("/app/keys", "API keys")),
        ));
    }
    person(state, headers, "/app/keys").await
}

/// Repeated fields (`scope`, `project`, `machine`) are why this reads the raw form.
fn parse(raw: &[u8]) -> Result<NewKey, String> {
    let mut new = NewKey::default();
    for (name, value) in form_urlencoded::parse(raw) {
        let value = value.into_owned();
        match name.as_ref() {
            "name" => new.name = value,
            "hours" => {
                new.hours = value
                    .parse()
                    .map_err(|_| "that is not a duration".to_string())?
            }
            "scope" => new
                .scopes
                .push(Scope::parse(&value).ok_or_else(|| format!("{value:?} is not a scope"))?),
            "project" => new.projects.push(value),
            "machine" => new.machines.push(value),
            "agent_id" => new.agent_id = (!value.is_empty()).then_some(value),
            _ => {}
        }
    }
    Ok(new)
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawForm(raw): RawForm,
) -> Response {
    let subject = match gate(&state, &headers).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let (Some(hub), Some(tenant)) = (&state.passkeys, tenant(&subject)) else {
        return no_keys();
    };
    let new = match parse(&raw) {
        Ok(new) => new,
        Err(why) => return refused(&why),
    };
    match hub.accounts().create_key(tenant, &subject.id, &new).await {
        Ok(made) => draw(&state, &subject, Some(made), None).await,
        Err(error) if error.code == "invalid" => refused(&error.message),
        Err(error) => {
            eprintln!("devagotchi hub: making an api key: {error}");
            unavailable()
        }
    }
}

pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let subject = match gate(&state, &headers).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let (Some(hub), Some(tenant)) = (&state.passkeys, tenant(&subject)) else {
        return no_keys();
    };
    match hub.accounts().revoke_key(tenant, &subject.id, &id).await {
        Ok(()) => see_other("/app/keys", &[]),
        Err(error) if error.code == "not_found" => refused(&error.message),
        Err(error) => {
            eprintln!("devagotchi hub: revoking an api key: {error}");
            unavailable()
        }
    }
}

pub async fn rotate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let subject = match gate(&state, &headers).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let (Some(hub), Some(tenant)) = (&state.passkeys, tenant(&subject)) else {
        return no_keys();
    };
    match hub.accounts().rotate_key(tenant, &subject.id, &id).await {
        Ok(made) => draw(&state, &subject, Some(made), None).await,
        Err(error) if error.code == "not_found" => refused(&error.message),
        Err(error) => {
            eprintln!("devagotchi hub: replacing an api key: {error}");
            unavailable()
        }
    }
}
