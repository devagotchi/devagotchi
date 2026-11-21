//! Escaped, labelled forms. Workflow row controls work without JavaScript.
use super::form::{SettingsDraft, WorkflowDraft};
use crate::{
    agenthub::{StateKind, Workflow},
    site::configuration_copy::ConfigurationCopy,
    web::{
        encode,
        pages::{self, Tab, escape},
    },
};
use axum::{http::StatusCode, response::Response};

pub(super) fn href(project: &str, workflow: bool) -> String {
    format!(
        "/app/boards/{}/{}",
        encode(project),
        if workflow { "workflow" } else { "settings" }
    )
}
pub(super) fn hidden(name: &str, value: impl ToString) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        escape(name),
        escape(&value.to_string())
    )
}
pub(super) fn field(name: &str, label: &str, value: &str, attributes: &str) -> String {
    format!(
        "<label class=\"stack\">{}<input id=\"{}\" name=\"{}\" value=\"{}\" {attributes}></label>",
        escape(label),
        escape(name),
        escape(name),
        escape(value)
    )
}
pub(super) fn select(
    name: &str,
    label: &str,
    value: &str,
    options: &[(String, String)],
    empty: &str,
) -> String {
    let mut items = String::new();
    if !options.iter().any(|(k, _)| k == value) {
        items.push_str(&format!(
            "<option value=\"{}\" selected>{}</option>",
            escape(value),
            escape(if value.is_empty() { empty } else { value })
        ));
    }
    for (key, label) in options {
        items.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            escape(key),
            if key == value { " selected" } else { "" },
            escape(label)
        ));
    }
    format!(
        "<div class=\"stack\"><label for=\"{}\">{}</label><select id=\"{}\" name=\"{}\" required>{items}</select></div>",
        escape(name),
        escape(label),
        escape(name),
        escape(name)
    )
}
fn row_button(action: &str, label: &str) -> String {
    format!(
        "<button type=\"submit\" name=\"action\" value=\"{}\" formnovalidate>{}</button>",
        escape(action),
        escape(label)
    )
}
pub(super) fn screen(
    project: &str,
    workflow: bool,
    status: StatusCode,
    notice: &str,
    form: String,
) -> Response {
    screen_at(
        project,
        workflow,
        status,
        notice,
        form,
        &href(project, workflow),
    )
}
pub(super) fn screen_at(
    project: &str,
    workflow: bool,
    status: StatusCode,
    notice: &str,
    form: String,
    reload: &str,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let title = if workflow { &c.workflow } else { &c.settings };
    let message = if notice.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"notice\" role=\"{}\"><p>{}</p>{}</div>",
            if status.is_client_error() || status.is_server_error() {
                "alert"
            } else {
                "status"
            },
            escape(notice),
            if status == StatusCode::CONFLICT {
                format!("<a href=\"{}\">{}</a>", escape(reload), escape(&c.reload))
            } else {
                String::new()
            }
        )
    };
    let body = format!(
        "<div class=\"stack\" data-page=\"board-configuration\"><header class=\"stack\"><a href=\"/app/boards/{}\">{}</a><h1>{}</h1><p>{}</p><nav class=\"cluster\" aria-label=\"{}\"><a href=\"{}\"{}>{}</a><a href=\"{}\"{}>{}</a></nav></header>{message}{form}</div>",
        encode(project),
        escape(&c.back),
        escape(&format!("{} · {project}", c.title)),
        escape(&c.description),
        escape(&c.title),
        href(project, false),
        if !workflow {
            " aria-current=\"page\""
        } else {
            ""
        },
        escape(&c.settings),
        href(project, true),
        if workflow {
            " aria-current=\"page\""
        } else {
            ""
        },
        escape(&c.workflow)
    );
    pages::app_page(status, title, &body, Tab::Boards)
}

pub(super) fn settings(
    project: &str,
    draft: &SettingsDraft,
    status: StatusCode,
    notice: &str,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let input = |key: &str, label: &str, attributes: &str| {
        field(
            key,
            label,
            draft
                .values
                .get(key)
                .map(String::as_str)
                .unwrap_or_default(),
            attributes,
        )
    };
    let duration = "type=\"number\" min=\"0.001\" step=\"0.001\" required";
    let nonnegative = "type=\"number\" min=\"0\" step=\"1\" required";
    let body = format!(
        "<form class=\"stack\" method=\"post\" action=\"{}\">{}<section class=\"card stack\"><h2>{}</h2><p>{}</p><div class=\"auto-grid\">{}{}{}</div></section><section class=\"card stack\"><div class=\"auto-grid\"><div class=\"stack\">{}<p>{}</p></div><div class=\"stack\">{}<p>{}</p></div><div class=\"stack\">{}<p>{}</p></div></div>{}{}<p>{}</p></section><button class=\"primary\" type=\"submit\">{}</button></form>",
        href(project, false),
        hidden("version", draft.version),
        escape(&c.settings),
        escape(&c.lease_help),
        input("lease_ttl", &c.default_lease, duration),
        input("lease_min_ttl", &c.minimum_lease, duration),
        input("lease_max_ttl", &c.maximum_lease, duration),
        input("max_lease_expiries", &c.expiry_limit, nonnegative),
        escape(&c.expiry_help),
        input("wip_limit_default", &c.wip_limit, nonnegative),
        escape(&c.wip_help),
        input(
            "default_priority",
            &c.priority,
            "type=\"number\" step=\"1\" required"
        ),
        escape(&c.priority_help),
        input("repo", &c.repo, "type=\"url\" maxlength=\"500\""),
        input("environments", &c.environments, "type=\"text\" required"),
        escape(&c.environment_help),
        escape(&c.save_settings)
    );
    screen(project, false, status, notice, body)
}

