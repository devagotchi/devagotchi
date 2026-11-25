//! `/admin`: the hub's own settings, for its owner, grouped by what they are for and guided
//! step by step. Sign-in providers are set here (Google, GitHub, Sign in with Shippin through
//! Nomen, and any other OpenID Connect provider), and so is the ClickHouse AgentHub sends its
//! board events to; both are stored in Postgres with their secrets sealed and take effect
//! without a restart. The page also says where AgentHub keeps its boards (which Postgres,
//! which schema) and how to move them to a Postgres of your own. The other sections say what
//! is set and where to change it. Anyone but the owner gets an unknown path's answer.

use crate::config::live::Group;
use crate::ops::warehouse;
use crate::{
    AppState, config::providers::ProviderConfig, identity::accounts::sign_in_settings::NewSignIn,
    ops::analytics_settings::StoredClickHouse,
};
use crate::{
    agenthub::web::approvals::same_origin,
    identity::web::sign_in_cards,
    settings::web::owner::{AnalyticsForm, Shown, SignInForm, owner},
    tenancy::web::operator::hidden,
    web::{
        pages::{self, Tab, escape},
        routes::{see_other, unavailable},
    },
};
use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};

pub(crate) use super::owner::is_owner;

/// Where AgentHub keeps its boards: this hub's own store, and how to point it at a Postgres
/// the person already runs (it cannot move itself while it runs: its settings are in it).
fn database_card(store: &crate::store::Store) -> String {
    let move_steps = "<ol class=\"connect-steps\">\
<li>Make a database for it in your Postgres, or use one that holds other things: the hub keeps its tables in a schema of their own and never touches the rest.</li>\
<li>Give it a login that may create a schema in that database. The hub then makes a role without BYPASSRLS for itself, so every tenant's rows stay apart.</li>\
<li>Set <code>DEVAGOTCHI_HUB_STORAGE_URL=postgres://user@host:5432/database</code> (the password from your secret store, not a file) and, for a schema other than <code>devagotchi</code>, <code>DEVAGOTCHI_HUB_STORAGE_SCHEMA</code>. Restart the hub.</li>\
<li>A new database starts empty. To keep this hub's boards and accounts, restore its backup into the new database first.</li></ol>";
    let (status, rows) = (
        "<span class=\"tag kind\">Postgres</span>",
        format!(
            "<ul class=\"rows\">{}{}{}</ul>",
            row("Database", store.place(), ""),
            row("Schema", store.schema().unwrap_or("devagotchi"), ""),
            row(
                "Tenants kept apart by the database",
                if store.enforced() { "Yes" } else { "No" },
                if store.enforced() {
                    ""
                } else {
                    "The hub's login may bypass row-level security. Run it as a role without superuser or BYPASSRLS."
                }
            ),
        ),
    );
    format!(
        "<details class=\"connector\" id=\"database\"><summary>Where AgentHub keeps its boards {status}</summary>{rows}\
<p class=\"meta\">To use a Postgres you already run:</p>{move_steps}</details>"
    )
}

