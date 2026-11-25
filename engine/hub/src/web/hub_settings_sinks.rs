//! `/admin` → Data and signals → Signal sinks (hub-settings M5): where the hub pushes its signals
//! (`notify/signals/`, `docs/signals.md`), set on the page beside the ones the config file names.
//! Each is kept in Postgres with its token and secret sealed (`notify::signal_sinks`) and runs
//! the moment it is saved; one removed stops at once. A config file's sink is shown, not changed,
//! and its name cannot be taken.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};

use super::{
    approvals::same_origin,
    hub_settings_owner::owner,
    operator::hidden,
    pages::{self, escape},
    passkeys::HubSignIn,
    routes::{see_other, unavailable},
};
use crate::{
    AppState,
    notify::signal_sinks::{StoredSink, sealed_as, sink_of},
};

const SOURCES: [(&str, &str); 3] = [
    ("board", "Boards: tickets, notes, claims, approvals"),
    (
        "tenancy",
        "Accounts and organisations: members, roles, invites, settings changed",
    ),
    ("audit", "Every tool call an AI made, and how it ended"),
];

/// Start every sink saved on `/admin` that is on; the ones running from the page stop first.
/// Run when the hub starts and after each save or removal.
pub async fn start_saved(
    hub: &HubSignIn,
    boards: &crate::agenthub::mcp::Boards,
    store: &crate::store::Store,
    audit: &Arc<crate::ops::audit::Audit>,
) -> devagotchi_contracts::BridgeResult<()> {
    boards.stop_page_sinks();
    let taken = boards.config_sink_names();
    for stored in hub.accounts().stored_sinks().await? {
        if !stored.enabled || taken.contains(&stored.name) {
            continue;
        }
        match sink_of(&stored, hub.sealer(), store.clone(), audit.clone()) {
            Ok(sink) => boards.start_sink(&stored.name, Arc::new(sink), false),
            Err(error) => eprintln!(
                "devagotchi hub: signal sink {} from the settings page is off: {}",
                stored.name, error.message
            ),
        }
    }
    Ok(())
}

fn sources_boxes(prefix: &str, chosen: &[String]) -> String {
    SOURCES
        .iter()
        .map(|(source, words)| {
            let on = chosen.is_empty() || chosen.iter().any(|c| c == source);
            format!(
                "<label class=\"check\"><input type=\"checkbox\" name=\"{prefix}source.{source}\" value=\"on\"{}><span>{}</span></label>",
                if on { " checked" } else { "" },
                escape(words)
            )
        })
        .collect()
}