fn state_options(w: &Workflow, star: bool, c: &ConfigurationCopy) -> Vec<(String, String)> {
    let mut options = Vec::new();
    if star {
        options.push(("*".into(), c.any_state.clone()));
    }
    options.extend(
        w.states
            .iter()
            .map(|s| (s.key.clone(), format!("{} ({})", s.label, s.key))),
    );
    options
}
pub(super) fn workflow(
    project: &str,
    draft: &WorkflowDraft,
    status: StatusCode,
    notice: &str,
    catalog: bool,
) -> Response {
    let site = pages::site();
    let mut body = String::new();
    if catalog {
        body.push_str(&super::shared::render::catalog_link(project));
    }
    body.push_str(&workflow_form(
        draft,
        &href(project, true),
        &site.messages.app.configuration.workflow_help,
    ));
    screen(project, true, status, notice, body)
}

pub(super) fn workflow_form(draft: &WorkflowDraft, action: &str, help: &str) -> String {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let w = &draft.workflow;
    let options = state_options(w, false, c);
    let kinds = [
        ("queue", &c.queue),
        ("active", &c.active),
        ("review", &c.review),
        ("blocked", &c.blocked),
        ("done", &c.done),
        ("attention", &c.attention),
    ]
    .map(|(k, v)| (k.into(), v.clone()));
    let mut body = format!(
        "<form class=\"stack\" method=\"post\" action=\"{}\">{}{}{}{}{}<section class=\"card stack\"><h2>{}</h2><p>{}</p>{}<div class=\"auto-grid\">{}{}</div><button class=\"primary\" type=\"submit\" name=\"action\" value=\"save\">{}</button></section>",
        escape(action),
        hidden("version", draft.version),
        hidden("id", &w.id),
        hidden("states", w.states.len()),
        hidden("lanes", w.lanes.len()),
        hidden("transitions", w.transitions.len()),
        escape(&c.workflow),
        escape(help),
        field(
            "name",
            &c.workflow_name,
            &w.name,
            "type=\"text\" maxlength=\"160\" required"
        ),
        select(
            "initial_state",
            &c.initial_state,
            &w.initial_state,
            &options,
            &c.choose_state
        ),
        select(
            "attention_state",
            &c.attention_state,
            &w.attention_state,
            &options,
            &c.choose_state
        ),
        escape(&c.save_workflow)
    );
    body.push_str(&format!(
        "<section class=\"card stack\"><h2>{}</h2><p>{}</p>",
        escape(&c.states),
        escape(&c.states_help)
    ));
    for (i, state) in w.states.iter().enumerate() {
        let kind = match state.kind {
            StateKind::Queue => "queue",
            StateKind::Active => "active",
            StateKind::Review => "review",
            StateKind::Blocked => "blocked",
            StateKind::Done => "done",
            StateKind::Attention => "attention",
        };
        body.push_str(&format!(
            "<fieldset class=\"stack\"><legend>{} {}</legend><div class=\"auto-grid\">{}{}{}</div>{}</fieldset>",
            escape(&c.state), i+1,
            field(&format!("state_{i}_key"), &c.state_key, &state.key, "type=\"text\" maxlength=\"64\" required"),
            field(&format!("state_{i}_label"), &c.state_label, &state.label, "type=\"text\" maxlength=\"160\" required"),
            select(&format!("state_{i}_kind"), &c.state_kind, kind, &kinds, &c.choose_state),
            row_button(&format!("remove-state-{i}"), &c.remove)
        ));
    }
    body.push_str(&row_button("add-state", &c.add_state));
    body.push_str(&format!(
        "</section><section class=\"card stack\"><h2>{}</h2><p>{}</p>",
        escape(&c.stages),
        escape(&c.stages_help)
    ));
    for (i, lane) in w.lanes.iter().enumerate() {
        body.push_str(&format!(
            "<fieldset class=\"stack\"><legend>{} {}</legend><div class=\"auto-grid\">",
            escape(&c.stage),
            i + 1
        ));
        for (name, label, value) in [
            ("queue", &c.stage_queue, &lane.queue),
            ("active", &c.stage_active, &lane.active),
            ("complete_to", &c.complete_to, &lane.complete_to),
            ("release_to", &c.release_to, &lane.release_to),
        ] {
            body.push_str(&select(
                &format!("lane_{i}_{name}"),
                label,
                value,
                &options,
                &c.choose_state,
            ));
        }
        body.push_str(&format!("</div><label class=\"cluster\"><input type=\"checkbox\" name=\"lane_{i}_distinct\" value=\"true\"{}>{}</label>{}</fieldset>",if lane.distinct_from_previous {" checked"} else {""},escape(&c.distinct),row_button(&format!("remove-lane-{i}"),&c.remove)));
    }
    body.push_str(&row_button("add-lane", &c.add_stage));
    body.push_str(&format!(
        "</section><section class=\"card stack\"><h2>{}</h2><p>{}</p>",
        escape(&c.transitions),
        escape(&c.transitions_help)
    ));
    let from = state_options(w, true, c);
    for (i, t) in w.transitions.iter().enumerate() {
        body.push_str(&format!("<fieldset class=\"stack\"><legend>{} {}</legend><div class=\"auto-grid\">{}{}</div>{}</fieldset>",escape(&c.transition),i+1,select(&format!("transition_{i}_from"),&c.from,&t.from,&from,&c.choose_state),select(&format!("transition_{i}_to"),&c.to,&t.to,&options,&c.choose_state),row_button(&format!("remove-transition-{i}"),&c.remove)));
    }
    body.push_str(&row_button("add-transition", &c.add_transition));
    body.push_str(&format!("</section><button class=\"primary\" type=\"submit\" name=\"action\" value=\"save\">{}</button></form>",escape(&c.save_workflow)));
    body
}