/// The ClickHouse AgentHub sends its board events to, for statistics: set here, or by the
/// hub's configuration (then shown, not changed, here).
fn analytics_card(
    from_config: Option<&crate::config::ClickHouseConfig>,
    stored: Option<&StoredClickHouse>,
    health: Option<&serde_json::Value>,
) -> String {
    let ready = health.is_some_and(|h| h["ready"] == true);
    let status = match (from_config.is_some(), health.is_some(), stored) {
        (true, _, _) => "<span class=\"tag kind\">On, from this hub's configuration</span>",
        (false, true, _) if ready => "<span class=\"tag kind\">On</span>",
        (false, true, _) => "<span class=\"tag\">On, not reaching ClickHouse</span>",
        (false, false, Some(_)) => "<span class=\"tag\">Off</span>",
        (false, false, None) => "<span class=\"tag\">Not set up</span>",
    };
    let live = health
        .map(|h| {
            let pending = h["pending"]
                .as_i64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unknown".into());
            let last = h["last_export_at"]
                .as_u64()
                .map(pages::utc)
                .unwrap_or_else(|| "not yet".into());
            let error = h["last_error"]
                .as_str()
                .map(|e| row("Last refusal", e, ""))
                .unwrap_or_default();
            format!(
                "<ul class=\"rows\">{}{}{error}</ul>",
                row("Events waiting in Postgres", &pending, ""),
                row("Last sent", &last, ""),
            )
        })
        .unwrap_or_default();
    let steps = "<ol class=\"connect-steps\">\
<li>Use a ClickHouse this hub can reach: ClickHouse Cloud, or your own server. Give its HTTP interface: <code>https://host:8443</code> for Cloud, <code>http://host:8123</code> for your own.</li>\
<li>Give the hub a user that may create a database, or create the database yourself and give the user rights on it.</li>\
<li>Check and save. The hub checks it at once, makes the one table it needs, and sends your boards' events from then on. Your boards never wait for it: while ClickHouse is down the events wait in Postgres.</li></ol>";
    let body = if let Some(config) = from_config {
        format!(
            "<ul class=\"rows\">{}{}</ul><p class=\"note help\">Set by this hub's configuration (<code>DEVAGOTCHI_CLICKHOUSE_URL</code> or <code>clickhouse</code> in its config file). Change it there, or remove it there to set it here.</p>",
            row("ClickHouse", &config.url, ""),
            row("Database", &config.database, "")
        )
    } else {
        let url = stored.map(|s| s.url.as_str()).unwrap_or_default();
        let database = stored.map(|s| s.database.as_str()).unwrap_or("devagotchi");
        let user = stored.map(|s| s.user.as_str()).unwrap_or_default();
        let password_hint = if stored.is_some_and(|s| !s.sealed_password.is_empty()) {
            "Saved. Leave empty to keep it."
        } else {
            "If the user has one"
        };
        let enabled = stored.map(|s| s.enabled).unwrap_or(true);
        format!(
            "{steps}<form class=\"prefs\" method=\"post\" action=\"/admin/analytics\">\
<label>ClickHouse address<input type=\"url\" name=\"url\" autocomplete=\"off\" spellcheck=\"false\" value=\"{url}\" placeholder=\"https://\" required></label>\
<label>Database<input type=\"text\" name=\"database\" pattern=\"[a-z_][a-z0-9_]{{0,62}}\" maxlength=\"63\" autocomplete=\"off\" spellcheck=\"false\" value=\"{database}\" required></label>\
<label>User<input type=\"text\" name=\"user\" maxlength=\"128\" autocomplete=\"off\" spellcheck=\"false\" value=\"{user}\" placeholder=\"default\"></label>\
<label>Password<input type=\"password\" name=\"password\" autocomplete=\"new-password\" placeholder=\"{password_hint}\"></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\"{checked}><span>Send board events to this ClickHouse</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Check and save</button></p></form>",
            url = escape(url),
            database = escape(database),
            user = escape(user),
            checked = if enabled { " checked" } else { "" },
        )
    };
    format!(
        "<details class=\"connector\" id=\"analytics\"{open}><summary>Statistics: ClickHouse {status}</summary>{live}{body}</details>",
        open = if health.is_some() || from_config.is_some() {
            ""
        } else {
            " open"
        },
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
    let cards = sign_in_cards::render(&state, &stored, &hub.providers);
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
    let store = hub.accounts().store();
    let stored_clickhouse = match hub.accounts().clickhouse_setting().await {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("devagotchi hub: analytics settings: {error}");
            return unavailable();
        }
    };
    let health = match state.hub.boards.warehouse() {
        Some(warehouse) => Some(warehouse.health().await),
        None => None,
    };
    let database = database_card(store);
    let analytics = analytics_card(
        state.hub.config.clickhouse.as_ref(),
        stored_clickhouse.as_ref(),
        health.as_ref(),
    );
    let stored_sinks = match hub.accounts().stored_sinks().await {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("devagotchi hub: signal sinks: {error}");
            return unavailable();
        }
    };
    let stored_mail = match hub.accounts().stored_mail().await {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("devagotchi hub: mail settings: {error}");
            return unavailable();
        }
    };
    let live = |group| super::live::card(&state, group);
    // Visits are counted only on a hosted hub, so only its owner sees how long they are kept.
    // A single environment offers local users; organizations remain on hosted hubs.
    let (visits, people_chip, people) = if state.hub.config.multi_tenant() {
        let orgs = row(
            "Organisations",
            "Members, teams, roles and invites",
            "<a href=\"/orgs\">Manage organisations</a>",
        );
        (
            live(Group::Analytics),
            "<a class=\"chip\" href=\"#people\">People</a>".to_string(),
            format!(
                "<section class=\"card\" id=\"people\"><h2>People</h2>{}<ul class=\"rows\">{orgs}</ul></section>",
                live(Group::SignUp)
            ),
        )
    } else {
        crate::tenancy::web::people::section()
    };
    let clients = format!("{} clients", devagotchi_connectors::all().len());
    let body = format!(
        "<h1>Hub settings</h1><p class=\"meta\">For this hub's owner. Changes apply as soon as you save.</p>{flash}\
<nav class=\"chips\" aria-label=\"Sections\"><a class=\"chip\" href=\"#sign-in\">Sign-in</a>\
<a class=\"chip\" href=\"#access\">AI access</a>{people_chip}\
<a class=\"chip\" href=\"#running\">Machines and workflows</a>\
<a class=\"chip\" href=\"#data\">Data and signals</a></nav>\
<section class=\"card\" id=\"sign-in\"><h2>Sign-in</h2><p class=\"note help\">How people sign in to this hub: \
Google, GitHub, Shippin, or any OpenID Connect provider. Shippin's switch is on once you save an issuer, or once install.sh --nomen set one; turn that switch off and the button goes. The issuer is the one you type; nothing is filled in for you. Secrets are encrypted in this hub's Postgres.</p>{cards}{passwords}\
{mail}</section>\
<section class=\"card\" id=\"access\"><h2>AI access</h2><ul class=\"rows\">{connect}{keys}</ul></section>\
{people}\
<section class=\"card\" id=\"running\"><h2>Machines and workflows</h2>{machines}{workflows}</section>\
<section class=\"card\" id=\"data\"><h2>Data and signals</h2><p class=\"note help\">Where AgentHub keeps its boards, \
and where it sends their events for statistics.</p><p><a href=\"/admin/audit\">{audit_title}</a></p>{database}{analytics}{visits}{signals}{deploy_time}</section>",
        audit_title = escape(&pages::site().messages.app.hub_audit.title),
        deploy_time = super::live::deploy_time_card(),
        passwords = live(Group::Passwords),
        machines = live(Group::Machines),
        workflows = live(Group::Workflows),
        mail = crate::settings::web::mail::card(hub, stored_mail.as_ref()),
        connect = row(
            "Connect an AI",
            &clients,
            "<a href=\"/connect\">Exact steps for each client</a>"
        ),
        keys = row(
            "API keys",
            "For agents that cannot sign in",
            "<a href=\"/app/keys\">Manage keys</a>"
        ),
        signals = crate::settings::web::sinks::card(&state, &stored_sinks),
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
    let (name, kind) = if let Some((guide_name, kind)) = sign_in_cards::guide(&path) {
        (guide_name.to_string(), kind)
    } else if path == "new" {
        let name = form.name.trim().to_string();
        if sign_in_cards::is_guide(&name) || current.iter().any(|s| s.name == name) {
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

pub async fn save_analytics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<AnalyticsForm>,
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
            Some(("/admin#analytics", "Hub settings")),
        )
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return refused(
            StatusCode::FORBIDDEN,
            "Change this from the hub's own settings page.",
        );
    }
    if state.hub.config.clickhouse.is_some() {
        return refused(
            StatusCode::CONFLICT,
            "This hub's configuration sets ClickHouse (DEVAGOTCHI_CLICKHOUSE_URL or clickhouse in its config file). Change it there, or remove it there to set it here.",
        );
    }
    let accounts = hub.accounts();
    let store = accounts.store().clone();
    let url = form.url.trim().trim_end_matches('/');
    let database = match form.database.trim() {
        "" => "devagotchi",
        database => database,
    };
    let user = form.user.trim();
    let password = form.password.trim();
    let bad = |text: &str, max: usize| text.len() > max || text.chars().any(char::is_control);
    let address_ok = (url.starts_with("https://") || url.starts_with("http://"))
        && !url.contains('@')
        && url
            .parse::<axum::http::Uri>()
            .is_ok_and(|uri| uri.host().is_some());
    if !address_ok || bad(url, 512) {
        return refused(
            StatusCode::BAD_REQUEST,
            "Give ClickHouse's HTTP interface, like https://host:8443 or http://host:8123, with no user or password in it.",
        );
    }
    if !crate::store::valid_schema(database) {
        return refused(
            StatusCode::BAD_REQUEST,
            "A database name is 1 to 63 of a-z, 0-9 and _, not starting with a digit.",
        );
    }
    if bad(user, 128) || bad(password, 1024) || user.contains(char::is_whitespace) {
        return refused(StatusCode::BAD_REQUEST, "Check the user and password.");
    }
    let current = match accounts.clickhouse_setting().await {
        Ok(current) => current,
        Err(error) => {
            eprintln!("devagotchi hub: analytics settings: {error}");
            return unavailable();
        }
    };
    let sealer = hub.sealer();
    // An empty password keeps the saved one, when the address and user are the same.
    let kept = current
        .as_ref()
        .filter(|c| c.url == url && c.user == user)
        .map(|c| c.sealed_password.clone())
        .unwrap_or_default();
    let sealed = if password.is_empty() {
        kept
    } else {
        sealer.seal(warehouse::SEALED_AS, password)
    };
    let setting = StoredClickHouse {
        url: url.to_string(),
        database: database.to_string(),
        user: user.to_string(),
        sealed_password: sealed,
        enabled: form.enabled.is_some(),
        updated_at: 0,
        updated_by: String::new(),
    };
    if setting.enabled {
        let password = sealer.open_value(warehouse::SEALED_AS, &setting.sealed_password);
        if let Err(error) =
            warehouse::start_checked(&state.hub.boards, store, &setting.config(), password).await
        {
            return refused(
                StatusCode::BAD_REQUEST,
                &format!(
                    "ClickHouse did not take it, so nothing was saved: {}",
                    error.message
                ),
            );
        }
    } else {
        state.hub.boards.set_warehouse(None);
    }
    if let Err(error) = accounts.save_clickhouse(&person.id, &setting).await {
        eprintln!("devagotchi hub: saving the ClickHouse setting: {error}");
        return unavailable();
    }
    see_other("/admin?saved=ClickHouse#analytics", &[])
}
