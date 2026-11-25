//! The setup walkthrough beside the hero's words: the steps of getting started, each a scene
//! drawn in HTML (`site/walkthrough.rs`). The steps are a group of radio buttons, so choosing
//! one shows its scene with no script (the stylesheet's `:has(:checked)`); the page's script
//! (`assets/landing.js`) moves to the next step after each step's seconds, shows how long is
//! left, and stops on hover, focus, Pause, a hidden tab, and for people who ask for less
//! motion. The scenes are pictures: a screen reader reads each step's name and caption.

use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};

use super::{Ctx, Front, pick};
use crate::site::text::{attr, escape_text};
use crate::site::walkthrough::{Screen, ScreenKind, WalkStep};

/// The walkthrough for `front`, or nothing when the site has none for it.
pub fn render(ctx: &Ctx, front: Front) -> Option<String> {
    let walkthrough = ctx.site.landing.hero.walkthrough.as_ref()?;
    let steps = pick(&walkthrough.steps, front)?;
    let label = ctx.plain(&walkthrough.label);
    let scenes: String = steps
        .iter()
        .enumerate()
        .map(|(i, step)| scene(ctx, i, step))
        .collect();
    let tabs: String = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            format!(
                "<label class=\"walk-tab\"><input type=\"radio\" name=\"walk\" value=\"{n}\"{checked}>\
<span class=\"walk-n\" aria-hidden=\"true\">{n}</span><span class=\"walk-name\">{tab}</span>\
<span class=\"visually-hidden\">: {caption}</span><span class=\"walk-bar\" aria-hidden=\"true\"></span></label>",
                n = i + 1,
                checked = if i == 0 { " checked" } else { "" },
                tab = text(ctx, &step.tab),
                caption = text(ctx, &step.caption),
            )
        })
        .collect();
    let captions: String = steps
        .iter()
        .map(|step| format!("<span>{}</span>", text(ctx, &step.caption)))
        .collect();
    Some(format!(
        "<figure class=\"walkthrough\" data-walkthrough>\
<div class=\"walk-stage\" aria-hidden=\"true\">{scenes}</div>\
<p class=\"walk-caption\" aria-hidden=\"true\">{captions}</p>\
<figcaption class=\"walk-nav\"><fieldset class=\"walk-tabs\"><legend class=\"visually-hidden\">{legend}</legend>{tabs}</fieldset>\
<button type=\"button\" class=\"walk-pause\" hidden data-pause=\"{pause}\" data-play=\"{play}\">{pause}</button>\
</figcaption></figure>",
        legend = escape_text(&label),
        pause = attr(&ctx.plain(&walkthrough.pause)),
        play = attr(&ctx.plain(&walkthrough.play)),
    ))
}

/// Copy as plain text, its placeholders filled.
fn text(ctx: &Ctx, copy: &str) -> String {
    escape_text(&ctx.plain(copy))
}

fn scene(ctx: &Ctx, i: usize, step: &WalkStep) -> String {
    let screens: String = step.screens.iter().map(|s| screen(ctx, s)).collect();
    format!(
        "<div class=\"walk-scene screens-{count}\" data-step=\"{n}\" data-seconds=\"{seconds}\">{screens}</div>",
        n = i + 1,
        count = step.screens.len(),
        seconds = step.seconds(),
    )
}

fn screen(ctx: &Ctx, screen: &Screen) -> String {
    match screen.kind {
        ScreenKind::Terminal => terminal(ctx, screen),
        ScreenKind::Window => window(ctx, screen, "win"),
        ScreenKind::Phone if !screen.notice.is_empty() => lock_screen(ctx, screen),
        ScreenKind::Phone => window(ctx, screen, "handset"),
        ScreenKind::Chat => chat(ctx, screen),
    }
}

/// A title bar: three dots and the title (a window's address).
fn bar(ctx: &Ctx, title: &str) -> String {
    format!(
        "<div class=\"scr-bar\"><i></i><i></i><i></i><span>{}</span></div>",
        text(ctx, title)
    )
}

