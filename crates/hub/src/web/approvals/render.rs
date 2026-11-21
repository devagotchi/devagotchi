//! Human approval rendering.
use super::*;

pub(super) struct Flash<'a> {
    pub(super) done: Option<&'a str>,
    pub(super) test: Option<&'a str>,
    pub(super) ahead: Option<&'a str>,
}

/// "restart", or "restart on lab".
fn kind_words(kind: &str, target: Option<&str>) -> String {
    match target {
        Some(target) => format!("{kind} on {target}"),
        None => kind.to_string(),
    }
}

/// Who asked, as the person reads it: their own account, or its AI's seat on its own board.
fn asker(requested_by: &str, viewer: &str) -> String {
    if requested_by == viewer || requested_by == "ai" {
        "Your AI asks".into()
    } else if requested_by == TEST_ASKER {
        format!("{} asks", escape(pages::site().name()))
    } else {
        format!("{} asks", escape(requested_by))
    }
}

/// What, why and how to undo it, as the person reads them; a description that does not read
/// only one way, whole, as it was written.
fn facts(description: &str) -> String {
    match approval::parts(description) {
        Some((what, why, undo)) => format!(
            "<dl class=\"ask-facts\"><dt>What</dt><dd class=\"cmd\">{}</dd><dt>Why</dt><dd>{}</dd>\
<dt>Undo</dt><dd>{}</dd></dl>",
            escape(what),
            escape(why),
            escape(undo)
        ),
        None => format!("<pre class=\"what\">{}</pre>", escape(description)),
    }
}

