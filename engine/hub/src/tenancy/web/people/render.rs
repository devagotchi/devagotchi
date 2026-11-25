use super::*;
use crate::site::Ctx;

pub(super) fn list(users: &[LocalUser]) -> Response {
    let site = pages::site();
    let c = &site.messages.app.people;
    let rows: String = users
        .iter()
        .take(100)
        .map(|u| {
            let role = if u.account.owner { &c.owner } else { &c.user };
            let status = if u.account.disabled_at.is_some() {
                &c.disabled
            } else {
                &c.active
            };
            format!(
                "<li><a href=\"/admin/users/{}\">{}</a><p>{} · {}</p><p class=\"what\">{}</p></li>",
                encode(&u.account.id),
                escape(&u.account.name),
                escape(role),
                escape(status),
                escape(u.email.as_deref().unwrap_or(&c.external))
            )
        })
        .collect();
    let rows = if rows.is_empty() {
        format!("<p>{}</p>", escape(&c.empty))
    } else {
        format!("<ul class=\"rows\" id=\"local-users\">{rows}</ul>")
    };
    let more = if users.len() > 100 {
        format!(
            "<a id=\"users-more\" href=\"/admin/users?after={}\">{}</a>",
            encode(&users[99].account.id),
            escape(&c.more)
        )
    } else {
        String::new()
    };
    page(
        StatusCode::OK,
        &c.title,
        &format!(
            "<div class=\"stack\"><header><a href=\"/admin\">{}</a><h1>{}</h1><p>{}</p></header><a class=\"button\" href=\"/admin/users/new\">{}</a><section class=\"card\">{rows}</section>{more}</div>",
            escape(&c.settings),
            escape(&c.title),
            escape(&c.description),
            escape(&c.add)
        ),
        Tab::Me,
    )
}

pub(super) fn new(input: &NewUser, min: u32, error: Option<&str>) -> Response {
    let site = pages::site();
    let c = &site.messages.app.people;
    let alert = error
        .map(|e| format!("<p role=\"alert\">{} {}</p>", escape(&c.refused), escape(e)))
        .unwrap_or_default();
    let status = if error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    page(
        status,
        &c.add,
        &format!(
            "<div class=\"stack\"><a href=\"/admin/users\">{}</a><h1>{}</h1>{alert}<form class=\"stack card\" method=\"post\" action=\"/admin/users\"><label>{}<input type=\"text\" name=\"name\" required maxlength=\"64\" value=\"{}\" autocomplete=\"off\"></label><label>{}<input type=\"email\" name=\"email\" required value=\"{}\" autocomplete=\"off\"></label><label>{}<input type=\"password\" name=\"password\" required minlength=\"{min}\" autocomplete=\"new-password\"></label><p class=\"note\">{}</p><button type=\"submit\">{}</button></form></div>",
            escape(&c.back),
            escape(&c.add),
            escape(&c.name),
            escape(&input.name),
            escape(&c.email),
            escape(&input.email),
            escape(&c.password),
            Ctx::new(&site)
                .with("min", min.to_string())
                .inline(&c.password_help),
            escape(&c.create)
        ),
        Tab::Me,
    )
}

pub(super) fn detail(user: &LocalUser, min: u32, saved: Option<&str>) -> Response {
    let site = pages::site();
    let c = &site.messages.app.people;
    let path = format!("/admin/users/{}", encode(&user.account.id));
    let notice = match saved {
        Some("created") => Some(&c.created),
        Some("updated") => Some(&c.updated),
        Some("reset") => Some(&c.reset_done),
        _ => None,
    }
    .map(|s| format!("<p role=\"status\">{}</p>", escape(s)))
    .unwrap_or_default();
    let checked = if user.account.disabled_at.is_none() {
        " checked"
    } else {
        ""
    };
    let (disabled, owner) = if user.account.owner {
        (
            " disabled",
            format!(
                "<input type=\"hidden\" name=\"enabled\" value=\"true\"><p>{}</p>",
                escape(&c.owner)
            ),
        )
    } else {
        ("", String::new())
    };
    let identity = escape(user.email.as_deref().unwrap_or(&c.external));
    let agents = format!(
        "<p><a href=\"/app/agents?account={}\">{}</a></p>",
        encode(&user.account.id),
        escape(&site.messages.app.agents.title)
    );
    let password = if user.email.is_some() {
        format!(
            "<form class=\"stack card\" method=\"post\" action=\"{path}/password\"><h2>{}</h2><input type=\"hidden\" name=\"version\" value=\"{}\"><label>{}<input type=\"password\" name=\"password\" required minlength=\"{min}\" autocomplete=\"new-password\"></label><p class=\"note\">{}</p><p>{}</p><button type=\"submit\">{}</button></form>",
            escape(&c.reset),
            user.version,
            escape(&c.password),
            Ctx::new(&site)
                .with("min", min.to_string())
                .inline(&c.password_help),
            escape(&c.reset_help),
            escape(&c.reset)
        )
    } else {
        String::new()
    };
    page(
        StatusCode::OK,
        &c.edit,
        &format!(
            "<div class=\"stack\"><a href=\"/admin/users\">{}</a><h1>{}</h1>{notice}<p class=\"what\">{identity}</p>{agents}<form class=\"stack card\" method=\"post\" action=\"{path}\"><input type=\"hidden\" name=\"version\" value=\"{}\"><label>{}<input type=\"text\" name=\"name\" required maxlength=\"64\" value=\"{}\" autocomplete=\"off\"></label><label><input type=\"checkbox\" name=\"enabled\" value=\"true\"{checked}{disabled}>{}</label>{owner}<p class=\"note\">{}</p><button type=\"submit\">{}</button></form>{password}</div>",
            escape(&c.back),
            escape(&c.edit),
            user.version,
            escape(&c.name),
            escape(&user.account.name),
            escape(&c.enabled),
            escape(&c.access_help),
            escape(&c.save)
        ),
        Tab::Me,
    )
}

fn page(status: StatusCode, title: &str, body: &str, tab: Tab) -> Response {
    let mut response = pages::app_page(status, title, body, tab);
    response.headers_mut().insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("same-origin"),
    );
    response
}
