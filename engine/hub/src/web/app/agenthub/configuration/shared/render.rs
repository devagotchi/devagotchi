//! Escaped catalog and binding controls, with one shared/private workflow editor.
use super::super::render::{field, hidden, screen, screen_at, select, workflow_form};
use super::{WorkflowDraft, form::Change};
use crate::{
    agenthub::{SharedWorkflow, WorkflowConfiguration},
    web::{
        encode,
        pages::{self, escape},
    },
};
use axum::{http::StatusCode, response::Response};

pub(super) fn href(project: &str) -> String {
    format!("/app/boards/{}/workflows", encode(project))
}
pub(super) fn edit_href(project: &str, id: &str) -> String {
    format!("{}/{}", href(project), encode(id))
}
pub(in crate::web::app::agenthub::configuration) fn catalog_link(project: &str) -> String {
    format!(
        "<p><a href=\"{}\">{}</a></p>",
        href(project),
        escape(&pages::site().messages.app.configuration.shared_workflows)
    )
}
fn button(text: &str) -> String {
    format!("<button type=\"submit\">{}</button>", escape(text))
}
fn form(project: &str, action: &str, body: String) -> String {
    form_with_key(project, action, body, None)
}
fn form_with_key(project: &str, action: &str, body: String, key: Option<&str>) -> String {
    format!(
        "<form class=\"stack\" method=\"post\" action=\"{}\">{}{body}</form>",
        href(project),
        hidden("action", action)
            + &crate::web::app::requests::request_field(Some(
                key.unwrap_or(&crate::web::app::requests::request_id())
            ))
    )
}
fn used_by(names: Option<&[String]>) -> String {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let value = match names {
        Some([]) => c.shared_unused.clone(),
        Some(names) => names.join(", "),
        None => c.shared_unknown.clone(),
    };
    format!(
        "<p><strong>{}</strong> {}</p>",
        escape(&c.shared_used_by),
        escape(&value)
    )
}
pub(in crate::web::app::agenthub::configuration) fn bound(
    project: &str,
    current: &WorkflowConfiguration,
    catalog: bool,
    notice: &str,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let binding = current.shared_workflow.as_ref().expect("bound board");
    let mut body = format!(
        "<section class=\"card stack\"><h2>{}</h2><p>{}: {}</p><p>{}</p>",
        escape(&c.shared_bound),
        escape(&c.workflow_name),
        escape(&current.configuration.workflow.name),
        escape(&c.shared_bound_help)
    );
    if catalog {
        body.push_str(&format!(
            "<p><a href=\"{}\">{}</a></p>{}",
            edit_href(project, &binding.id),
            escape(&c.shared_edit),
            catalog_link(project)
        ));
        body.push_str(&form(
            project,
            "detach",
            format!(
                "{}<p>{}</p>{}",
                hidden("board_version", current.configuration.workflow_version),
                escape(&c.shared_detach_help),
                button(&c.shared_detach)
            ),
        ));
    }
    body.push_str("</section>");
    screen(project, true, StatusCode::OK, notice, body)
}
pub(super) fn catalog(
    project: &str,
    current: &WorkflowConfiguration,
    items: &[SharedWorkflow],
    draft: Option<&Change>,
    status: StatusCode,
    notice: &str,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let version = current.configuration.workflow_version;
    let form = |project: &str, action: &str, body: String| {
        form_with_key(
            project,
            action,
            body,
            draft
                .filter(|d| d.action() == action)
                .and_then(Change::request_id),
        )
    };
    let mut body = format!(
        "<section class=\"card stack\"><h2>{}</h2><p>{}</p></section>",
        escape(&c.shared_workflows),
        escape(&c.shared_help)
    );
    if let Some(binding) = &current.shared_workflow {
        let board = match draft {
            Some(Change::Detach { board, .. }) => *board,
            _ => version,
        };
        body.push_str(&format!(
            "<section class=\"card stack\"><h2>{}</h2><p>{}</p><a href=\"{}\">{}</a>{}</section>",
            escape(&c.shared_bound),
            escape(&current.configuration.workflow.name),
            edit_href(project, &binding.id),
            escape(&c.shared_edit),
            form(
                project,
                "detach",
                format!(
                    "{}<p>{}</p>{}",
                    hidden("board_version", board),
                    escape(&c.shared_detach_help),
                    button(&c.shared_detach)
                )
            )
        ));
    }
    if current.shared_workflow.is_none() || matches!(draft, Some(Change::Publish { .. })) {
        let (board, id, name) = match draft {
            Some(Change::Publish {
                board, id, name, ..
            }) => (*board, id.as_str(), name.as_str()),
            _ => (version, "", current.configuration.workflow.name.as_str()),
        };
        let inputs = format!(
            "{}<h2>{}</h2><p>{}</p>{}{}{}",
            hidden("board_version", board),
            escape(&c.shared_publish),
            escape(&c.shared_publish_help),
            field(
                "id",
                &c.shared_id,
                id,
                "type=\"text\" maxlength=\"64\" required"
            ),
            field(
                "name",
                &c.workflow_name,
                name,
                "type=\"text\" maxlength=\"160\" required"
            ),
            button(&c.shared_publish)
        );
        body.push_str(&format!(
            "<section class=\"card stack\">{}</section>",
            form(project, "publish", inputs)
        ));
    }
    if !items.is_empty() || matches!(draft, Some(Change::Bind { .. })) {
        let selected = current
            .shared_workflow
            .as_ref()
            .map(|b| format!("{}:{}", b.id, b.version))
            .unwrap_or_default();
        let (board, selected) = match draft {
            Some(Change::Bind {
                board, selection, ..
            }) => (*board, selection.as_str()),
            _ => (version, selected.as_str()),
        };
        let options = items
            .iter()
            .map(|w| {
                (
                    format!("{}:{}", w.workflow.id, w.version),
                    format!("{} ({})", w.workflow.name, w.workflow.id),
                )
            })
            .collect::<Vec<_>>();
        let inputs = format!(
            "{}<h2>{}</h2><p>{}</p>{}{}",
            hidden("board_version", board),
            escape(&c.shared_use),
            escape(&c.shared_use_help),
            select(
                "selection",
                &c.shared_choose,
                selected,
                &options,
                &c.shared_choose
            ),
            button(&c.shared_use)
        );
        body.push_str(&format!(
            "<section class=\"card stack\">{}</section>",
            form(project, "bind", inputs)
        ));
    }
    if items.is_empty() {
        body.push_str(&format!("<p>{}</p>", escape(&c.shared_empty)));
    }
    for item in items {
        body.push_str(&format!(
            "<section class=\"card stack\"><h2><a href=\"{}\">{}</a></h2><p>{}: {}</p>{}",
            edit_href(project, &item.workflow.id),
            escape(&item.workflow.name),
            escape(&c.shared_id),
            escape(&item.workflow.id),
            used_by(Some(&item.used_by))
        ));
        if item.used_by.is_empty() {
            let version = match draft {
                Some(Change::Delete { id, version, .. }) if id == &item.workflow.id => *version,
                _ => item.version,
            };
            body.push_str(&form_with_key(
                project,
                "delete",
                format!(
                    "{}{}<p>{}</p>{}",
                    hidden("id", &item.workflow.id),
                    hidden("version", version),
                    escape(&c.shared_delete_help),
                    button(&c.shared_delete)
                ),
                draft
                    .filter(|d| matches!(d, Change::Delete {id, ..} if id == &item.workflow.id))
                    .and_then(Change::request_id),
            ));
        }
        body.push_str("</section>");
    }
    screen_at(project, true, status, notice, body, &href(project))
}
pub(super) fn editor(
    project: &str,
    id: &str,
    draft: &WorkflowDraft,
    uses: Option<&[String]>,
    status: StatusCode,
    notice: &str,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let target = edit_href(project, id);
    let body = format!(
        "{}<section class=\"card stack\"><h2>{}</h2><p>{}</p>{}</section>{}",
        catalog_link(project),
        escape(&c.shared_edit),
        escape(&c.shared_edit_help),
        used_by(uses),
        workflow_form(draft, &target, &c.shared_edit_help)
    );
    screen_at(project, true, status, notice, body, &target)
}

