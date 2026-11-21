use super::*;
use devagotchi_agenthub::hosted::registry::{self, ClaimCursor};
use devagotchi_contracts::{BridgeError, BridgeResult};
use std::collections::BTreeMap;

pub(super) fn list(ctx: &Context, agents: &[Agent], shown: &Shown) -> Response {
    let site = pages::site();
    let c = &site.messages.app.agents;
    let mut rows = String::new();
    for agent in agents.iter().take(100) {
        rows.push_str(&format!("<li><a href=\"/app/agents/{}?account={}\">{}</a><p>{} · {}</p><p>{}: {}</p><p>{}: {}</p>",
            encode(&agent.id),encode(&agent.account),escape(&agent.name),escape(&agent.kind),escape(if agent.enabled {&c.active}else{&c.disabled}),
            escape(&c.wip),escape(&agent.wip_limit.map(|n|n.to_string()).unwrap_or(c.default_limit.clone())),
            escape(&c.last_seen),agent.last_seen.map(pages::ago).unwrap_or_else(||escape(&c.never))));
        let roles = agent
            .roles
            .iter()
            .map(|role| {
                format!(
                    "<a href=\"/app/agents?role={}\">{}</a>",
                    encode(role),
                    escape(role)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        rows.push_str(&format!(
            "<p>{}: {} · {}: {roles}</p></li>",
            escape(&c.account),
            escape(&agent.account),
            escape(&c.roles)
        ));
    }
    let list = if rows.is_empty() {
        format!("<p>{}</p>", escape(&c.empty))
    } else {
        format!("<ul class=\"rows\" id=\"agents\">{rows}</ul>")
    };
    let add = if ctx.owner {
        format!(
            "<p><a class=\"button\" href=\"/app/agents/new?account={}\">{}</a> <a href=\"/app/agents/configuration\">{}</a></p>",
            encode(&ctx.account),
            escape(&c.new),
            escape(&c.settings)
        )
    } else {
        String::new()
    };
    let more = if agents.len() > 100 {
        format!(
            "<a href=\"/app/agents?after={}{}{}\">{}</a>",
            encode(&format!("{}/{}", agents[99].account, agents[99].id)),
            shown
                .account
                .as_ref()
                .map(|value| format!("&amp;account={}", encode(value)))
                .unwrap_or_default(),
            shown
                .role
                .as_ref()
                .map(|value| format!("&amp;role={}", encode(value)))
                .unwrap_or_default(),
            escape(&c.more)
        )
    } else {
        String::new()
    };
    pages::app_page(
        StatusCode::OK,
        &c.title,
        &format!(
            "<div class=\"stack\"><h1>{}</h1><p>{}</p><p>{}: {}</p>{add}<section class=\"card\">{list}</section>{more}</div>",
            escape(&c.title),
            escape(&c.description),
            escape(&c.account),
            escape(if ctx.owner && shown.account.is_none() {
                &c.title
            } else {
                &ctx.account_name
            })
        ),
        Tab::Me,
    )
}

pub(super) async fn edit(
    ctx: &Context,
    input: &form::AgentForm,
    saved: Option<&str>,
    error: Option<&BridgeError>,
    extra: Option<&str>,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.agents;
    let field = |name: &str, label: &str, value: &str, readonly: bool| {
        format!(
            "<label>{}<input name=\"{}\" value=\"{}\"{}></label>",
            escape(label),
            name,
            escape(value),
            if readonly { " readonly" } else { "" }
        )
    };
    let notice = if let Some(e) = error {
        format!(
            "<p role=\"alert\">{} {}</p>",
            escape(&c.refused),
            escape(&e.message)
        )
    } else if saved.is_some() {
        format!("<p role=\"status\">{}</p>", escape(&c.saved))
    } else {
        String::new()
    };
    let checkbox = format!(
        "<label><input type=\"checkbox\" name=\"enabled\" value=\"true\"{}{}>{}</label>{}",
        if input.enabled { " checked" } else { "" },
        if !ctx.owner { " disabled" } else { "" },
        escape(&c.enabled),
        if !ctx.owner && input.enabled {
            "<input type=\"hidden\" name=\"enabled\" value=\"true\">"
        } else {
            ""
        }
    );
    let key = if input.version > 0 && ctx.account == ctx.actor.id {
        format!(
            "<p><a href=\"/app/keys?agent={}\">{}</a></p>",
            encode(&input.id),
            escape(&c.keys)
        )
    } else {
        String::new()
    };
    let authority = if ctx.owner {
        let config = match ctx.accounts.principal_configuration(&ctx.actor.id).await {
            Ok(config) => config,
            Err(e) => return super::error(&e),
        };
        let kind = if config.settings.kinds.is_empty() {
            field("kind", &c.kind, &input.kind, false)
        } else {
            let mut kinds = config.settings.kinds;
            if !kinds.contains(&input.kind) {
                kinds.push(input.kind.clone());
            }
            let options: String = kinds
                .iter()
                .map(|kind| {
                    format!(
                        "<option value=\"{}\"{}>{}</option>",
                        escape(kind),
                        if kind == &input.kind { " selected" } else { "" },
                        escape(kind)
                    )
                })
                .collect();
            format!(
                "<label>{}<select name=\"kind\">{options}</select></label>",
                escape(&c.kind)
            )
        };
        let selected: Vec<_> = input
            .roles
            .split(',')
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .collect();
        let mut roles: Vec<_> = config
            .roles
            .into_iter()
            .map(|role| (role.id, role.description))
            .collect();
        for id in &selected {
            if !roles.iter().any(|(known, _)| known == id) {
                roles.push((id.to_string(), String::new()));
            }
        }
        let checks: String = roles.iter().map(|(id, description)| format!("<label class=\"check\"><input type=\"checkbox\" name=\"role\" value=\"{}\"{}><span>{}<small>{}</small></span></label>", escape(id), if selected.contains(&id.as_str()) {" checked"} else {""}, escape(id), escape(description))).collect();
        format!(
            "{kind}<fieldset><legend>{}</legend>{checks}</fieldset>",
            escape(&c.roles)
        )
    } else {
        field("kind", &c.kind, &input.kind, true) + &field("roles", &c.roles, &input.roles, true)
    };
    let request = form::request_field(input.request_id.as_deref());
    let fields = field("id", &c.id, &input.id, input.version > 0)
        + &format!("<p class=\"note\">{}</p>", escape(&c.id_help))
        + &field("name", &c.name, &input.name, false)
        + &authority
        + &format!("<p class=\"note\">{}</p>", escape(&c.roles_help))
        + &field("wip", &c.wip, &input.wip, !ctx.owner)
        + &format!("<p class=\"note\">{}</p>", escape(&c.wip_help));
    let status = error
        .map(|e| {
            if e.code == "version_conflict" {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            }
        })
        .unwrap_or(StatusCode::OK);
    pages::app_page(
        status,
        &c.title,
        &format!(
            "<div class=\"stack\"><a href=\"/app/agents?account={}\">{}</a><h1>{}</h1>{notice}<p>{}</p><form class=\"card stack\" method=\"post\" action=\"/app/agents\"><input type=\"hidden\" name=\"account\" value=\"{}\"><input type=\"hidden\" name=\"version\" value=\"{}\">{request}{fields}{checkbox}<button type=\"submit\">{}</button></form>{key}{}</div>",
            encode(&ctx.account),
            escape(&c.back),
            escape(&c.title),
            escape(&c.owner_help),
            escape(&ctx.account),
            input.version,
            escape(&c.save),
            extra.unwrap_or_default()
        ),
        Tab::Me,
    )
}

pub(super) async fn work(
    state: &AppState,
    ctx: &Context,
    agent: &Agent,
    shown: &Shown,
) -> BridgeResult<String> {
    let site = pages::site();
    let c = &site.messages.app.agents;
    let mut names = BTreeMap::new();
    for name in state.hub.board_names(&ctx.actor)? {
        let (board, _, _) = state.hub.board_access(&ctx.actor, &name)?;
        names.insert(board.hosted().board_id().to_string(), name);
    }
    let after = match (&shown.board, &shown.ticket) {
        (Some(board), Some(ticket)) => Some(ClaimCursor {
            board_id: board.clone(),
            ticket_id: ticket.clone(),
        }),
        (None, None) => None,
        _ => return Err(BridgeError::new("invalid", "incomplete work page cursor")),
    };
    let tenant = ctx
        .actor
        .tenant
        .as_deref()
        .ok_or_else(|| BridgeError::new("not_granted", "environment unavailable"))?;
    let workload = registry::principal_workload(
        ctx.accounts.store(),
        tenant,
        &agent.principal(),
        &names.keys().cloned().collect::<Vec<_>>(),
        after.as_ref(),
        100,
    )
    .await?;
    let mut rows = String::new();
    for claim in workload.claims {
        let name = names
            .get(&claim.cursor.board_id)
            .ok_or_else(|| BridgeError::new("not_granted", "work outside visible boards"))?;
        rows.push_str(&format!(
            "<li><a href=\"/app/tickets/{}/{}\">{}</a><p>{}</p></li>",
            encode(name),
            encode(&claim.cursor.ticket_id),
            escape(&claim.title),
            escape(name)
        ));
    }
    let rows = if rows.is_empty() {
        format!("<p>{}</p>", escape(&c.work_empty))
    } else {
        format!("<ul class=\"rows\">{rows}</ul>")
    };
    let more = workload
        .next
        .map(|next| {
            format!(
                "<a href=\"/app/agents/{}?account={}&board={}&ticket={}\">{}</a>",
                encode(&agent.id),
                encode(&ctx.account),
                encode(&next.board_id),
                encode(&next.ticket_id),
                escape(&c.more)
            )
        })
        .unwrap_or_default();
    Ok(format!(
        "<section class=\"card\" id=\"agent-work\"><h2>{}</h2><p>{}: {}</p><p>{}</p>{rows}{more}</section>",
        escape(&c.work),
        escape(&c.total),
        workload.total,
        escape(&c.work_help)
    ))
}

pub(super) async fn bindings(ctx: &Context, agent: &Agent) -> BridgeResult<String> {
    let site = pages::site();
    let c = &site.messages.app.agents;
    let clients = ctx
        .accounts
        .agent_client_options(&ctx.actor.id, &ctx.account)
        .await?;
    let bindings = ctx
        .accounts
        .agent_oauth_bindings(&ctx.actor.id, &ctx.account)
        .await?;
    let mut forms = String::new();
    for (id, name) in clients {
        let bound = bindings.iter().find(|binding| binding.client == id);
        let current = bound
            .map(|binding| {
                format!(
                    "{}: {} ({})",
                    escape(&c.binding_current),
                    escape(&binding.agent_id),
                    escape(if binding.enabled {
                        &c.active
                    } else {
                        &c.disabled
                    })
                )
            })
            .unwrap_or_else(|| escape(&c.binding_unbound));
        let version = bound.map(|binding| binding.version).unwrap_or(0);
        let enabled = bound.is_none_or(|binding| binding.agent_id != agent.id || binding.enabled);
        let request = form::request_field(Some(&form::request_id()));
        forms.push_str(&format!("<form class=\"card stack\" method=\"post\" action=\"/app/agents/{}/oauth\">{request}<h3>{}: {}</h3><p class=\"what\">{}</p><p>{current}</p><input type=\"hidden\" name=\"account\" value=\"{}\"><input type=\"hidden\" name=\"client\" value=\"{}\"><input type=\"hidden\" name=\"version\" value=\"{}\"><label><input type=\"checkbox\" name=\"enabled\" value=\"true\"{}>{}</label><button type=\"submit\">{}</button></form>",encode(&agent.id),escape(&c.client),escape(&name),escape(&id),escape(&ctx.account),escape(&id),version,if enabled {" checked"}else{""},escape(&c.binding_enabled),escape(&c.bind)));
    }
    if forms.is_empty() {
        forms = format!("<p>{}</p>", escape(&c.no_clients));
    }
    Ok(format!(
        "<section class=\"stack\"><h2>{}</h2><p>{}</p>{forms}</section>",
        escape(&c.oauth),
        escape(&c.oauth_help)
    ))
}
