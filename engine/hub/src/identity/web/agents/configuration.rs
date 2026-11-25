use super::*;
use crate::identity::accounts::agents::PrincipalConfiguration;
use devagotchi_agenthub::policy::{
    Effect, Rule, Selector,
    catalog::{PrincipalSettings, Role},
};
use devagotchi_contracts::{BridgeError, BridgeResult};
use std::collections::BTreeMap;

type Fields = BTreeMap<String, String>;

pub(super) async fn page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match context(&state, &headers, None, false).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match ctx.accounts.principal_configuration(&ctx.actor.id).await {
        Ok(config) => render(&config, None, None),
        Err(e) => error(&e),
    }
}

pub(super) async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(fields): Form<Fields>,
) -> Response {
    let ctx = match context(&state, &headers, None, true).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !ctx.owner {
        return crate::tenancy::web::operator::hidden();
    }
    let result = apply(&ctx, &fields).await;
    match result {
        Ok(replayed) => crate::web::app::requests::redirect("/app/agents/configuration", replayed),
        Err(e)
            if matches!(
                e.code.as_str(),
                "invalid" | "version_conflict" | "in_use" | "self_lockout" | "policy_denied"
            ) =>
        {
            match ctx.accounts.principal_configuration(&ctx.actor.id).await {
                Ok(config) => render(&config, Some(&fields), Some(&e)),
                Err(e) => error(&e),
            }
        }
        Err(e) => error(&e),
    }
}

fn get<'a>(fields: &'a Fields, key: &str) -> &'a str {
    fields.get(key).map(String::as_str).unwrap_or_default()
}
fn number(fields: &Fields, key: &str) -> BridgeResult<i64> {
    get(fields, key)
        .parse()
        .map_err(|_| BridgeError::new("invalid", format!("{key} must be a whole number")))
}
fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn apply(ctx: &Context, fields: &Fields) -> BridgeResult<bool> {
    let id = get(fields, "id");
    let request_key = fields.get("request_id").map(String::as_str);
    let version = number(fields, "version")?;
    let replayed = match get(fields, "operation") {
        "role-save" => {
            ctx.accounts
                .put_principal_role_requested(
                    &ctx.actor.id,
                    &Role {
                        id: id.into(),
                        description: get(fields, "description").into(),
                        version,
                    },
                    request_key,
                )
                .await?
                .replayed
        }
        "role-delete" => {
            ctx.accounts
                .delete_principal_role_requested(&ctx.actor.id, id, version, request_key)
                .await?
                .replayed
        }
        "rule-save" => {
            let effect = match get(fields, "effect") {
                "allow" => Effect::Allow,
                "deny" => Effect::Deny,
                _ => return Err(BridgeError::new("invalid", "choose allow or deny")),
            };
            ctx.accounts
                .put_principal_rule_requested(
                    &ctx.actor.id,
                    &Rule {
                        id: id.into(),
                        name: get(fields, "name").into(),
                        subject: get(fields, "subject").into(),
                        action: get(fields, "action").into(),
                        selector: Selector {
                            projects: lines(get(fields, "projects")),
                            labels: lines(get(fields, "labels")),
                        },
                        effect,
                        priority: number(fields, "priority")?,
                        enabled: get(fields, "enabled") == "true",
                        version,
                    },
                    request_key,
                )
                .await?
                .replayed
        }
        "rule-delete" => {
            ctx.accounts
                .delete_principal_rule_requested(&ctx.actor.id, id, version, request_key)
                .await?
                .replayed
        }
        "settings" => {
            ctx.accounts
                .put_principal_settings_requested(
                    &ctx.actor.id,
                    &PrincipalSettings {
                        kinds: lines(get(fields, "kinds")),
                        wip_limit_default: number(fields, "wip")?,
                        version,
                    },
                    request_key,
                )
                .await?
                .replayed
        }
        _ => return Err(BridgeError::new("invalid", "unknown configuration change")),
    };
    Ok(replayed)
}

fn input(name: &str, label: &str, value: &str, readonly: bool) -> String {
    format!(
        "<label>{}<input name=\"{}\" value=\"{}\"{}></label>",
        escape(label),
        name,
        escape(value),
        if readonly { " readonly" } else { "" }
    )
}
fn area(name: &str, label: &str, value: &str) -> String {
    format!(
        "<label>{}<textarea name=\"{}\" rows=\"3\">{}</textarea></label>",
        escape(label),
        name,
        escape(value)
    )
}
fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{name}\" value=\"{}\">",
        escape(value)
    )
}
fn begin(operation: &str, version: i64, request: Option<&str>) -> String {
    format!(
        "<form class=\"stack card\" method=\"post\" action=\"/app/agents/configuration\">{}{}{}",
        form::request_field(Some(request.unwrap_or(&form::request_id()))),
        hidden("operation", operation),
        hidden("version", &version.to_string())
    )
}
fn end(label: &str) -> String {
    format!("<button type=\"submit\">{}</button></form>", escape(label))
}
fn save_begin(operation: &str, version: i64, draft: Option<&Fields>) -> String {
    // A stale form stays stale until the person reloads and reviews the new version.
    begin(
        operation,
        draft
            .and_then(|d| get(d, "version").parse().ok())
            .unwrap_or(version),
        draft.and_then(|d| d.get("request_id").map(String::as_str)),
    )
}
fn draft_for<'a>(
    draft: Option<&'a Fields>,
    operation: &str,
    id: Option<&str>,
) -> Option<&'a Fields> {
    draft.filter(|draft| {
        get(draft, "operation") == operation && id.is_none_or(|id| get(draft, "id") == id)
    })
}
fn value(draft: Option<&Fields>, key: &str, current: &str) -> String {
    draft.map(|d| get(d, key)).unwrap_or(current).to_string()
}

