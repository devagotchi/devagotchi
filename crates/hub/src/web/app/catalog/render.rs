use super::*;

pub(super) fn record_url(kind: Kind, id: &str) -> String {
    format!("/app/catalog/{}/records/{}", kind.name(), encode(id))
}
fn label(row: &Row) -> &str {
    row.data
        .get("label")
        .or_else(|| row.data.get("title"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&row.id)
}
pub(super) fn index() -> Response {
    let site = pages::site();
    let c = &site.messages.app.catalog;
    let links = Kind::ALL
        .iter()
        .map(|kind| {
            format!(
                "<li><a href=\"/app/catalog/{}\">{}</a></li>",
                kind.name(),
                escape(&c.kinds[kind.name()])
            )
        })
        .collect::<String>();
    pages::app_page(
        StatusCode::OK,
        &c.title,
        &format!(
            "<h1>{}</h1><p>{}</p><ul class=\"rows\">{links}</ul>",
            escape(&c.title),
            escape(&c.description)
        ),
        Tab::Me,
    )
}
pub(super) fn list(kind: Kind, rows: &[Row]) -> Response {
    let site = pages::site();
    let c = &site.messages.app.catalog;
    let title = &c.kinds[kind.name()];
    let mut body = format!(
        "<p><a href=\"/app/catalog\">{}</a></p><h1>{}</h1><p><a href=\"/app/catalog/{}/new\">{}</a></p>",
        escape(&c.back),
        escape(title),
        kind.name(),
        escape(&c.new)
    );
    if rows.is_empty() {
        body.push_str(&format!("<p>{}</p>", escape(&c.empty)));
    }
    body.push_str("<ul class=\"rows\">");
    for row in rows.iter().take(100) {
        body.push_str(&format!(
            "<li><a href=\"{}\">{}</a> <small>{}</small>{}</li>",
            record_url(kind, &row.id),
            escape(label(row)),
            escape(&row.id),
            if row.is_default == Some(true) {
                format!(" <span>{}</span>", escape(&c.default))
            } else {
                String::new()
            }
        ));
    }
    body.push_str("</ul>");
    if rows.len() > 100 {
        body.push_str(&format!(
            "<a href=\"/app/catalog/{}?after={}\">{}</a>",
            kind.name(),
            encode(&rows[99].id),
            escape(&c.more)
        ));
    }
    pages::app_page(StatusCode::OK, title, &body, Tab::Me)
}
pub(super) fn editor(
    kind: Kind,
    draft: &Draft,
    new: bool,
    error: Option<&BridgeError>,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.catalog;
    let title = &c.kinds[kind.name()];
    let action = if new {
        format!("/app/catalog/{}/new", kind.name())
    } else {
        record_url(kind, &draft.id)
    };
    let mut body = format!(
        "<p><a href=\"/app/catalog/{}\">{}</a></p><h1>{}</h1>",
        kind.name(),
        escape(&c.back),
        escape(title)
    );
    if let Some(e) = error {
        body.push_str(&format!(
            "<p role=\"alert\">{}: {}</p>",
            escape(&c.refused),
            escape(&e.message)
        ));
    }
    body.push_str(&format!("<form method=\"post\" action=\"{action}\">{}<input type=\"hidden\" name=\"version\" value=\"{}\"><label>{}<input name=\"id\" value=\"{}\" {} required maxlength=\"64\"></label><p class=\"note\">{}</p>",requests::request_field(Some(&draft.request_id)),escape(&draft.version),escape(&c.id),escape(&draft.id),if new{""}else{"readonly"},escape(&c.id_help)));
    for f in kind.fields() {
        let value = draft
            .fields
            .get(f.name)
            .map(String::as_str)
            .unwrap_or_default();
        let label = escape(&c.fields[f.name]);
        let value_escaped = escape(value);
        match f.kind {
            FieldType::Bool => body.push_str(&format!(
                "<label><input type=\"checkbox\" name=\"{}\" value=\"true\" {}>{label}</label>",
                f.name,
                if value == "true" { "checked" } else { "" }
            )),
            FieldType::Json | FieldType::List | FieldType::LongText => {
                body.push_str(&format!("<label>{label}<textarea name=\"{}\" rows=\"6\">{value_escaped}</textarea></label>",f.name));
                if matches!(f.kind, FieldType::Json | FieldType::List) {
                    body.push_str(&format!(
                        "<p class=\"note\">{}</p>",
                        escape(&c.structured_help)
                    ));
                }
            }
            _ => body.push_str(&format!(
                "<label>{label}<input name=\"{}\" value=\"{value_escaped}\"></label>",
                f.name
            )),
        }
    }
    body.push_str(&format!(
        "<button name=\"action\" value=\"save\">{}</button>",
        escape(&c.save)
    ));
    if !new {
        if kind == Kind::GrantPresets {
            if draft.is_default {
                body.push_str(&format!("<p>{}</p>", escape(&c.default)));
            } else {
                body.push_str(&format!(
                    "<button name=\"action\" value=\"default\" formnovalidate>{}</button>",
                    escape(&c.choose_default)
                ));
            }
        }
        body.push_str(&format!("<p class=\"note\">{}</p><button name=\"action\" value=\"remove\" formnovalidate>{}</button>",escape(&c.remove_help),escape(&c.remove)));
    }
    body.push_str("</form>");
    pages::app_page(
        error.map(status).unwrap_or(StatusCode::OK),
        title,
        &body,
        Tab::Me,
    )
}
