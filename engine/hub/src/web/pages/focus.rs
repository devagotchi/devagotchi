//! Navigation destinations in the app focus sheet.
use super::{Site, escape};

/// The Go to sheet (`app.js` opens it and fills in the boards and machines from `/app/focus`):
/// a section per place, in the tabs' words, side by side so they swipe, each also a chip to tap.
pub(super) fn focus_sheet(site: &Site) -> String {
    let (words, focus) = (&site.messages.app, &site.messages.focus);
    let e = |text: &str| escape(text);
    let link = |href: &str, text: &str| format!("<li><a href=\"{href}\">{}</a></li>", e(text));
    let fill = |what: &str| {
        format!(
            "<ul class=\"rows\" data-fill=\"{what}\"><li class=\"meta\">{}</li></ul>",
            e(&focus.loading)
        )
    };
    let sections = [
        (
            "agenthub",
            &words.tab_boards,
            "/app/boards /approvals",
            format!(
                "<ul class=\"rows\">{}{}</ul>{}",
                link("/app/boards", &focus.all_boards),
                link("/approvals", &words.tab_approvals),
                fill("boards")
            ),
        ),
        (
            "overview",
            &words.tab_home,
            "/app",
            format!(
                "<ul class=\"rows\">{}{}</ul>",
                link("/app", &words.tab_home),
                link("/app#needs-you", &focus.needs_you)
            ),
        ),
        (
            "machines",
            &words.tab_machines,
            "/app/machines",
            format!(
                "<ul class=\"rows\">{}</ul>{}",
                link("/app/machines", &focus.all_machines),
                fill("machines")
            ),
        ),
        (
            "you",
            &site.messages.me.tab,
            "/app/me",
            format!(
                "<ul class=\"rows\">{}{}{}{}<li data-owner hidden><a href=\"/admin\">{}</a></li></ul>",
                link("/app/me", &focus.settings),
                link("/app/keys", &focus.keys),
                link("/app/agents", &site.messages.app.agents.title),
                link("/connect", &focus.connect),
                e(&focus.admin)
            ),
        ),
    ];
    let chips: String = sections
        .iter()
        .map(|(id, label, _, _)| format!("<a class=\"chip\" href=\"#focus-{id}\">{}</a>", e(label)))
        .collect();
    let panels: String = sections
        .iter()
        .map(|(id, label, tabs, body)| {
            format!(
                "<section class=\"sheet-panel\" id=\"focus-{id}\" data-tabs=\"{tabs}\" aria-label=\"{}\">{body}</section>",
                e(label)
            )
        })
        .collect();
    format!(
        "<dialog class=\"sheet\" id=\"focus\" aria-labelledby=\"focus-title\" data-no-boards=\"{}\" \
data-no-machines=\"{}\"><div class=\"sheet-grip\" aria-hidden=\"true\"></div>\
<header class=\"sheet-head\"><h2 id=\"focus-title\">{}</h2><form method=\"dialog\">\
<button class=\"sheet-close\" type=\"submit\" aria-label=\"{}\">&times;</button></form></header>\
<nav class=\"sheet-chips\" aria-label=\"{}\">{chips}</nav>\
<div class=\"sheet-panels\" id=\"focus-panels\">{panels}</div></dialog>",
        e(&focus.no_boards),
        e(&focus.no_machines),
        e(&focus.title),
        e(&focus.close),
        e(&focus.title)
    )
}