/// "Notify this phone" and "Send me a test approval"; nothing on a hub without push.
pub(in crate::web) fn phone_controls(push_key: Option<&str>, can_test: bool) -> String {
    push_key
        .map(|key| {
            format!(
                "<p><button id=\"notify\" type=\"button\" data-key=\"{}\">Notify this phone</button> \
<span id=\"notify-status\" class=\"note\" role=\"status\"></span></p>{}",
                escape(key),
                if can_test {
                    "<form method=\"post\" action=\"/approvals/test\"><p><button class=\"test\" type=\"submit\">\
Send me a test approval</button></p></form>"
                } else {
                    ""
                }
            )
        })
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn page(
    pending: &[(String, ApprovalStatus)],
    flash: &Flash<'_>,
    push_key: Option<&str>,
    viewer: &str,
    can_test: bool,
    ahead: &[Ahead],
    preferences: &Preferences,
    access_count: Option<u64>,
) -> Response {
    let allow_days = preferences.allow_days;
    let decided = flash
        .done
        .map(|id| {
            format!(
                "<p class=\"note\">Decided {}. The agent continues on its own.</p>",
                escape(id)
            )
        })
        .unwrap_or_default();
    let tested = match flash.test {
        Some("sent") => {
            "<p class=\"note\">Test sent. Your phone should buzz within a few seconds: approve it from the notification.</p>"
        }
        Some("no-phone") => {
            "<p class=\"note\">Tap Notify this phone first, then send the test.</p>"
        }
        _ => "",
    };
    let changed = match flash.ahead {
        Some("granted") => format!(
            "<p class=\"note\">Done: that exact step is approved ahead of time for {allow_days} days. You can stop it below.</p>"
        ),
        Some("revoked") => "<p class=\"note\">Stopped: it needs you again.</p>".to_string(),
        _ => String::new(),
    };
    let flash = format!("{decided}{tested}{changed}");
    let cards: String = pending
        .iter()
        .map(|(project, a)| {
            format!(
                "<section class=\"card ask\" id=\"{p}-{i}\"><p class=\"ask-tags\">{kind}<span class=\"tag\">{p}</span></p>\
<h2>{t}</h2><p class=\"meta\">{by} · {when}</p>{d}<form method=\"post\" action=\"/approvals/decide\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"id\" value=\"{i}\">\
<input type=\"hidden\" name=\"version\" value=\"{v}\">\
<details class=\"note-box\"><summary>Add a note</summary>\
<label>Note (optional)<input type=\"text\" name=\"note\" maxlength=\"2000\"></label></details>\
<p class=\"actions\"><button class=\"approve\" name=\"verdict\" value=\"approved\">Approve</button> \
<button class=\"deny\" name=\"verdict\" value=\"denied\">Deny</button></p></form>{allow}</section>",
                p = escape(project),
                i = escape(&a.id),
                t = escape(&a.title),
                by = asker(&a.requested_by, viewer),
                kind = a
                    .kind
                    .as_deref()
                    .map(|kind| format!(
                        "<span class=\"tag kind\">{}</span>",
                        escape(&kind_words(kind, a.target.as_deref()))
                    ))
                    .unwrap_or_default(),
                when = pages::ago((a.requested_at / 1000).max(0) as u64),
                d = facts(&a.description),
                v = a.version,
                allow = a
                    .kind
                    .as_deref()
                    .map(|kind| if preferences.always_asks(kind) {
                        format!(
                            "<p class=\"note always\">You always decide {} yourself.</p>",
                            escape(kind)
                        )
                    } else if a.what().is_none() {
                        String::new()
                    } else { format!(
                        "<form method=\"post\" action=\"/approvals/decide\" class=\"allow\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"id\" value=\"{i}\">\
<input type=\"hidden\" name=\"version\" value=\"{v}\"><input type=\"hidden\" name=\"verdict\" value=\"approved\">\
<input type=\"hidden\" name=\"allow_days\" value=\"{allow_days}\">\
<p class=\"actions\"><button class=\"approve allow\" type=\"submit\">Approve, and allow this exact {k} for {allow_days} days</button></p></form>",
                        p = escape(project),
                        i = escape(&a.id),
                        v = a.version,
                        k = escape(&kind_words(kind, a.target.as_deref()))
                    ) })
                    .unwrap_or_default(),
            )
        })
        .collect();
    let body = if pending.is_empty() {
        "<p class=\"empty\">Nothing is waiting for you.</p>".to_string()
    } else {
        cards
    };
    let notify = phone_controls(push_key, can_test);
    let notify = if notify.is_empty() {
        notify
    } else {
        format!("<section class=\"card\" id=\"phone\"><h2>This phone</h2>{notify}</section>")
    };
    let ahead_cards: String = ahead
        .iter()
        .map(|board| {
            let p = escape(&board.project);
            let live: String = board
                .standing
                .iter()
                .map(|s| {
                    format!(
                        "<li id=\"standing-{id}\">{k} <code>{what}</code> <span class=\"meta\">by {by}, until {until}</span>\
<form method=\"post\" action=\"/approvals/standing/revoke\" class=\"inline\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"id\" value=\"{id}\">\
<button class=\"deny\" type=\"submit\">Stop</button></form></li>",
                        id = escape(&s.id),
                        k = escape(&kind_words(&s.kind, s.target.as_deref())),
                        what = escape(&s.what),
                        by = escape(&s.granted_by),
                        until = pages::utc((s.expires_at / 1000).max(0) as u64),
                    )
                })
                .collect();
            let offered: String = board
                .suggestions
                .iter()
                .map(|s| {
                    format!(
                        "<li class=\"suggestion\">You approved {k} {n} times in 30 days and never denied it: <code>{what}</code>\
<form method=\"post\" action=\"/approvals/standing\" class=\"inline\">\
<input type=\"hidden\" name=\"project\" value=\"{p}\"><input type=\"hidden\" name=\"kind\" value=\"{kind}\">\
<input type=\"hidden\" name=\"target\" value=\"{target}\"><input type=\"hidden\" name=\"what\" value=\"{what}\">\
<button class=\"approve\" type=\"submit\">Approve it automatically for {allow_days} days</button></form></li>",
                        k = escape(&kind_words(&s.kind, s.target.as_deref())),
                        n = s.approved,
                        what = escape(&s.what),
                        kind = escape(&s.kind),
                        target = escape(s.target.as_deref().unwrap_or_default()),
                    )
                })
                .collect();
            format!(
                "<section class=\"card\" id=\"ahead-{p}\"><h2>Approved ahead of time · {p}</h2>{}{}</section>",
                if live.is_empty() {
                    "<p class=\"empty\">Nothing yet: every step here asks you.</p>".to_string()
                } else {
                    format!("<ul class=\"rows\">{live}</ul>")
                },
                if offered.is_empty() {
                    String::new()
                } else {
                    format!("<ul class=\"rows\">{offered}</ul>")
                }
            )
        })
        .collect();
    layout::page(
        &format!(
            "{}{}",
            super::super::app::access::attention_link(access_count),
            flash
        ),
        &body,
        &ahead_cards,
        &notify,
    )
}
