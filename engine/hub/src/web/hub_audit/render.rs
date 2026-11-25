use super::*;
use crate::{
    ops::audit::{ChainStatus, Recorded},
    site::{Ctx, Site},
};

pub(super) fn body(
    shown: &Shown,
    head: u64,
    events: &[Recorded],
    chain: Option<&ChainStatus>,
    site: &Site,
    me: &str,
) -> String {
    let copy = &site.messages.app.audit;
    let hub = &site.messages.app.hub_audit;
    let fields: String = [
        ("actor", &copy.actor, &shown.actor),
        ("action", &copy.action, &shown.action),
        ("machine", &hub.machine, &shown.machine),
        ("outcome", &hub.outcome, &shown.outcome),
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
        Some(_) => format!(
            "<p role=\"alert\" id=\"audit-broken\">{}</p>",
            escape(&hub.broken)
        ),
        None => format!("<p class=\"note\">{}</p>", escape(&copy.unverified)),
    };
    let rows: String = events.iter().take(100).map(|event| {
        let actor = if event.subject == me { &site.messages.app.you } else { &event.subject };
        let details: String = [(&hub.machine, &event.broker), (&hub.outcome, &event.outcome), (&hub.via, &event.via), (&hub.operation, &event.operation_id), (&copy.hash, &event.hash)]
            .into_iter().map(|(label, value)| format!("<dt>{}</dt><dd><pre class=\"what\">{}</pre></dd>", escape(label), escape(value))).collect();
        format!("<li id=\"event-{}\"><div class=\"meta\">#{} · {} · <span title=\"{}\">{}</span></div><p>{} · {}</p><details><summary>{}</summary><dl>{details}</dl></details></li>", event.seq, event.seq, escape(&event.at), escape(&event.subject), escape(actor), escape(&event.tool), escape(&event.outcome), escape(&copy.details))
    }).collect();
    let history = if rows.is_empty() {
        format!("<p class=\"empty\">{}</p>", escape(&copy.empty))
    } else {
        format!("<ol class=\"rows\" id=\"audit-events\">{rows}</ol>")
    };
    let older = if events.len() > 100 {
        format!(
            "<a class=\"button\" id=\"audit-older\" href=\"{}\">{}</a>",
            shown.href(Some(events[99].seq), false),
            escape(&copy.older)
        )
    } else {
        String::new()
    };
    let refresh = shown.href(None, false);
    format!(
        "<div class=\"stack\" data-page=\"audit-history\" data-changes=\"/admin/audit/changes?after={head}\" data-refresh=\"{refresh}\"><header><a href=\"/admin\">{}</a><h1>{}</h1><p>{}</p></header>\
<form class=\"stack\" method=\"get\" action=\"/admin/audit\">{fields}<button type=\"submit\">{}</button></form>\
<div class=\"cluster\"><a class=\"button\" href=\"{refresh}\">{}</a><a class=\"button\" href=\"{}\">{}</a></div>{status}\
<p role=\"status\" id=\"audit-updated\" hidden><a href=\"{refresh}\">{}</a></p><p role=\"alert\" id=\"audit-unavailable\" hidden>{}</p>\
<section class=\"card\">{history}</section>{older}</div><script src=\"/assets/audit-history.js\" defer></script>",
        escape(&hub.back),
        escape(&hub.title),
        escape(&hub.description),
        escape(&copy.filter),
        escape(&copy.refresh),
        shown.href(shown.before, true),
        escape(&copy.verify),
        escape(&copy.updated),
        escape(&copy.unavailable)
    )
}
