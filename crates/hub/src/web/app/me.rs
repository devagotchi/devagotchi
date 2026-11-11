//! `/app/me`, the person's own page: who is signed in and in which organisations, this
//! phone's notifications, and what they want (`crate::preferences`): what to call them, how
//! the app looks, how long "approve and allow" lasts, whether kinds are offered, which kinds
//! always wait for them, and whether their machines may buzz their phone. Sign out lives here.
//!
//! A kind newly set to always ask stops what this person approved ahead of time for it, on
//! every board they decide on. What someone else allowed on a shared board stays theirs to
//! stop, from the same Approvals page.

use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

use super::{person, words};
use crate::{
    AppState,
    accounts::identities::UNNAMED,
    auth::Subject,
    board::Actor,
    preferences::{ALLOW_CHOICES, Preferences, Theme, parse_kinds},
    web::{
        approvals,
        pages::{self, Me, Tab, escape},
        routes::{see_other, unavailable},
    },
};

#[derive(Deserialize)]
pub struct Shown {
    saved: Option<String>,
    stopped: Option<usize>,
}

/// The preferences form. An unticked box sends nothing.
#[derive(Deserialize)]
pub struct Choices {
    #[serde(default)]
    name: String,
    theme: String,
    allow_days: u32,
    #[serde(default)]
    offer_ahead: Option<String>,
    #[serde(default)]
    always_ask: String,
    #[serde(default)]
    machine_pushes: Option<String>,
}

/// The tenant a person's preferences live under: their own, or on a hub whose accounts live
/// elsewhere its home tenant. None where the hub keeps no tenants.
async fn tenant_of(state: &AppState, subject: &Subject) -> Option<String> {
    if let Some(tenant) = &subject.tenant {
        return Some(tenant.clone());
    }
    state.hub.boards.orgs()?.home_tenant().await.ok().flatten()
}

/// This person's preferences. A store that fails is the defaults and a line in the log: what
/// someone likes never stops them approving something.
pub(in crate::web) async fn of(state: &AppState, subject: &Subject) -> Preferences {
    let Some(store) = state.hub.boards.preferences() else {
        return Preferences::default();
    };
    let Some(tenant) = tenant_of(state, subject).await else {
        return Preferences::default();
    };
    store
        .get(&tenant, &subject.id)
        .await
        .unwrap_or_else(|error| {
            eprintln!("devagotchi hub: preferences: {error}");
            Preferences::default()
        })
}

/// How they signed in, as they would recognise it: the login their identity provider
/// reported, or the address their account signs in with.
async fn login_of(state: &AppState, subject: &Subject) -> Option<String> {
    if let Some(login) = subject.name.as_deref().map(str::trim)
        && !login.is_empty()
        && login != UNNAMED
    {
        return Some(login.to_string());
    }
    let hub = state.passkeys.as_ref()?;
    hub.accounts().address_of(&subject.id).await.ok().flatten()
}

/// What to call someone from a login: its first part, with a capital.
fn first_part(login: &str) -> Option<String> {
    let local = login.split('@').next().unwrap_or(login);
    let first = local
        .split(|c: char| matches!(c, '.' | '_' | '-' | '+') || c.is_whitespace())
        .find(|part| !part.is_empty())?;
    let mut letters = first.chars();
    let head = letters.next()?;
    Some(head.to_uppercase().chain(letters).collect())
}

/// The letters of their round mark: the first of two names, or of the one.
fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next());
    let last = words.last().and_then(|w| w.chars().next());
    first
        .into_iter()
        .chain(last)
        .flat_map(char::to_uppercase)
        .collect()
}