/// Keep submitted values without pretending an unreadable catalog is empty or current.
pub(super) fn unavailable_change(project: &str, draft: &Change) -> Response {
    let site = pages::site();
    let c = &site.messages.app.configuration;
    let (action, inputs, label) = match draft {
        Change::Publish {
            board, id, name, ..
        } => (
            "publish",
            format!(
                "{}{}{}",
                hidden("board_version", board),
                field("id", &c.shared_id, id, "type=\"text\" required"),
                field("name", &c.workflow_name, name, "type=\"text\" required")
            ),
            &c.shared_publish,
        ),
        Change::Bind {
            board, selection, ..
        } => (
            "bind",
            format!(
                "{}{}",
                hidden("board_version", board),
                field(
                    "selection",
                    &c.shared_choose,
                    selection,
                    "type=\"text\" readonly"
                )
            ),
            &c.shared_use,
        ),
        Change::Detach { board, .. } => {
            ("detach", hidden("board_version", board), &c.shared_detach)
        }
        Change::Delete { id, version, .. } => (
            "delete",
            format!(
                "{}{}",
                hidden("version", version),
                field("id", &c.shared_id, id, "type=\"text\" readonly")
            ),
            &c.shared_delete,
        ),
    };
    let body = format!(
        "<section class=\"card stack\"><h2>{}</h2><p>{}</p>{}</section>",
        escape(label),
        escape(&c.shared_unknown),
        form_with_key(
            project,
            action,
            format!("{inputs}{}", button(label)),
            draft.request_id()
        )
    );
    screen_at(
        project,
        true,
        StatusCode::SERVICE_UNAVAILABLE,
        &c.refused,
        body,
        &href(project),
    )
}
