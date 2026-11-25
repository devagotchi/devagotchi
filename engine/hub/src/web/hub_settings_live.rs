//! `/admin`'s cards for the settings the hub reads while it runs (`crate::config::live`): one
//! card per group, a form per card, saved to Postgres and in force at once. A setting the
//! config file or its environment variable set is shown, not changed, with where to change it.

use std::collections::HashMap;

use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};

use super::{
    approvals::same_origin,
    hub_settings_owner::owner,
    operator::hidden,
    pages::{self, escape},
    routes::{see_other, unavailable},
};
use crate::{
    AppState,
    config::live::{Group, Kind, SETTINGS, Setting, Value},
};

/// `/admin`: the page, its sign-in and ClickHouse cards (`hub_settings`) and these.
pub(super) fn routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route("/admin", get(super::hub_settings::page))
        .route(
            "/admin/sign-in/{name}",
            post(super::hub_settings::save_sign_in),
        )
        .route(
            "/admin/analytics",
            post(super::hub_settings::save_analytics),
        )
        .route("/admin/settings/{group}", post(save))
        .route("/admin/mail", post(super::hub_settings_mail::save))
        .route("/admin/sinks/{name}", post(super::hub_settings_sinks::save))
        .route(
            "/admin/sinks/{name}/remove",
            post(super::hub_settings_sinks::remove),
        )
}

/// The card for `group`, with the values in force.
pub(super) fn card(state: &AppState, group: Group) -> String {
    let config = &state.hub.config;
    let live = config.live();
    let mut fields = String::new();
    for setting in SETTINGS.iter().filter(|setting| setting.group == group) {
        let value = live.get(setting.key).unwrap_or(Value::Number(0));
        let pinned = config.pinned().contains(setting.key);
        let disabled = if pinned { " disabled" } else { "" };
        let pinned_note = if pinned {
            format!(
                "<p class=\"meta\">Set by this hub's configuration (<code>{}</code>{}). Change it there, or remove it there to set it here.</p>",
                escape(setting.key),
                setting
                    .env
                    .map(|env| format!(" or <code>{env}</code>"))
                    .unwrap_or_default()
            )
        } else {
            String::new()
        };
        match (setting.kind, value) {
            (Kind::Number { min, max, unit }, Value::Number(n)) => fields.push_str(&format!(
                "<label>{label}<input type=\"number\" name=\"{key}\" min=\"{min}\" max=\"{max}\" step=\"1\" value=\"{n}\" required{disabled}></label>\
<p class=\"meta\">{unit}, {min} to {max}. {help}</p>{pinned_note}",
                label = escape(setting.label),
                key = escape(setting.key),
                help = escape(setting.help),
            )),
            (_, Value::Switch(on)) => fields.push_str(&format!(
                "<label class=\"check\"><input type=\"checkbox\" name=\"{key}\" value=\"on\"{checked}{disabled}><span>{label}</span></label>\
<p class=\"meta\">{help}</p>{pinned_note}",
                key = escape(setting.key),
                checked = if on { " checked" } else { "" },
                label = escape(setting.label),
                help = escape(setting.help),
            )),
            _ => {}
        }
    }
    let all_pinned = SETTINGS
        .iter()
        .filter(|setting| setting.group == group)
        .all(|setting| config.pinned().contains(setting.key));
    let button = if all_pinned {
        String::new()
    } else {
        "<p class=\"actions\"><button class=\"approve\" type=\"submit\">Save</button></p>".into()
    };
    format!(
        "<details class=\"connector\" id=\"{id}\"><summary>{title}</summary><p class=\"note help\">{help}</p>\
<form class=\"prefs\" method=\"post\" action=\"/admin/settings/{id}\">{fields}{button}</form></details>",
        id = group.id(),
        title = escape(group.title()),
        help = escape(group.help()),
    )
}

/// What is set when the hub is deployed, not here, and why: the rest of the map.
pub(super) fn deploy_time_card() -> String {
    let rows: String = crate::config::live::DEPLOY_TIME
        .iter()
        .map(|(key, why)| {
            format!(
                "<li><code>{}</code> <span class=\"meta\">{}</span></li>",
                escape(key),
                escape(why)
            )
        })
        .collect();
    format!(
        "<details class=\"connector\" id=\"deploy-time\"><summary>Set when the hub is deployed</summary><p class=\"note help\">These are in the hub's config file or its environment (<code>/etc/devagotchi/hub.json</code>, <code>.env</code> in a compose stack), because a page could not safely change them or the hub needs them before it can read its settings. Everything else is on this page.</p><ul class=\"rows\">{rows}</ul></details>"
    )
}

/// The values a group's form sends, checked: every setting of the group that is not pinned,
/// an unchecked switch sending nothing (off). All of them or, with every reason, none.
pub(crate) fn read_form(
    group: Group,
    form: &HashMap<String, String>,
    pinned: &crate::config::Pinned,
) -> Result<Vec<(&'static Setting, Value)>, Vec<String>> {
    let mut values = Vec::new();
    let mut wrong = Vec::new();
    for setting in SETTINGS.iter().filter(|setting| setting.group == group) {
        if pinned.contains(setting.key) {
            continue;
        }
        let text = form.get(setting.key).map(String::as_str).unwrap_or("");
        match setting.parse(text) {
            Ok(value) => values.push((setting, value)),
            Err(why) => wrong.push(why),
        }
    }
    if wrong.is_empty() {
        Ok(values)
    } else {
        Err(wrong)
    }
}

pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(person) = owner(&state, &headers).await else {
        return hidden();
    };
    let (Some(hub), Some(group)) = (&state.passkeys, Group::from_id(&group)) else {
        return hidden();
    };
    let back = format!("/admin#{}", group.id());
    let refused = |status: StatusCode, why: &str| {
        pages::message(status, "Not saved", why, Some((&back, "Hub settings")))
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return refused(
            StatusCode::FORBIDDEN,
            "Change this from the hub's own settings page.",
        );
    }
    let config = &state.hub.config;
    let values = match read_form(group, &form, config.pinned()) {
        Ok(values) => values,
        Err(wrong) => {
            return refused(
                StatusCode::BAD_REQUEST,
                &format!("Nothing was saved. {}.", wrong.join(". ")),
            );
        }
    };
    let now = config.live();
    let changes: Vec<(String, String)> = values
        .into_iter()
        .filter(|(setting, value)| now.get(setting.key) != Some(*value))
        .map(|(setting, value)| (setting.key.to_string(), value.text()))
        .collect();
    if !changes.is_empty() {
        let accounts = hub.accounts();
        if let Err(error) = accounts.save_hub_settings(&person.id, &changes).await {
            eprintln!("devagotchi hub: saving settings: {error}");
            return unavailable();
        }
        if let Err(error) = crate::ops::hub_settings::apply_saved(config, accounts).await {
            eprintln!("devagotchi hub: applying settings: {error}");
            return unavailable();
        }
    }
    see_other(
        &format!(
            "/admin?saved={}#{}",
            super::encode(group.title()),
            group.id()
        ),
        &[],
    )
}