fn terminal(ctx: &Ctx, screen: &Screen) -> String {
    let lines: String = screen
        .lines
        .iter()
        .map(|line| {
            let line = ctx.plain(line);
            if let Some(command) = line.strip_prefix("$ ") {
                format!(
                    "<div class=\"ln cmd\"><span class=\"ps\">$</span> <span class=\"typed\">{}</span></div>",
                    escape_text(command)
                )
            } else if let Some(shown) = line.strip_prefix("! ") {
                format!("<div class=\"ln hl\">{}</div>", escape_text(shown))
            } else {
                format!("<div class=\"ln\">{}</div>", escape_text(&line))
            }
        })
        .collect();
    format!(
        "<div class=\"scr tty\">{}<div class=\"scr-body\">{lines}</div></div>",
        bar(ctx, &screen.title)
    )
}

/// A page in a browser window (`win`) or on a phone (`handset`).
fn window(ctx: &Ctx, screen: &Screen, class: &str) -> String {
    let mut page = String::new();
    if !screen.heading.is_empty() {
        page.push_str(&format!(
            "<p class=\"pg-h ln\">{}</p>",
            text(ctx, &screen.heading)
        ));
    }
    for line in &screen.lines {
        page.push_str(&format!("<p class=\"ln\">{}</p>", text(ctx, line)));
    }
    for field in &screen.fields {
        page.push_str(&format!(
            "<div class=\"fld ln\"><span class=\"fld-l\">{}</span><span class=\"fld-v\"><span class=\"typed\">{}</span></span></div>",
            text(ctx, &field.label),
            text(ctx, &field.value)
        ));
    }
    if !screen.code.is_empty() {
        page.push_str(&format!(
            "<p class=\"code ln\">{}</p>",
            text(ctx, &screen.code)
        ));
    }
    if !screen.button.is_empty() || !screen.secondary.is_empty() {
        let secondary = if screen.secondary.is_empty() {
            String::new()
        } else {
            format!(
                "<span class=\"btn\">{}</span>",
                text(ctx, &screen.secondary)
            )
        };
        page.push_str(&format!(
            "<p class=\"acts ln\"><span class=\"btn pri tap\">{}</span>{secondary}</p>",
            text(ctx, &screen.button)
        ));
    }
    if !screen.status.is_empty() {
        page.push_str(&format!(
            "<p class=\"done\">{}</p>",
            text(ctx, &screen.status)
        ));
    }
    format!(
        "<div class=\"scr {class}\">{}<div class=\"scr-body\">{page}</div></div>",
        bar(ctx, &screen.title)
    )
}

/// A phone's lock screen with one notification, tapped.
fn lock_screen(ctx: &Ctx, screen: &Screen) -> String {
    format!(
        "<div class=\"scr handset lock\"><div class=\"scr-body\"><p class=\"clock\">{time}</p>\
<div class=\"notice tap\"><p class=\"notice-t\">{title}</p><p>{body}</p></div></div></div>",
        time = text(ctx, &screen.title),
        title = text(ctx, &screen.notice[0]),
        body = text(ctx, &screen.notice[1]),
    )
}

fn chat(ctx: &Ctx, screen: &Screen) -> String {
    let lines: String = screen
        .lines
        .iter()
        .map(|line| {
            let line = ctx.plain(line);
            if let Some(said) = line.strip_prefix("you: ") {
                format!("<p class=\"ln say you\">{}</p>", escape_text(said))
            } else if let Some(said) = line.strip_prefix("ai: ") {
                format!("<p class=\"ln say ai\">{}</p>", escape_text(said))
            } else {
                let call = line.strip_prefix("tool: ").unwrap_or(&line);
                let (name, what) = call.split_once(' ').unwrap_or((call, ""));
                format!(
                    "<p class=\"ln tool\"><span class=\"tool-n\">{}</span> {}</p>",
                    escape_text(name),
                    escape_text(what)
                )
            }
        })
        .collect();
    format!(
        "<div class=\"scr chat\">{}<div class=\"scr-body\">{lines}</div></div>",
        bar(ctx, &screen.title)
    )
}

/// `GET /assets/landing.js`: the script that moves the walkthrough along.
pub async fn script() -> Response {
    let mut response = include_str!("../assets/landing.js").into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