/// Who the app's frame says is signed in.
pub(in crate::web) async fn identity(state: &AppState, subject: &Subject) -> Me {
    let preferences = of(state, subject).await;
    let name = match preferences.name {
        Some(name) => Some(name),
        None => login_of(state, subject)
            .await
            .as_deref()
            .and_then(first_part),
    }
    .unwrap_or_else(|| words().messages.me.tab.clone());
    Me {
        initials: initials(&name),
        name,
        theme: preferences.theme,
    }
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let subject = match person(&state, &headers, "/app/me").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let site = words();
    let copy = &site.messages.me;
    let preferences = of(&state, &subject).await;
    let me = identity(&state, &subject).await;

    let flash = match (shown.saved.as_deref(), shown.stopped.unwrap_or(0)) {
        (Some(_), 0) => format!("<p class=\"note\" id=\"flash\">{}</p>", escape(&copy.saved)),
        (Some(_), n) => format!(
            "<p class=\"note\" id=\"flash\">{}</p>",
            crate::site::Ctx::new(&site)
                .with("n", n.to_string())
                .inline(&copy.saved_stopped)
        ),
        _ => String::new(),
    };
    let login = login_of(&state, &subject)
        .await
        .filter(|login| *login != me.name)
        .map(|login| format!("<p class=\"meta\" id=\"login\">{}</p>", escape(&login)))
        .unwrap_or_default();
    let who = format!(
        "<section class=\"who\" id=\"who\"><span class=\"avatar large\">{}</span>\
<div><h1>{}</h1>{login}</div></section>",
        escape(&me.initials),
        escape(&me.name),
    );

    let orgs = match state.hub.boards.orgs() {
        Some(orgs) => {
            let rows: Vec<String> = match orgs.of_subject(&subject.id).await {
                Ok(found) => found
                    .iter()
                    .map(|(org, role)| {
                        format!(
                            "<li id=\"org-{s}\"><a href=\"/orgs/{s}\"><strong>{}</strong></a> \
<span class=\"meta\">{}</span></li>",
                            escape(&org.name),
                            escape(role.as_str()),
                            s = escape(&org.slug),
                        )
                    })
                    .collect(),
                Err(error) => {
                    eprintln!("devagotchi hub: organisations of a person: {error}");
                    return unavailable();
                }
            };
            format!(
                "{}<p><a href=\"/orgs\">{}</a></p>",
                super::card("orgs", &copy.orgs_title, &rows, &copy.orgs_none),
                escape(&copy.orgs_manage),
            )
        }
        None => String::new(),
    };

    let key = state.hub.push.as_ref().map(|p| p.public_key.clone());
    let can_test = match state.hub.approval_boards(&subject) {
        Ok(boards) => !boards.is_empty(),
        Err(error) => {
            eprintln!("devagotchi hub: approval boards: {error}");
            false
        }
    };
    let phone = approvals::phone_controls(key.as_deref(), can_test);
    let phone = if phone.is_empty() {
        String::new()
    } else {
        format!(
            "<section class=\"card\" id=\"phone\"><h2>{}</h2>{phone}</section>",
            escape(&copy.phone_title)
        )
    };

    let keeps =
        state.hub.boards.preferences().is_some() && tenant_of(&state, &subject).await.is_some();
    let form = if keeps {
        form(&preferences)
    } else {
        format!("<p class=\"note\">{}</p>", escape(&copy.not_kept))
    };
    let leave = format!(
        "<section class=\"card leave\" id=\"leave\"><p><a href=\"/app/keys\" id=\"keys-link\">API keys</a></p><p><a href=\"/account\">{}</a></p>\
<form method=\"post\" action=\"/auth/logout\"><button type=\"submit\">{}</button></form></section>",
        escape(&copy.account_link),
        escape(&copy.sign_out),
    );
    pages::app(
        StatusCode::OK,
        &copy.title,
        &format!("{who}{flash}{orgs}{phone}{form}{leave}"),
        Tab::Me,
    )
}

/// The preferences form, showing what is set now.
fn form(preferences: &Preferences) -> String {
    let site = words();
    let copy = &site.messages.me;
    let option = |value: &str, label: &str, on: bool| {
        format!(
            "<option value=\"{value}\"{}>{}</option>",
            if on { " selected" } else { "" },
            escape(label)
        )
    };
    let themes = [
        (Theme::System, &copy.theme_system),
        (Theme::Light, &copy.theme_light),
        (Theme::Dark, &copy.theme_dark),
    ]
    .iter()
    .map(|(theme, label)| option(theme.as_str(), label, preferences.theme == *theme))
    .collect::<String>();
    let days = ALLOW_CHOICES
        .iter()
        .map(|days| {
            let label = crate::site::Ctx::new(&site)
                .with("n", days.to_string())
                .plain(&copy.days);
            option(&days.to_string(), &label, preferences.allow_days == *days)
        })
        .collect::<String>();
    let ticked = |on: bool| if on { " checked" } else { "" };
    format!(
        "<form class=\"card prefs\" id=\"prefs\" method=\"post\" action=\"/app/me\">\
<h2>{title}</h2>\
<label>{name_label}<input type=\"text\" name=\"name\" maxlength=\"40\" autocomplete=\"nickname\" value=\"{name}\"></label>\
<label>{theme_label}<select name=\"theme\">{themes}</select></label>\
<h3>{loop_title}</h3>\
<label>{days_label}<select name=\"allow_days\">{days}</select></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"offer_ahead\" value=\"on\"{offer}><span>{offer_label}</span></label>\
<label>{always_label}<input type=\"text\" name=\"always_ask\" autocapitalize=\"none\" autocorrect=\"off\" spellcheck=\"false\" value=\"{always}\"></label>\
<p class=\"note help\">{always_help}</p>\
<h3>{phone_title}</h3>\
<label class=\"check\"><input type=\"checkbox\" name=\"machine_pushes\" value=\"on\"{pushes}><span>{pushes_label}</span></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">{save}</button></p></form>",
        title = escape(&copy.prefs_title),
        name_label = escape(&copy.name_label),
        name = escape(preferences.name.as_deref().unwrap_or_default()),
        theme_label = escape(&copy.theme_label),
        loop_title = escape(&copy.loop_title),
        days_label = escape(&copy.days_label),
        offer = ticked(preferences.offer_ahead),
        offer_label = escape(&copy.offer_label),
        always_label = escape(&copy.always_label),
        always = escape(&preferences.always_ask.join(", ")),
        always_help = escape(&copy.always_help),
        phone_title = escape(&copy.phone_title),
        pushes = ticked(preferences.machine_pushes),
        pushes_label = escape(&copy.pushes_label),
        save = escape(&copy.save),
    )
}

pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(choices): Form<Choices>,
) -> Response {
    let site = words();
    let copy = &site.messages.me;
    if !approvals::same_origin(&headers, &state.hub.config.public_url) {
        return pages::message(
            StatusCode::FORBIDDEN,
            &copy.not_saved,
            &site.messages.app.not_from_here,
            Some(("/app/me", &copy.title)),
        );
    }
    let subject = match person(&state, &headers, "/app/me").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let (Some(store), Some(tenant)) = (
        state.hub.boards.preferences(),
        tenant_of(&state, &subject).await,
    ) else {
        return pages::message(
            StatusCode::CONFLICT,
            &copy.not_saved,
            &copy.not_kept,
            Some(("/app/me", &copy.title)),
        );
    };
    let refused = |why: &str| {
        pages::message(
            StatusCode::BAD_REQUEST,
            &copy.not_saved,
            why,
            Some(("/app/me", &copy.title)),
        )
    };
    let Some(theme) = Theme::parse(&choices.theme) else {
        return refused("That is not one of the appearances.");
    };
    let name = choices.name.trim();
    let next = Preferences {
        name: (!name.is_empty()).then(|| name.to_string()),
        theme,
        allow_days: choices.allow_days,
        offer_ahead: choices.offer_ahead.is_some(),
        always_ask: parse_kinds(&choices.always_ask),
        machine_pushes: choices.machine_pushes.is_some(),
        ..Preferences::default()
    };
    let saved = match store.save(&tenant, &subject.id, &next).await {
        Ok(saved) => saved,
        Err(error) if error.code == "invalid" => return refused(&error.message),
        Err(error) => {
            eprintln!("devagotchi hub: saving preferences: {error}");
            return unavailable();
        }
    };
    let stopped = stop_own(&state, &subject, saved.added_always_ask).await;
    see_other(&format!("/app/me?saved=1&stopped={stopped}"), &[])
}

/// End what this person approved ahead of time for kinds that now always ask them, on every
/// board they decide on. How many ended. A board that fails is logged: the preference is saved
/// and the Approvals page still lists what is live, with Stop.
async fn stop_own(state: &AppState, subject: &Subject, kinds: Vec<String>) -> usize {
    if kinds.is_empty() {
        return 0;
    }
    let boards = match state.hub.approval_boards(subject) {
        Ok(boards) => boards,
        Err(error) => {
            eprintln!("devagotchi hub: approval boards: {error}");
            return 0;
        }
    };
    let id = subject.id.clone();
    tokio::task::spawn_blocking(move || {
        let actor = Actor {
            id: &id,
            human: true,
            wip_limit: None,
        };
        let mut stopped = 0;
        for (project, board) in boards {
            let now = board.now();
            let live = match board.standing() {
                Ok(live) => live,
                Err(error) => {
                    eprintln!("devagotchi hub: standing approvals on {project}: {error}");
                    continue;
                }
            };
            for standing in live {
                if standing.live(now) && standing.granted_by == id && kinds.contains(&standing.kind)
                {
                    match board.revoke_standing(&actor, &standing.id) {
                        Ok(_) => stopped += 1,
                        Err(error) => {
                            eprintln!("devagotchi hub: stopping on {project}: {error}");
                        }
                    }
                }
            }
        }
        stopped
    })
    .await
    .unwrap_or(0)
}
