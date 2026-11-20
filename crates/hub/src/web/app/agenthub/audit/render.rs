use super::*;
use crate::{
    agenthub::{ChainStatus, Event},
    site::{Ctx, Site},
};

pub(super) fn body(
    project: &str,
    shown: &Shown,
    head: i64,
    events: &[Event],
    chain: Option<&ChainStatus>,
    site: &Site,
    me: &str,
) -> String {
    let copy = &site.messages.app.audit;
    let path = format!("/app/boards/{}/audit", encode(project));
    let fields: String = [
        ("actor", &copy.actor, &shown.actor),
        ("action", &copy.action, &shown.action),
        ("ticket", &copy.ticket, &shown.ticket),
    ]
    .into_iter()
    .map(|(name, label, value)| {
        format!(
            "<label>{}<input type=\"text\" name=\"{name}\" value=\"{}\"></label>",
            escape(label),
            escape(value.as_deref().unwrap_or_default())
        )
    })
    .collect();
    let status = match chain {
        Some(chain) if chain.ok => format!(
            "<div role=\"status\" id=\"audit-verified\"><p>{}</p><pre class=\"what\">{}</pre></div>",
            Ctx::new(site)
                .with("count", chain.entries.to_string())
                .inline(&copy.verified),
            escape(&chain.head)
        ),
        Some(chain) => format!(
            "<p role=\"alert\" id=\"audit-broken\">{}</p>",
            Ctx::new(site)
                .with("seq", chain.first_broken.unwrap_or(0).to_string())
                .inline(&copy.broken)
        ),
        None => format!("<p class=\"note\">{}</p>", escape(&copy.unverified)),
    };
    let rows: String = events.iter().take(100).map(|event| {
        let actor = if event.actor == me { &site.messages.app.you } else { &event.actor };
        let actor = format!("<span title=\"{}\">{}</span>", escape(&event.actor), escape(actor));
        let ticket = event.ticket.as_deref().map(|id| format!(" <a href=\"/app/tickets/{}/{}\">{}</a>", encode(project), encode(id), escape(id))).unwrap_or_default();
        format!("<li id=\"event-{}\"><div class=\"meta\">#{} · {} · {}</div><p>{}{ticket}</p><details><summary>{}</summary><pre class=\"what\">{}</pre><p class=\"meta\">{}</p><pre class=\"what\">{}</pre></details></li>",
            event.seq, event.seq, pages::utc((event.at / 1000).max(0) as u64), actor, escape(&event.action), escape(&copy.details),
            escape(&serde_json::to_string_pretty(&event.data).expect("event JSON")), escape(&copy.hash), escape(&event.hash))
    }).collect();
    let history = if rows.is_empty() {
        format!("<p class=\"empty\">{}</p>", escape(&copy.empty))
    } else {
        format!("<ol class=\"rows\" id=\"audit-events\">{rows}</ol>")
    };
    let older = if events.len() > 100 {
        format!(
            "<a class=\"button\" id=\"audit-older\" href=\"{}\">{}</a>",
            shown.href(project, Some(events[99].seq), false),
            escape(&copy.older)
        )
    } else {
        String::new()
    };
    let refresh = shown.href(project, None, false);
    format!(
        "<div class=\"stack\" data-page=\"audit-history\" data-changes=\"{path}/changes?after={head}\" data-refresh=\"{refresh}\"><header><a href=\"/app/boards/{}\">{}</a><h1>{}</h1><p>{}</p></header>\
<form class=\"stack\" method=\"get\" action=\"{path}\">{fields}<button type=\"submit\">{}</button></form>\
<div class=\"cluster\"><a class=\"button\" href=\"{refresh}\">{}</a><a class=\"button\" href=\"{}\">{}</a></div>{status}\
<p role=\"status\" id=\"audit-updated\" hidden><a href=\"{refresh}\">{}</a></p><p role=\"alert\" id=\"audit-unavailable\" hidden>{}</p>\
<section class=\"card\">{history}</section>{older}</div><script src=\"/assets/audit-history.js\" defer></script>",
        encode(project),
        escape(project),
        escape(&copy.title),
        escape(&copy.description),
        escape(&copy.filter),
        escape(&copy.refresh),
        shown.href(project, shown.before, true),
        escape(&copy.verify),
        escape(&copy.updated),
        escape(&copy.unavailable)
    )
}