/// A secret field's hint: kept when one is saved.
fn kept(sealed: Option<&str>, empty: &'static str) -> &'static str {
    if sealed.is_some_and(|s| !s.is_empty()) {
        "Saved. Leave empty to keep it."
    } else {
        empty
    }
}

fn form(stored: Option<&StoredSink>) -> String {
    let (action, name_field) = match stored {
        Some(sink) => (
            format!("/admin/sinks/{}", sink.name),
            format!("<input type=\"hidden\" name=\"name\" value=\"{}\">", escape(&sink.name)),
        ),
        None => (
            "/admin/sinks/new".to_string(),
            "<label>Name<input type=\"text\" name=\"name\" pattern=\"[a-z0-9][a-z0-9-]{0,62}\" maxlength=\"63\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"shippin-logs\" required></label>".to_string(),
        ),
    };
    let full = stored.is_some_and(|s| s.fields == "full");
    format!(
        "<form class=\"prefs\" method=\"post\" action=\"{action}\">{name_field}\
<label>Address<input type=\"url\" name=\"url\" autocomplete=\"off\" spellcheck=\"false\" value=\"{url}\" placeholder=\"https://logs.example.com/ingest\" required maxlength=\"512\"></label>\
<label>Bearer token<input type=\"password\" name=\"token\" autocomplete=\"new-password\" maxlength=\"2048\" placeholder=\"{token}\"></label>\
<label>Signing secret<input type=\"password\" name=\"secret\" autocomplete=\"new-password\" maxlength=\"2048\" placeholder=\"{secret}\"></label>\
<label>What each signal carries<select name=\"fields\"><option value=\"metadata\"{meta}>What happened, to what, by whom, how it ended</option><option value=\"full\"{fullsel}>All of that and its text (ticket text, notes)</option></select></label>\
{sources}<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\"{enabled}><span>Send signals to this sink</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Save</button></p></form>",
        url = escape(stored.map(|s| s.url.as_str()).unwrap_or_default()),
        token = kept(
            stored.map(|s| s.sealed_token.as_str()),
            "If it asks for one"
        ),
        secret = kept(
            stored.map(|s| s.sealed_secret.as_str()),
            "Optional: batches signed with it"
        ),
        meta = if full { "" } else { " selected" },
        fullsel = if full { " selected" } else { "" },
        sources = sources_boxes("", stored.map(|s| s.sources.as_slice()).unwrap_or_default()),
        enabled = if stored.is_none_or(|s| s.enabled) {
            " checked"
        } else {
            ""
        },
    )
}

/// The card: every sink with how it is doing, a form for each set here, and one to add.
pub(super) fn card(state: &AppState, stored: &[StoredSink]) -> String {
    let running = state.hub.boards.sinks();
    let health = |name: &str| {
        running
            .iter()
            .map(|(sink, _)| sink.health())
            .find(|h| h["name"] == name)
    };
    let mut rows = String::new();
    for (sink, from_config) in running.iter().filter(|(_, from_config)| *from_config) {
        let h = sink.health();
        rows.push_str(&format!(
            "<li><strong>{}</strong> <span class=\"meta\">{}</span><p class=\"meta\">Set by this hub's configuration (<code>signals.sinks</code>); change it there.</p></li>",
            escape(h["name"].as_str().unwrap_or("")),
            escape(&summary(&h))
        ));
        let _ = from_config;
    }
    let mut forms = String::new();
    for sink in stored {
        let state_words = match health(&sink.name) {
            Some(h) => summary(&h),
            None if sink.enabled => "not running: see the hub's log".into(),
            None => "off".into(),
        };
        forms.push_str(&format!(
            "<details class=\"connector\" id=\"sink-{name}\"><summary>{name} <span class=\"tag\">{state}</span></summary>{form}\
<form method=\"post\" action=\"/admin/sinks/{name}/remove\"><p class=\"actions\"><button type=\"submit\">Remove this sink</button></p></form></details>",
            name = escape(&sink.name),
            state = escape(&state_words),
            form = form(Some(sink)),
        ));
    }
    format!(
        "<details class=\"connector\" id=\"sinks\"><summary>Signal sinks <span class=\"tag\">{count}</span></summary>\
<p class=\"note help\">Where this hub pushes what happens on it, as it happens: an HTTPS address that takes batches of signals as NDJSON (docs/signals.md), such as a log service or your own collector. Each sink keeps its own place; one that is down falls behind and catches up, and never holds back the hub. Tokens and secrets are encrypted in this hub's Postgres.</p>\
<ul class=\"rows\">{rows}</ul>{forms}<details class=\"connector\" id=\"sink-new\"><summary>Add a sink</summary>{add}</details></details>",
        count = running.len(),
        add = form(None),
    )
}

fn summary(health: &serde_json::Value) -> String {
    let sent = health["sent"].as_u64().unwrap_or(0);
    match health["last_error"].as_str() {
        Some(error) => format!("{sent} sent; refusing: {error}"),
        None => format!("{sent} sent"),
    }
}

/// What a sink's form sends, checked as the config file's sinks are; else why not.
fn read(
    name: &str,
    form: &HashMap<String, String>,
    current: Option<&StoredSink>,
    hub: &HubSignIn,
) -> Result<StoredSink, String> {
    let text = |key: &str| form.get(key).map(|v| v.trim()).unwrap_or("");
    let url = text("url").trim_end_matches('/').to_string();
    let sources: Vec<String> = SOURCES
        .iter()
        .filter(|(source, _)| text(&format!("source.{source}")) == "on")
        .map(|(source, _)| source.to_string())
        .collect();
    if sources.is_empty() {
        return Err("Choose at least one kind of signal.".into());
    }
    let sources = if sources.len() == SOURCES.len() {
        Vec::new()
    } else {
        sources
    };
    let fields = match text("fields") {
        "full" => "full",
        _ => "metadata",
    };
    // The config file's own rules for a sink: its name and address.
    let check = crate::config::SignalsConfig {
        sinks: vec![crate::config::SinkConfig {
            name: name.to_string(),
            url: url.clone(),
            token_env: None,
            secret_env: None,
            fields: crate::config::SinkFields::Metadata,
            sources: sources.clone(),
        }],
    };
    check.check().map_err(|why| {
        format!(
            "{}.",
            why.trim_start_matches(&format!("signals.sinks {name}: "))
        )
    })?;
    let seal = |given: &str, what: &str, kept: Option<&str>| {
        if given.is_empty() {
            kept.unwrap_or_default().to_string()
        } else {
            hub.sealer().seal(&sealed_as(name, what), given)
        }
    };
    let same = current.filter(|c| c.url == url);
    let (token, secret) = (text("token"), text("secret"));
    if token.len() > 2048
        || secret.len() > 2048
        || token.chars().chain(secret.chars()).any(char::is_control)
    {
        return Err("Check the token and the secret.".into());
    }
    Ok(StoredSink {
        name: name.to_string(),
        url,
        sealed_token: seal(token, "token", same.map(|c| c.sealed_token.as_str())),
        sealed_secret: seal(secret, "secret", same.map(|c| c.sealed_secret.as_str())),
        fields: fields.to_string(),
        sources,
        enabled: text("enabled") == "on",
    })
}

pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(person) = owner(&state, &headers).await else {
        return hidden();
    };
    let Some(hub) = &state.passkeys else {
        return hidden();
    };
    let refused = |status: StatusCode, why: &str| {
        pages::message(
            status,
            "Not saved",
            why,
            Some(("/admin#sinks", "Hub settings")),
        )
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return refused(
            StatusCode::FORBIDDEN,
            "Change this from the hub's own settings page.",
        );
    }
    let accounts = hub.accounts();
    let stored = match accounts.stored_sinks().await {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("devagotchi hub: signal sinks: {error}");
            return unavailable();
        }
    };
    let name = if path == "new" {
        form.get("name")
            .map(|n| n.trim().to_string())
            .unwrap_or_default()
    } else {
        path.clone()
    };
    let current = stored.iter().find(|s| s.name == name);
    if path == "new" && current.is_some() {
        return refused(
            StatusCode::CONFLICT,
            "A sink of that name is set up already; change it in its own card.",
        );
    }
    if path != "new" && current.is_none() {
        return hidden();
    }
    if state.hub.boards.config_sink_names().contains(&name) {
        return refused(
            StatusCode::CONFLICT,
            "This hub's configuration has a sink of that name; choose another.",
        );
    }
    let sink = match read(&name, &form, current, hub) {
        Ok(sink) => sink,
        Err(why) => return refused(StatusCode::BAD_REQUEST, &format!("Not saved: {why}")),
    };
    if let Err(error) = accounts.save_sink(&person.id, &sink).await {
        eprintln!("devagotchi hub: saving a signal sink: {error}");
        return unavailable();
    }
    if let Err(error) = restart(&state, hub).await {
        eprintln!("devagotchi hub: starting signal sinks: {error}");
        return unavailable();
    }
    see_other(
        &format!(
            "/admin?saved={}#sink-{}",
            super::encode(&name),
            super::encode(&name)
        ),
        &[],
    )
}

pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    let Some(person) = owner(&state, &headers).await else {
        return hidden();
    };
    let Some(hub) = &state.passkeys else {
        return hidden();
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return pages::message(
            StatusCode::FORBIDDEN,
            "Not removed",
            "Change this from the hub's own settings page.",
            Some(("/admin#sinks", "Hub settings")),
        );
    }
    match hub.accounts().remove_sink(&person.id, &name).await {
        Ok(true) => {}
        Ok(false) => return hidden(),
        Err(error) => {
            eprintln!("devagotchi hub: removing a signal sink: {error}");
            return unavailable();
        }
    }
    if let Err(error) = restart(&state, hub).await {
        eprintln!("devagotchi hub: starting signal sinks: {error}");
        return unavailable();
    }
    see_other("/admin?saved=Signal%20sinks#sinks", &[])
}

async fn restart(state: &AppState, hub: &HubSignIn) -> devagotchi_contracts::BridgeResult<()> {
    let store = hub.accounts().store().clone();
    start_saved(hub, &state.hub.boards, &store, &state.hub.audit).await
}
