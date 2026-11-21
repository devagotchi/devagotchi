use super::*;

fn screen(title: &str, body: String, error: Option<&BridgeError>) -> Response {
    let notice = error
        .map(|e| format!("<p role=\"alert\">{}</p>", escape(&e.message)))
        .unwrap_or_default();
    let c = &pages::site().messages.app.projects;
    pages::app_page(
        error.map(status).unwrap_or(StatusCode::OK),
        title,
        &format!(
            "<h1>{}</h1>{notice}{body}<p><a href=\"/app/projects\">{}</a></p>",
            escape(title),
            escape(&c.back)
        ),
        Tab::Boards,
    )
}
fn hidden(key: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{key}\" value=\"{}\">",
        escape(value)
    )
}
pub(super) fn list(projects: &[ProjectRecord], retired: bool) -> Response {
    let c = &pages::site().messages.app.projects;
    let mut body = format!(
        "<p>{}</p><nav><a href=\"/app/projects\">{}</a> · <a href=\"/app/projects?retired=true\">{}</a> · <a href=\"/app/projects/new\">{}</a></nav>",
        escape(&c.description),
        escape(&c.active),
        escape(&c.retired),
        escape(&c.new)
    );
    if projects.is_empty() {
        body.push_str(&format!("<p>{}</p>", escape(&c.empty)));
    }
    body.push_str("<ul>");
    for project in projects.iter().take(100) {
        let path = if retired {
            format!("archive/{}", encode(&project.board_id))
        } else {
            encode(&project.id)
        };
        body.push_str(&format!(
            "<li><a href=\"/app/projects/{path}\">{}</a></li>",
            escape(&project.name)
        ));
    }
    body.push_str("</ul>");
    if projects.len() > 100 {
        let p = &projects[99];
        body.push_str(&format!(
            "<p><a href=\"/app/projects?retired={retired}&amp;after={}\">{}</a></p>",
            encode(&format!("{}/{}", p.id, p.board_id)),
            escape(&c.more)
        ));
    }
    screen(&c.title, body, None)
}
pub(super) fn editor(
    draft: &Draft,
    workflows: &[SharedWorkflow],
    error: Option<&BridgeError>,
) -> Response {
    let c = &pages::site().messages.app.projects;
    let path = if draft.version == 0 {
        "/app/projects/new".into()
    } else {
        format!("/app/projects/{}", encode(&draft.id))
    };
    let mut choices = format!("<option value=\"\">{}</option>", escape(&c.local_workflow));
    let mut found = draft.workflow.is_empty();
    for w in workflows {
        let value = format!("{}:{}", w.workflow.id, w.version);
        let selected = if value == draft.workflow {
            found = true;
            " selected"
        } else {
            ""
        };
        choices.push_str(&format!(
            "<option value=\"{}\"{selected}>{}</option>",
            escape(&value),
            escape(&w.workflow.name)
        ));
    }
    // Keep the submitted choice visible on a stale definition instead of silently selecting another.
    if !found {
        choices.push_str(&format!(
            "<option value=\"{}\" selected>{}</option>",
            escape(&draft.workflow),
            escape(&draft.workflow)
        ));
    }
    let mut body = format!(
        "<form method=\"post\" action=\"{path}\" class=\"card stack\">{}{}<label>{}<input name=\"id\" value=\"{}\" required maxlength=\"63\" pattern=\"[a-z0-9][a-z0-9-]{{0,62}}\" {} aria-describedby=\"project-address-help\"></label><p id=\"project-address-help\">{}</p><label>{}<input name=\"name\" value=\"{}\" required maxlength=\"100\"></label><label>{}<select name=\"workflow\">{choices}</select></label><button type=\"submit\">{}</button></form>",
        requests::request_field(Some(&draft.request_id)),
        hidden("version", &draft.version.to_string()),
        escape(&c.id),
        escape(&draft.id),
        if draft.version == 0 { "" } else { "readonly" },
        escape(&c.id_help),
        escape(&c.name),
        escape(&draft.name),
        escape(&c.workflow),
        escape(&c.save)
    );
    if draft.version > 0 {
        body.push_str(&format!("<p><a href=\"/app/boards/{}\">{}</a> · <a href=\"/app/boards/{}/workflows\">{}</a></p><form method=\"post\" action=\"{path}/remove\" class=\"card stack\">{}{}<p>{}</p><button type=\"submit\">{}</button></form>",encode(&draft.id),escape(&c.open),encode(&draft.id),escape(&c.workflow),requests::request_field(Some(&requests::request_id())),hidden("version",&draft.version.to_string()),escape(&c.remove_help),escape(&c.remove)));
    }
    screen(
        if draft.version == 0 {
            &c.new
        } else {
            &draft.name
        },
        body,
        error,
    )
}
pub(super) fn history(record: &ProjectRecord, events: &[crate::agenthub::Event]) -> Response {
    let c = &pages::site().messages.app.projects;
    let mut body = format!(
        "<p>{}</p><h2>{}</h2><ol>",
        escape(&c.retired_notice),
        escape(&record.name)
    );
    for event in events {
        body.push_str(&format!(
            "<li><strong>{}</strong> {}<pre>{}</pre></li>",
            escape(&event.action),
            escape(&event.actor),
            escape(&event.data.to_string())
        ));
    }
    body.push_str("</ol>");
    if events.len() == 100 {
        body.push_str(&format!(
            "<a href=\"/app/projects/archive/{}?after_seq={}\">{}</a>",
            encode(&record.board_id),
            events.last().unwrap().seq,
            escape(&c.more)
        ));
    }
    screen(&c.history, body, None)
}