fn render(
    config: &PrincipalConfiguration,
    draft: Option<&Fields>,
    error: Option<&BridgeError>,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.agents;
    let mut body = format!(
        "<div class=\"stack\"><a href=\"/app/agents\">{}</a><h1>{}</h1>",
        escape(&c.back),
        escape(&c.configuration)
    );
    if let Some(e) = error {
        body.push_str(&format!(
            "<p role=\"alert\">{} {}</p>",
            escape(&c.refused),
            escape(&e.message)
        ));
    }
    let d = draft_for(draft, "settings", None);
    body.push_str(
        &(save_begin("settings", config.settings.version, d)
            + &area(
                "kinds",
                &c.kinds,
                &value(d, "kinds", &config.settings.kinds.join("\n")),
            )
            + &format!("<p class=\"note\">{}</p>", escape(&c.kinds_help))
            + &input(
                "wip",
                &c.default_wip,
                &value(d, "wip", &config.settings.wip_limit_default.to_string()),
                false,
            )
            + &end(&c.save_settings)),
    );
    body.push_str(&format!("<h2>{}</h2>", escape(&c.roles)));
    for role in config.roles.iter().map(Some).chain(std::iter::once(None)) {
        let id = role.map(|r| r.id.as_str()).unwrap_or_default();
        let version = role.map(|r| r.version).unwrap_or(0);
        let d = draft_for(draft, "role-save", role.map(|r| r.id.as_str()))
            .filter(|d| role.is_some() || get(d, "version") == "0");
        body.push_str(
            &(save_begin("role-save", version, d)
                + &input("id", &c.id, &value(d, "id", id), role.is_some())
                + &input(
                    "description",
                    &c.role_description,
                    &value(
                        d,
                        "description",
                        role.map(|r| r.description.as_str()).unwrap_or_default(),
                    ),
                    false,
                )
                + &end(if role.is_some() {
                    &c.save_role
                } else {
                    &c.new_role
                })),
        );
        if role.is_some() {
            body.push_str(
                &(save_begin(
                    "role-delete",
                    version,
                    draft_for(draft, "role-delete", Some(id)),
                ) + &hidden("id", id)
                    + &end(&c.remove)),
            );
        }
    }
    body.push_str(&format!(
        "<h2>{}</h2><p>{}</p>",
        escape(&c.policies),
        escape(&c.policy_help)
    ));
    for rule in config.rules.iter().map(Some).chain(std::iter::once(None)) {
        let id = rule.map(|r| r.id.as_str()).unwrap_or_default();
        let version = rule.map(|r| r.version).unwrap_or(0);
        let d = draft_for(draft, "rule-save", rule.map(|r| r.id.as_str()))
            .filter(|d| rule.is_some() || get(d, "version") == "0");
        let mut form = save_begin("rule-save", version, d)
            + &input("id", &c.id, &value(d, "id", id), rule.is_some());
        for (key, label, current) in [
            (
                "name",
                &c.rule_name,
                rule.map(|r| r.name.as_str()).unwrap_or_default(),
            ),
            (
                "subject",
                &c.subject,
                rule.map(|r| r.subject.as_str()).unwrap_or("agent"),
            ),
            (
                "action",
                &c.action,
                rule.map(|r| r.action.as_str())
                    .unwrap_or("task.claim:ready"),
            ),
        ] {
            form += &input(key, label, &value(d, key, current), false);
        }
        form += &format!(
            "<p class=\"note\">{} {}</p>",
            escape(&c.subject_help),
            escape(&c.action_help)
        );
        form += &area(
            "projects",
            &c.projects,
            &value(
                d,
                "projects",
                &rule
                    .map(|r| r.selector.projects.join("\n"))
                    .unwrap_or_default(),
            ),
        );
        form += &area(
            "labels",
            &c.labels,
            &value(
                d,
                "labels",
                &rule
                    .map(|r| r.selector.labels.join("\n"))
                    .unwrap_or_default(),
            ),
        );
        form += &format!("<p class=\"note\">{}</p>", escape(&c.selector_help));
        let effect = value(
            d,
            "effect",
            if rule.is_some_and(|r| r.effect == Effect::Deny) {
                "deny"
            } else {
                "allow"
            },
        );
        form += &format!(
            "<label>{}<select name=\"effect\"><option value=\"allow\"{}>{}</option><option value=\"deny\"{}>{}</option></select></label>",
            escape(&c.effect),
            if effect == "allow" { " selected" } else { "" },
            escape(&c.allow),
            if effect == "deny" { " selected" } else { "" },
            escape(&c.deny)
        );
        form += &input(
            "priority",
            &c.priority,
            &value(
                d,
                "priority",
                &rule.map(|r| r.priority).unwrap_or(0).to_string(),
            ),
            false,
        );
        let enabled = d
            .map(|d| get(d, "enabled") == "true")
            .unwrap_or(rule.is_none_or(|r| r.enabled));
        form += &format!(
            "<label><input type=\"checkbox\" name=\"enabled\" value=\"true\"{}>{}</label>",
            if enabled { " checked" } else { "" },
            escape(&c.rule_enabled)
        );
        form += &end(if rule.is_some() {
            &c.save_rule
        } else {
            &c.new_rule
        });
        body += &form;
        if rule.is_some() {
            body += &(save_begin(
                "rule-delete",
                version,
                draft_for(draft, "rule-delete", Some(id)),
            ) + &hidden("id", id)
                + &end(&c.remove));
        }
    }
    body += "</div>";
    let status = error
        .map(|e| {
            if matches!(e.code.as_str(), "version_conflict" | "in_use") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            }
        })
        .unwrap_or(StatusCode::OK);
    pages::app_page(status, &c.configuration, &body, Tab::Me)
}
