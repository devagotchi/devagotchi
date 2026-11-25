use super::*;
use crate::{machines::access::GrantTerms, site::access_copy::AccessCopy};

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").into()
}
fn state(c: &AccessCopy, value: &str) -> String {
    escape(c.statuses.get(value).map(String::as_str).unwrap_or(value))
}
fn level(value: GrantLevel) -> String {
    serde_json::to_value(value)
        .expect("enum")
        .as_str()
        .unwrap()
        .into()
}
fn duration(c: &AccessCopy, value: GrantDuration) -> String {
    match value {
        GrantDuration::Once => c.once.clone(),
        GrantDuration::Task => c.task_duration.clone(),
        GrantDuration::UntilRevoked => c.until_revoked.clone(),
        GrantDuration::Hours(hours) => c.hours.replace("{hours}", &hours.to_string()),
    }
}
fn when(ms: i64) -> String {
    pages::utc(u64::try_from(ms.max(0) / 1000).unwrap_or(0))
}
fn nav(c: &AccessCopy) -> String {
    format!(
        "<nav aria-label=\"{}\"><a href=\"/app/access\">{}</a> · <a href=\"/app/access?history=true\">{}</a> · <a href=\"/app/access/grants\">{}</a></nav>",
        escape(&c.title),
        escape(&c.pending),
        escape(&c.history),
        escape(&c.grants)
    )
}
fn page(title: &str, body: String, status: StatusCode) -> Response {
    pages::app_page(status, title, &body, Tab::Approvals)
}
pub(super) fn requests(rows: &[Value], next: Option<&str>, history: bool) -> Response {
    let site = pages::site();
    let c = &site.messages.app.access;
    let mut body = format!(
        "{}<h1>{}</h1><p>{}</p>",
        nav(c),
        escape(if history { &c.history } else { &c.title }),
        escape(&c.description)
    );
    if rows.is_empty() {
        body.push_str(&format!("<p>{}</p>", escape(&c.empty)))
    }
    body.push_str("<ul class=\"rows\">");
    for row in rows {
        let label = row["prepared"]["capability"]["data"]["label"]
            .as_str()
            .unwrap_or("access");
        body.push_str(&format!("<li><a href=\"/app/access/requests/{}\">{} — {}</a><p>{}</p><p><small>{}</small></p></li>",
            encode(&text(row,"id")),escape(label),escape(&text(row,"machine")),escape(&text(row,"reason")),state(c,&text(row,"status"))));
    }
    body.push_str("</ul>");
    if let Some(next) = next {
        body.push_str(&format!(
            "<p><a href=\"/app/access?history={history}&amp;after={}\">{}</a></p>",
            encode(next),
            escape(&c.more)
        ))
    }
    page(&c.title, body, StatusCode::OK)
}
fn selector(c: &AccessCopy, item: &GrantItem) -> String {
    let value = serde_json::to_value(&item.selector).expect("typed selector");
    let fields = value.as_object().expect("selector object");
    if fields.is_empty() {
        return format!("<p>{}</p>", escape(&c.no_scope));
    }
    let mut out = String::from("<dl>");
    for (key, value) in fields {
        let label = c.selectors.get(key).map(String::as_str).unwrap_or(key);
        let shown = match value {
            Value::String(s) => s.clone(),
            Value::Array(items) if items.is_empty() => c.none.clone(),
            Value::Array(items) if items.iter().all(Value::is_string) => items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" · "),
            _ => value.to_string(),
        };
        out.push_str(&format!(
            "<dt>{}</dt><dd><code>{}</code></dd>",
            escape(label),
            escape(&shown)
        ));
    }
    out.push_str("</dl>");
    out
}
fn hidden(draft: &Draft) -> String {
    format!(
        "{}<input type=\"hidden\" name=\"version\" value=\"{}\">",
        requests::request_field(Some(&draft.request_id)),
        escape(&draft.version)
    )
}
fn option(value: &str, label: &str, selected: &str) -> String {
    format!(
        "<option value=\"{}\"{}>{}</option>",
        escape(value),
        if value == selected { " selected" } else { "" },
        escape(label)
    )
}
pub(super) fn review(
    review: &AccessReview,
    draft: &Draft,
    error: Option<&BridgeError>,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.access;
    let label = review
        .prepared
        .capability
        .data
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or(&review.prepared.capability.id);
    let mut body = format!(
        "{}<h1>{}</h1><dl><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd></dl><h2>{}</h2><blockquote>{}</blockquote><h2>{}</h2>{}",
        nav(c),
        escape(label),
        escape(&c.who),
        escape(&review.requester),
        escape(&c.machine),
        escape(&review.machine),
        escape(&c.status),
        state(c, &review.status),
        escape(&c.duration),
        escape(&duration(c, review.terms.duration)),
        escape(&c.expires),
        when(review.expires_at),
        escape(&c.reason),
        escape(&review.reason),
        escape(&c.scope),
        selector(c, &review.terms.item)
    );
    let requested_level = level(review.terms.item.sandbox_level);
    body.push_str(&format!(
        "<p>{}: <strong>{}</strong></p>",
        escape(&c.sandbox),
        escape(
            c.levels
                .get(&requested_level)
                .map(String::as_str)
                .unwrap_or(&requested_level)
        )
    ));
    if review.terms.item.sandbox_level == GrantLevel::Unsandboxed {
        body.push_str(&format!("<p role=\"note\">{}</p>", escape(&c.unsandboxed)))
    }
    if let Some(uses) = review.terms.uses_max {
        body.push_str(&format!("<p>{}: {uses}</p>", escape(&c.uses)))
    }
    if let (Some(project), Some(ticket)) = (&review.project, &review.ticket) {
        body.push_str(&format!(
            "<p>{}: <a href=\"/app/tickets/{}/{}\">{} / {}</a></p>",
            escape(&c.task),
            encode(project),
            encode(ticket),
            escape(project),
            escape(ticket)
        ))
    }
    body.push_str(&format!("<h2>{}</h2>", escape(&c.risks)));
    if review.prepared.risks.is_empty() {
        body.push_str(&format!("<p>{}</p>", escape(&c.no_risks)))
    }
    for risk in &review.prepared.risks {
        body.push_str(&format!(
            "<section><h3>{} — {}</h3><p>{}</p><p>{}: {}</p></section>",
            escape(&risk.severity),
            escape(&risk.title),
            escape(&risk.plain_text),
            escape(&c.mitigation),
            escape(&risk.mitigation)
        ))
    }
    if let Some(error) = error {
        body.push_str(&format!(
            "<div role=\"alert\"><p>{}</p><p>{}</p></div>",
            escape(&c.unchanged),
            escape(&message(error))
        ))
    }
    if review.status == "pending" || error.is_some() {
        let mut levels = String::new();
        for candidate in [
            GrantLevel::ReadOnly,
            GrantLevel::WorkspaceWrite,
            GrantLevel::Desktop,
            GrantLevel::Unsandboxed,
        ] {
            if candidate.within(review.terms.item.sandbox_level)
                && review.terms.item.capability.supports(candidate)
            {
                let key = level(candidate);
                levels.push_str(&option(
                    &key,
                    c.levels.get(&key).map(String::as_str).unwrap_or(&key),
                    &draft.level,
                ));
            }
        }
        if !levels.contains(&format!("value=\"{}\"", escape(&draft.level))) {
            levels.push_str(&option(&draft.level, &draft.level, &draft.level));
        }
        let mut durations = option("", &c.keep_duration, &draft.duration);
        let mut choices = vec![
            GrantDuration::Once,
            GrantDuration::Hours(1),
            GrantDuration::Hours(8),
            review.terms.duration,
        ];
        if review.project.is_some() {
            choices.push(GrantDuration::Task)
        }
        let mut seen = Vec::new();
        for candidate in choices {
            if candidate.within(review.terms.duration) && !seen.contains(&candidate) {
                seen.push(candidate);
                let key: String = candidate.into();
                durations.push_str(&option(&key, &duration(c, candidate), &draft.duration))
            }
        }
        if !durations.contains(&format!("value=\"{}\"", escape(&draft.duration))) {
            durations.push_str(&option(&draft.duration, &draft.duration, &draft.duration));
        }
        body.push_str(&format!("<form method=\"post\" action=\"/app/access/requests/{}\">{}<label>{}<textarea name=\"note\" maxlength=\"8192\">{}</textarea></label><div><button name=\"action\" value=\"approve\">{}</button><button name=\"action\" value=\"deny\">{}</button> <a href=\"/app/access\">{}</a></div><details{}><summary>{}</summary><label>{}<select name=\"level\">{levels}</select></label><label>{}<select name=\"duration\">{durations}</select></label><label>{}<input name=\"uses_max\" inputmode=\"numeric\" value=\"{}\"></label><p>{}</p><label><select name=\"paths_mode\">{}{}</select></label><label>{}<textarea name=\"paths\">{}</textarea></label><p>{}</p><button name=\"action\" value=\"narrow\">{}</button></details></form>",
            encode(&review.id),hidden(draft),escape(&c.note),escape(&draft.note),escape(&c.approve),escape(&c.deny),escape(&c.later),if error.is_some(){" open"}else{""},escape(&c.narrow),escape(&c.sandbox),escape(&c.duration),escape(&c.uses),escape(&draft.uses_max),escape(&c.uses_help),
            option("keep",&c.keep_paths,&draft.paths_mode),option("restrict",&c.restrict_paths,&draft.paths_mode),escape(&c.paths),escape(&draft.paths),escape(&c.paths_help),escape(&c.narrow)));
    } else {
        body.push_str(&format!("<p>{}</p>", escape(&c.stale)))
    }
    if let Some(granted) = &review.granted {
        body.push_str(&format!(
            "<h2>{}</h2>{}{}",
            escape(&c.granted),
            selector(c, &granted.item),
            terms(c, granted)
        ));
    }
    if let Some(note) = &review.decision_note {
        body.push_str(&format!("<p>{}: {}</p>", escape(&c.note), escape(note)))
    }
    page(&c.review, body, error.map_or(StatusCode::OK, status))
}
pub(super) fn grants(
    rows: &[Value],
    next: Option<&str>,
    ended: bool,
    error: Option<(&str, &Draft, &BridgeError)>,
) -> Response {
    let site = pages::site();
    let c = &site.messages.app.access;
    let mut body = format!(
        "{}<h1>{}</h1><p><a href=\"/app/access/grants\">{}</a> · <a href=\"/app/access/grants?ended=true\">{}</a></p>",
        nav(c),
        escape(&c.grants),
        escape(&c.live),
        escape(&c.ended)
    );
    if let Some((id, draft, error)) = error {
        body.push_str(&format!("<p role=\"alert\">{}</p><form method=\"post\" action=\"/app/access/grants/{}/revoke\">{}<button name=\"action\" value=\"revoke\">{}</button></form>",escape(&message(error)),encode(id),hidden(draft),escape(&c.revoke)))
    }
    let shown = rows;
    if shown.is_empty() {
        body.push_str(&format!("<p>{}</p>", escape(&c.empty)))
    }
    for row in shown {
        let id = text(row, "id");
        let live = row["live"] == true;
        let item = match serde_json::from_value::<GrantItem>(row["item"].clone()) {
            Ok(item) => item,
            Err(_) => return refused(&BridgeError::new("access_damaged", "invalid saved grant")),
        };
        body.push_str(&format!(
            "<section class=\"card\"><h2>{} — {}</h2><p>{}: {}</p>{}<p>{}: {}</p>",
            escape(
                c.capabilities
                    .get(
                        serde_json::to_value(item.capability)
                            .expect("enum")
                            .as_str()
                            .unwrap()
                    )
                    .map(String::as_str)
                    .unwrap_or(&c.scope)
            ),
            escape(&text(row, "machine")),
            escape(&c.who),
            escape(&text(row, "requester")),
            selector(c, &item),
            escape(&c.uses),
            escape(&row["uses"].to_string())
        ));
        let grant_duration = match serde_json::from_value::<GrantDuration>(row["duration"].clone())
        {
            Ok(value) => value,
            Err(_) => {
                return refused(&BridgeError::new(
                    "access_damaged",
                    "invalid saved duration",
                ));
            }
        };
        body.push_str(&terms(
            c,
            &GrantTerms {
                item: item.clone(),
                duration: grant_duration,
                uses_max: row["uses_max"].as_u64().and_then(|n| u32::try_from(n).ok()),
            },
        ));
        if let Some(expires) = row["expires_at"].as_i64() {
            body.push_str(&format!("<p>{}: {}</p>", escape(&c.expires), when(expires)))
        }
        if row["paused"] == true {
            body.push_str(&format!("<p>{}</p>", escape(&c.paused)))
        }
        if !live {
            body.push_str(&format!(
                "<p>{}: {}</p>",
                escape(&c.ended),
                state(c, &text(row, "end_reason"))
            ))
        }
        if let Some(risks) = row["risks"].as_array() {
            for risk in risks {
                body.push_str(&format!(
                    "<p>{}: {}: {}. {}</p>",
                    escape(&c.risks),
                    escape(&text(risk, "severity")),
                    escape(&text(risk, "title")),
                    escape(&text(risk, "plain_text"))
                ))
            }
        }
        if live && !error.is_some_and(|(target, _, _)| target == id) {
            let fresh = Draft {
                request_id: requests::request_id(),
                version: row["version"].to_string(),
                action: "revoke".into(),
                duration: String::new(),
                uses_max: String::new(),
                level: String::new(),
                paths: String::new(),
                paths_mode: String::new(),
                note: String::new(),
            };
            let draft = error
                .filter(|(target, _, _)| *target == id)
                .map(|(_, d, _)| d)
                .unwrap_or(&fresh);
            body.push_str(&format!("<form method=\"post\" action=\"/app/access/grants/{}/revoke\">{}<button name=\"action\" value=\"revoke\">{}</button></form>",encode(&id),hidden(draft),escape(&c.revoke)));
        }
        body.push_str("</section>");
    }
    if let Some(next) = next {
        body.push_str(&format!(
            "<a href=\"/app/access/grants?ended={ended}&amp;after={}\">{}</a>",
            encode(next),
            escape(&c.more)
        ))
    }
    page(
        &c.grants,
        body,
        error.map_or(StatusCode::OK, |(_, _, e)| status(e)),
    )
}

fn terms(c: &AccessCopy, terms: &GrantTerms) -> String {
    let key = level(terms.item.sandbox_level);
    let mut out = format!(
        "<dl><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd></dl>",
        escape(&c.duration),
        escape(&duration(c, terms.duration)),
        escape(&c.sandbox),
        escape(c.levels.get(&key).map(String::as_str).unwrap_or(&key))
    );
    if let Some(limit) = terms.uses_max {
        out.push_str(&format!("<p>{}: {limit}</p>", escape(&c.use_limit)))
    }
    if terms.item.sandbox_level == GrantLevel::Unsandboxed {
        out.push_str(&format!("<p role=\"note\">{}</p>", escape(&c.unsandboxed)))
    }
    out
}
