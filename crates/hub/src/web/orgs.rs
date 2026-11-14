//! The organisation pages (increment 116): `/orgs` lists a person's organisations and makes
//! one; `/orgs/<org>` is one organisation, its projects (with who sees each, its own members
//! and its last week of events when the hub exports to ClickHouse), its members and their
//! roles, its teams and its invites; `/invite` takes an invite link. Every change is a
//! form posted from this hub's pages (`same_origin`), by a person signed in to it, and is
//! the call `crate::orgs` makes for the API: the same checks, the same chain. Every word is
//! the site's (`messages.orgs`).

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use devagotchi_contracts::BridgeError;
use serde::Deserialize;

use super::{
    approvals::same_origin,
    encode,
    pages::{self, Tab, escape},
    routes::{Who, not_set_up, see_other, unavailable, who},
};
use crate::{
    AppState,
    auth::Subject,
    orgs::{Org, Orgs, Principal, ProjectRef, Role, Visibility, owner_of},
    site::{Ctx, Site, model::OrgsCopy},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs", get(list).post(create))
        .route("/orgs/{org}", get(show))
        .route("/orgs/{org}/members", post(set_member))
        .route("/orgs/{org}/members/remove", post(remove_member))
        .route("/orgs/{org}/teams", post(create_team))
        .route("/orgs/{org}/teams/delete", post(delete_team))
        .route("/orgs/{org}/teams/member", post(team_member))
        .route("/orgs/{org}/projects", post(create_project))
        .route("/orgs/{org}/projects/visibility", post(set_visibility))
        .route("/orgs/{org}/projects/member", post(project_member))
        .route("/orgs/{org}/invites", post(create_invite))
        .route("/orgs/{org}/invites/revoke", post(revoke_invite))
        .route("/invite", get(invite_page).post(accept))
        .route("/assets/invite.js", get(script))
}

fn site() -> std::sync::Arc<Site> {
    pages::site()
}

fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let mut response = pages::app_page(status, title, body, Tab::Boards);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The person signed in, or the page that answers instead.
async fn signed_in(state: &AppState, headers: &HeaderMap, here: &str) -> Result<Subject, Response> {
    match who(state, headers).await {
        Who::Person(subject) => Ok(subject),
        Who::SignedOut => Err(see_other(
            &format!("/auth/login?return_to={}", encode(here)),
            &[],
        )),
        Who::Unavailable => Err(unavailable()),
        Who::NotSetUp => Err(not_set_up()),
    }
}

/// A change: from this hub's page, by a person signed in, whose tenant is not waiting.
async fn changer(state: &AppState, headers: &HeaderMap, here: &str) -> Result<Subject, Response> {
    let copy = &site().messages.orgs;
    if !same_origin(headers, &state.hub.config.public_url) {
        return Err(page(
            StatusCode::FORBIDDEN,
            &copy.title,
            &format!("<p class=\"note\">{}</p>", escape(&copy.from_here)),
        ));
    }
    let subject = signed_in(state, headers, here).await?;
    if let Some(refused) = state.hub.waiting_refusal(&subject) {
        return Err(back(here, Err(refused)));
    }
    Ok(subject)
}

fn orgs_of(state: &AppState) -> Result<Orgs, Response> {
    state.hub.boards.orgs().cloned().ok_or_else(unavailable)
}

/// Back to `here` saying how it went.
fn back(here: &str, result: Result<(), BridgeError>) -> Response {
    match result {
        Ok(()) => see_other(&format!("{here}?done=1"), &[]),
        Err(error) => see_other(&format!("{here}?why={}", encode(&error.message)), &[]),
    }
}

#[derive(Deserialize, Default)]
pub struct Shown {
    done: Option<String>,
    why: Option<String>,
}

fn flash(site: &Site, shown: &Shown) -> String {
    let copy = &site.messages.orgs;
    if let Some(why) = shown.why.as_deref().filter(|why| !why.is_empty()) {
        let why: String = why.chars().take(300).collect();
        return format!(
            "<p class=\"note\" role=\"status\" id=\"flash\">{}</p>",
            Ctx::new(site).with("why", why).inline(&copy.not_changed)
        );
    }
    if shown.done.is_some() {
        return format!(
            "<p class=\"note\" role=\"status\" id=\"flash\">{}</p>",
            escape(&copy.done)
        );
    }
    String::new()
}

fn role_word(copy: &OrgsCopy, role: Role) -> &str {
    match role {
        Role::Owner => &copy.role_owner,
        Role::Admin => &copy.role_admin,
        Role::Member => &copy.role_member,
        Role::Agent => &copy.role_agent,
        Role::Viewer => &copy.role_viewer,
    }
}

/// A role picker: the roles up to `most`, `chosen` selected.
fn role_select(copy: &OrgsCopy, most: Role, chosen: Role, owner_too: bool) -> String {
    let options: String = Role::ALL
        .into_iter()
        .filter(|role| *role <= most && (owner_too || *role != Role::Owner))
        .map(|role| {
            format!(
                "<option value=\"{}\"{}>{}</option>",
                role.as_str(),
                if role == chosen { " selected" } else { "" },
                escape(role_word(copy, role))
            )
        })
        .collect();
    format!(
        "<label>{} <select name=\"role\">{options}</select></label>",
        escape(&copy.role)
    )
}

fn visibility_select(copy: &OrgsCopy, chosen: Visibility) -> String {
    let option = |value: Visibility, word: &str| {
        format!(
            "<option value=\"{}\"{}>{}</option>",
            match value {
                Visibility::Org => "org",
                Visibility::Members => "members",
            },
            if value == chosen { " selected" } else { "" },
            escape(word)
        )
    };
    format!(
        "<label>{} <select name=\"visibility\">{}{}</select></label>",
        escape(&copy.visibility),
        option(Visibility::Org, &copy.visibility_org),
        option(Visibility::Members, &copy.visibility_members)
    )
}

fn form(action: &str, inner: &str, button: &str) -> String {
    format!(
        "<form method=\"post\" action=\"{}\" class=\"inline\">{inner}<button class=\"button\" type=\"submit\">{}</button></form>",
        escape(action),
        escape(button)
    )
}

fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{name}\" value=\"{}\">",
        escape(value)
    )
}

// ---- /orgs --------------------------------------------------------------------------

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let subject = match signed_in(&state, &headers, "/orgs").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let orgs = match orgs_of(&state) {
        Ok(orgs) => orgs,
        Err(response) => return response,
    };
    let site = site();
    let copy = &site.messages.orgs;
    let held = match orgs.of_subject(&subject.id).await {
        Ok(held) => held,
        Err(error) => {
            eprintln!("devagotchi hub: organisations page: {error}");
            return unavailable();
        }
    };
    let rows = if held.is_empty() {
        format!("<p class=\"empty\">{}</p>", escape(&copy.none))
    } else {
        let items: String = held
            .iter()
            .map(|(org, role)| {
                format!(
                    "<li><a href=\"/orgs/{}\">{}</a> <span class=\"meta\">{}</span></li>",
                    encode(&org.slug),
                    escape(&org.name),
                    escape(role_word(copy, *role))
                )
            })
            .collect();
        format!("<ul class=\"rows\" id=\"orgs\">{items}</ul>")
    };
    let body = format!(
        "<h1>{title}</h1>{flash}<section class=\"card\"><h2>{yours}</h2>{rows}</section>\
<section class=\"card\"><h2>{new}</h2><form method=\"post\" action=\"/orgs\" id=\"new-org\">\
<label>{slug} <input name=\"org\" required maxlength=\"63\" pattern=\"[a-z0-9][a-z0-9-]{{0,62}}\" autocomplete=\"off\"></label>\
<label>{name} <input name=\"name\" required maxlength=\"100\"></label>\
<button class=\"button\" type=\"submit\">{create}</button></form></section>",
        title = escape(&copy.title),
        flash = flash(&site, &shown),
        yours = escape(&copy.yours),
        new = escape(&copy.new_title),
        slug = escape(&copy.slug),
        name = escape(&copy.name),
        create = escape(&copy.create),
    );
    page(StatusCode::OK, &copy.title, &body)
}

#[derive(Deserialize)]
pub struct NewOrg {
    org: String,
    name: String,
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<NewOrg>,
) -> Response {
    let subject = match changer(&state, &headers, "/orgs").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let orgs = match orgs_of(&state) {
        Ok(orgs) => orgs,
        Err(response) => return response,
    };
    let slug = input.org.trim().to_ascii_lowercase();
    match orgs.create(&subject.id, &slug, &input.name).await {
        Ok(org) => see_other(&format!("/orgs/{}?done=1", encode(&org.slug)), &[]),
        Err(error) => back("/orgs", Err(error)),
    }
}

// ---- /orgs/<org> --------------------------------------------------------------------

fn unknown_org() -> Response {
    let copy = &site().messages.orgs;
    page(
        StatusCode::NOT_FOUND,
        &copy.unknown,
        &format!(
            "<h1>{}</h1><p class=\"note\">{}</p><p><a href=\"/orgs\">{}</a></p>",
            escape(&copy.unknown),
            escape(&copy.unknown_text),
            escape(&copy.title)
        ),
    )
}

/// The organisation `slug` and the caller's role in it, or the page that answers instead.
async fn membership(orgs: &Orgs, slug: &str, subject: &Subject) -> Result<(Org, Role), Response> {
    let org = match orgs.by_slug(slug).await {
        Ok(Some(org)) => org,
        Ok(None) => return Err(unknown_org()),
        Err(error) => {
            eprintln!("devagotchi hub: organisation page: {error}");
            return Err(unavailable());
        }
    };
    match orgs.role_in(&org, &subject.id).await {
        Ok(Some(role)) => Ok((org, role)),
        Ok(None) => Err(unknown_org()),
        Err(error) => {
            eprintln!("devagotchi hub: organisation page: {error}");
            Err(unavailable())
        }
    }
}

/// Events on one of the organisation's boards in the last 7 days, when the hub exports to
/// ClickHouse and it answers; nothing otherwise.
async fn events_week(state: &AppState, subject: &Subject, name: &str) -> Option<i64> {
    let warehouse = state.hub.boards.warehouse()?;
    let hub = state.hub.clone();
    let who = subject.clone();
    let name = name.to_string();
    let (tenant, board) = tokio::task::spawn_blocking(move || {
        let (board, _, _) = hub.board_access(&who, &name).ok()?;
        let hosted = board.hosted_backend().ok()?;
        Some((hosted.tenant().to_string(), hosted.board_id().to_string()))
    })
    .await
    .ok()??;
    let stats = warehouse.board_stats(&tenant, &board, 7).await.ok()?;
    stats["events"].as_i64()
}

pub async fn show(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    let here = format!("/orgs/{}", encode(&slug));
    let subject = match signed_in(&state, &headers, &here).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let orgs = match orgs_of(&state) {
        Ok(orgs) => orgs,
        Err(response) => return response,
    };
    let (org, role) = match membership(&orgs, &slug, &subject).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let boards = state.hub.boards.clone();
    let owner = owner_of(&org.id);
    let names = match tokio::task::spawn_blocking(move || boards.account_boards(&owner)).await {
        Ok(Ok(names)) => names,
        other => {
            eprintln!("devagotchi hub: organisation page: {other:?}");
            return unavailable();
        }
    };
    let read = async {
        let visible = orgs.visible(&org, &names, &subject.id).await?;
        let shown: Vec<String> = visible.iter().map(|(name, _)| name.clone()).collect();
        let projects = orgs.projects(&org, &shown).await?;
        let members = orgs.members(&org).await?;
        let teams = orgs.teams(&org).await?;
        let invites = if role.may_configure() {
            orgs.invites(&org, &subject.id).await?
        } else {
            Vec::new()
        };
        Ok::<_, BridgeError>((projects, members, teams, invites))
    }
    .await;
    let (projects, members, teams, invites) = match read {
        Ok(read) => read,
        Err(error) => {
            eprintln!("devagotchi hub: organisation page: {error}");
            return unavailable();
        }
    };
    let site = site();
    let copy = &site.messages.orgs;
    let admin = role.may_configure();
    let base = format!("/orgs/{}", encode(&org.slug));

    // Projects.
    let mut project_rows = String::new();
    for project in &projects {
        let name = format!("{}/{}", org.slug, project.project);
        let week = events_week(&state, &subject, &name)
            .await
            .map(|n| {
                format!(
                    " <span class=\"meta\">{}</span>",
                    Ctx::new(&site)
                        .with("n", n.to_string())
                        .inline(&copy.events_week)
                )
            })
            .unwrap_or_default();
        let shown_to = match project.visibility {
            Visibility::Org => &copy.visibility_org,
            Visibility::Members => &copy.visibility_members,
        };
        let mut row = format!(
            "<li id=\"project-{p}\"><a href=\"/app/boards/{href}\">{p}</a> <span class=\"meta\">{shown}</span>{week}",
            p = escape(&project.project),
            href = encode(&name),
            shown = escape(shown_to),
        );
        if !project.members.is_empty() {
            let own: String = project
                .members
                .iter()
                .map(|m| {
                    let kind = match m.kind {
                        Principal::Subject => &copy.kind_subject,
                        Principal::Team => &copy.kind_team,
                    };
                    let remove = if admin {
                        form(
                            &format!("{base}/projects/member"),
                            &format!(
                                "{}{}{}{}",
                                hidden("project", &project.project),
                                hidden(
                                    "kind",
                                    if m.kind == Principal::Team {
                                        "team"
                                    } else {
                                        "subject"
                                    }
                                ),
                                hidden("principal", &m.principal),
                                hidden("remove", "1")
                            ),
                            &copy.remove,
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "<li>{} {} <span class=\"meta\">{}</span>{remove}</li>",
                        escape(kind),
                        escape(&m.principal),
                        escape(role_word(copy, m.role))
                    )
                })
                .collect();
            row.push_str(&format!(
                "<p class=\"meta\">{}</p><ul class=\"rows\">{own}</ul>",
                escape(&copy.project_members)
            ));
        }
        if admin {
            row.push_str(&form(
                &format!("{base}/projects/visibility"),
                &format!(
                    "{}{}",
                    hidden("project", &project.project),
                    visibility_select(copy, project.visibility)
                ),
                &copy.change,
            ));
            row.push_str(&form(
                &format!("{base}/projects/member"),
                &format!(
                    "{}<label>{} <select name=\"kind\"><option value=\"subject\">{}</option><option value=\"team\">{}</option></select></label>\
<label>{} <input name=\"principal\" required maxlength=\"256\"></label>{}",
                    hidden("project", &project.project),
                    escape(&copy.give_role),
                    escape(&copy.kind_subject),
                    escape(&copy.kind_team),
                    escape(&copy.subject),
                    role_select(copy, role.min(Role::Admin), Role::Agent, false)
                ),
                &copy.give_role,
            ));
        }
        row.push_str("</li>");
        project_rows.push_str(&row);
    }
    let projects_card = format!(
        "<section class=\"card\" id=\"projects\"><h2>{}</h2>{}{}</section>",
        escape(&copy.projects),
        if projects.is_empty() {
            format!("<p class=\"empty\">{}</p>", escape(&copy.projects_empty))
        } else {
            format!("<ul class=\"rows\">{project_rows}</ul>")
        },
        if role.is_person() {
            format!(
                "<h3>{}</h3>{}",
                escape(&copy.new_project),
                form(
                    &format!("{base}/projects"),
                    &format!(
                        "<label>{} <input name=\"project\" required maxlength=\"63\" pattern=\"[a-z0-9][a-z0-9-]{{0,62}}\" autocomplete=\"off\"></label>{}",
                        escape(&copy.project),
                        visibility_select(copy, Visibility::Org)
                    ),
                    &copy.create,
                )
            )
        } else {
            String::new()
        }
    );

    // Members.
    let member_rows: String = members
        .iter()
        .map(|m| {
            let mine = m.subject == subject.id;
            let manage = admin && (m.role != Role::Owner || role == Role::Owner);
            let mut row = format!(
                "<li id=\"member-{id}\">{id} <span class=\"meta\">{r}</span>",
                id = escape(&m.subject),
                r = escape(role_word(copy, m.role))
            );
            if manage {
                row.push_str(&form(
                    &format!("{base}/members"),
                    &format!(
                        "{}{}",
                        hidden("subject", &m.subject),
                        role_select(copy, role, m.role, role == Role::Owner)
                    ),
                    &copy.change,
                ));
            }
            if manage || mine {
                row.push_str(&form(
                    &format!("{base}/members/remove"),
                    &hidden("subject", &m.subject),
                    if mine { &copy.leave } else { &copy.remove },
                ));
            }
            row.push_str("</li>");
            row
        })
        .collect();
    let members_card = format!(
        "<section class=\"card\" id=\"members\"><h2>{}</h2><ul class=\"rows\">{member_rows}</ul>{}</section>",
        escape(&copy.members),
        if admin {
            format!(
                "<h3>{}</h3>{}",
                escape(&copy.add_member),
                form(
                    &format!("{base}/members"),
                    &format!(
                        "<label>{} <input name=\"subject\" required maxlength=\"256\" autocomplete=\"off\"></label>{}",
                        escape(&copy.subject),
                        role_select(copy, role, Role::Member, role == Role::Owner)
                    ),
                    &copy.add_member,
                )
            )
        } else {
            String::new()
        }
    );

    // Teams.
    let team_rows: String = teams
        .iter()
        .map(|t| {
            let people: String = t
                .members
                .iter()
                .map(|who| {
                    let remove = if admin {
                        form(
                            &format!("{base}/teams/member"),
                            &format!(
                                "{}{}{}",
                                hidden("team", &t.team),
                                hidden("subject", who),
                                hidden("remove", "1")
                            ),
                            &copy.remove,
                        )
                    } else {
                        String::new()
                    };
                    format!("<li>{}{remove}</li>", escape(who))
                })
                .collect();
            let manage = if admin {
                format!(
                    "{}{}",
                    form(
                        &format!("{base}/teams/member"),
                        &format!(
                            "{}<label>{} <input name=\"subject\" required maxlength=\"256\" autocomplete=\"off\"></label>",
                            hidden("team", &t.team),
                            escape(&copy.subject)
                        ),
                        &copy.add_to_team,
                    ),
                    form(
                        &format!("{base}/teams/delete"),
                        &hidden("team", &t.team),
                        &copy.delete_team,
                    )
                )
            } else {
                String::new()
            };
            format!(
                "<li id=\"team-{id}\">{name} <span class=\"meta\">{id}</span><ul class=\"rows\">{people}</ul>{manage}</li>",
                id = escape(&t.team),
                name = escape(&t.name)
            )
        })
        .collect();
    let teams_card = format!(
        "<section class=\"card\" id=\"teams\"><h2>{}</h2>{}{}</section>",
        escape(&copy.teams),
        if teams.is_empty() {
            format!("<p class=\"empty\">{}</p>", escape(&copy.teams_empty))
        } else {
            format!("<ul class=\"rows\">{team_rows}</ul>")
        },
        if admin {
            format!(
                "<h3>{}</h3>{}",
                escape(&copy.new_team),
                form(
                    &format!("{base}/teams"),
                    &format!(
                        "<label>{} <input name=\"team\" required maxlength=\"63\" pattern=\"[a-z0-9][a-z0-9-]{{0,62}}\" autocomplete=\"off\"></label>\
<label>{} <input name=\"name\" required maxlength=\"100\"></label>",
                        escape(&copy.team),
                        escape(&copy.team_name)
                    ),
                    &copy.new_team,
                )
            )
        } else {
            String::new()
        }
    );

    // Invites.
    let invites_card = if admin {
        let rows: String = invites
            .iter()
            .map(|i| {
                format!(
                    "<li id=\"invite-{id}\">{r} <span class=\"meta\">{note} {until}</span>{revoke}</li>",
                    id = escape(&i.id),
                    r = escape(role_word(copy, i.role)),
                    note = escape(i.note.as_deref().unwrap_or_default()),
                    until = Ctx::new(&site)
                        .with("at", crate::web::utc(i.expires_at.max(0) as u64))
                        .inline(&copy.expires),
                    revoke = form(
                        &format!("{base}/invites/revoke"),
                        &hidden("id", &i.id),
                        &copy.revoke
                    )
                )
            })
            .collect();
        format!(
            "<section class=\"card\" id=\"invites\"><h2>{}</h2>{}<h3>{}</h3>{}</section>",
            escape(&copy.invites),
            if invites.is_empty() {
                format!("<p class=\"empty\">{}</p>", escape(&copy.invites_empty))
            } else {
                format!("<ul class=\"rows\">{rows}</ul>")
            },
            escape(&copy.new_invite),
            form(
                &format!("{base}/invites"),
                &format!(
                    "{}<label>{} <input name=\"note\" maxlength=\"100\"></label>",
                    role_select(copy, role, Role::Member, role == Role::Owner),
                    escape(&copy.note)
                ),
                &copy.invite_button,
            )
        )
    } else {
        String::new()
    };

    let body = format!(
        "<h1>{name}</h1><p class=\"meta\" id=\"your-role\">{you} {role}</p>{flash}{projects_card}{members_card}{teams_card}{invites_card}\
<p><a href=\"/orgs\">{all}</a></p>",
        name = escape(&org.name),
        you = escape(&copy.your_role),
        role = escape(role_word(copy, role)),
        flash = flash(&site, &shown),
        all = escape(&copy.title),
    );
    page(StatusCode::OK, &org.name, &body)
}

// ---- changes --------------------------------------------------------------------------

/// Run one change of `slug` for the signed-in person: the organisation, then `change`,
/// then back to its page saying how it went.
async fn change<F, Fut>(state: &AppState, headers: &HeaderMap, slug: &str, change: F) -> Response
where
    F: FnOnce(Orgs, Org, Subject) -> Fut,
    Fut: std::future::Future<Output = Result<(), BridgeError>>,
{
    let here = format!("/orgs/{}", encode(slug));
    let subject = match changer(state, headers, &here).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let orgs = match orgs_of(state) {
        Ok(orgs) => orgs,
        Err(response) => return response,
    };
    let (org, _) = match membership(&orgs, slug, &subject).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    back(&here, change(orgs, org, subject).await)
}

fn role_of(text: &str) -> Result<Role, BridgeError> {
    Role::parse(text).ok_or_else(|| BridgeError::new("invalid", format!("{text:?} is not a role")))
}

#[derive(Deserialize)]
pub struct MemberForm {
    subject: String,
    #[serde(default)]
    role: Option<String>,
}

pub async fn set_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<MemberForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        let role = role_of(input.role.as_deref().unwrap_or("member"))?;
        orgs.set_member(&org, &by.id, input.subject.trim(), role)
            .await
    })
    .await
}

pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<MemberForm>,
) -> Response {
    let leaving = {
        // Who leaves goes to the list, since the organisation is no longer theirs to see.
        match who(&state, &headers).await {
            Who::Person(subject) => subject.id == input.subject,
            _ => false,
        }
    };
    let response = change(&state, &headers, &slug, |orgs, org, by| async move {
        orgs.remove_member(&org, &by.id, &input.subject).await
    })
    .await;
    let went = response
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|location| location.ends_with("?done=1"));
    if leaving && went {
        return see_other("/orgs?done=1", &[]);
    }
    response
}

#[derive(Deserialize)]
pub struct TeamForm {
    team: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    remove: Option<String>,
}

pub async fn create_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<TeamForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        let name = input.name.unwrap_or_else(|| input.team.clone());
        orgs.create_team(&org, &by.id, input.team.trim(), &name)
            .await
    })
    .await
}

pub async fn delete_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<TeamForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        orgs.delete_team(&org, &by.id, &input.team).await
    })
    .await
}

pub async fn team_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<TeamForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        let subject = input.subject.unwrap_or_default();
        orgs.set_team_member(
            &org,
            &by.id,
            &input.team,
            subject.trim(),
            input.remove.is_none(),
        )
        .await
    })
    .await
}

#[derive(Deserialize)]
pub struct ProjectForm {
    project: String,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    principal: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    remove: Option<String>,
}

fn visibility_of(text: Option<&str>) -> Result<Visibility, BridgeError> {
    Visibility::parse(text.unwrap_or("org"))
        .ok_or_else(|| BridgeError::new("invalid", "who sees it is org or members"))
}

pub async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<ProjectForm>,
) -> Response {
    let hub = state.hub.clone();
    change(&state, &headers, &slug, |orgs, org, by| async move {
        let visibility = visibility_of(input.visibility.as_deref())?;
        let project = input.project.trim().to_ascii_lowercase();
        hub.bind_org(&by, &org.slug, &project).await?;
        if visibility != Visibility::Org {
            orgs.set_visibility(&org, &by.id, &project, visibility)
                .await?;
        }
        Ok(())
    })
    .await
}

pub async fn set_visibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<ProjectForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        let visibility = visibility_of(input.visibility.as_deref())?;
        orgs.set_visibility(&org, &by.id, &input.project, visibility)
            .await
    })
    .await
}

pub async fn project_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<ProjectForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        let kind = Principal::parse(input.kind.as_deref().unwrap_or("subject"))
            .ok_or_else(|| BridgeError::new("invalid", "a member is an account or a team"))?;
        let principal = input.principal.unwrap_or_default();
        let at = ProjectRef {
            tenant: org.id.clone(),
            owner: owner_of(&org.id),
            project: input.project.clone(),
        };
        if input.remove.is_some() {
            orgs.remove_project_member(&at, &by.id, None, kind, principal.trim())
                .await
        } else {
            let role = role_of(input.role.as_deref().unwrap_or("agent"))?;
            orgs.set_project_member(&at, &by.id, None, kind, principal.trim(), role)
                .await
        }
    })
    .await
}

#[derive(Deserialize)]
pub struct InviteForm {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

/// An invite is made here and its link shown on this one answer, never put in a URL the
/// browser keeps: the page is not stored (`no-store`) and the link's token is after `#`.
pub async fn create_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<InviteForm>,
) -> Response {
    let here = format!("/orgs/{}", encode(&slug));
    let subject = match changer(&state, &headers, &here).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let orgs = match orgs_of(&state) {
        Ok(orgs) => orgs,
        Err(response) => return response,
    };
    let (org, _) = match membership(&orgs, &slug, &subject).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let role = match role_of(input.role.as_deref().unwrap_or("member")) {
        Ok(role) => role,
        Err(error) => return back(&here, Err(error)),
    };
    let note = input.note.filter(|note| !note.trim().is_empty());
    let invite = match orgs.invite(&org, &subject.id, role, note.as_deref()).await {
        Ok(invite) => invite,
        Err(error) => return back(&here, Err(error)),
    };
    let site = site();
    let copy = &site.messages.orgs;
    let link = format!(
        "{}/invite#{}",
        state.hub.config.public_url.trim_end_matches('/'),
        invite.token
    );
    page(
        StatusCode::OK,
        &copy.invites,
        &format!(
            "<h1>{}</h1><p>{}</p><p><input readonly id=\"invite-link\" value=\"{}\" size=\"60\"></p><p><a href=\"{here}\">{}</a></p>",
            escape(&copy.new_invite),
            escape(&copy.invite_made),
            escape(&link),
            escape(&org.name)
        ),
    )
}

pub async fn revoke_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(input): Form<InviteForm>,
) -> Response {
    change(&state, &headers, &slug, |orgs, org, by| async move {
        orgs.revoke_invite(&org, &by.id, input.id.as_deref().unwrap_or_default())
            .await
    })
    .await
}

// ---- /invite ------------------------------------------------------------------------

/// The page an invite link opens. Its token is after `#`, which no browser sends: the
/// script (`/assets/invite.js`) keeps it in this tab's session storage, takes it out of the
/// address bar, and puts it in the form, so it survives signing in first and never reaches
/// a server log.
pub async fn invite_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let site = site();
    let copy = &site.messages.orgs;
    let signed_in = matches!(who(&state, &headers).await, Who::Person(_));
    let body = if signed_in {
        format!(
            "<h1>{title}</h1><p>{text}</p><p class=\"note\" id=\"invite-missing\" hidden>{missing}</p>\
<form method=\"post\" action=\"/invite\" id=\"invite-form\"><input type=\"hidden\" name=\"token\" id=\"invite-token\">\
<button class=\"button primary\" type=\"submit\" id=\"invite-join\">{join}</button></form>",
            title = escape(&copy.join_title),
            text = escape(&copy.join_text),
            missing = escape(&copy.join_missing),
            join = escape(&copy.join_button),
        )
    } else {
        format!(
            "<h1>{title}</h1><p>{text}</p><p class=\"note\">{signed_out}</p>\
<p><a class=\"button primary\" href=\"/auth/login?return_to=%2Finvite\" id=\"invite-sign-in\">{sign_in}</a></p>",
            title = escape(&copy.join_title),
            text = escape(&copy.join_text),
            signed_out = escape(&copy.join_signed_out),
            sign_in = escape(&site.messages.sign_in.title),
        )
    };
    let mut response =
        pages::scripted(StatusCode::OK, &copy.join_title, &body, "/assets/invite.js");
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

#[derive(Deserialize)]
pub struct AcceptForm {
    token: String,
}

pub async fn accept(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<AcceptForm>,
) -> Response {
    let subject = match changer(&state, &headers, "/invite").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let orgs = match orgs_of(&state) {
        Ok(orgs) => orgs,
        Err(response) => return response,
    };
    match orgs.accept(&input.token, &subject.id).await {
        Ok((org, _)) => see_other(&format!("/orgs/{}?done=1", encode(&org.slug)), &[]),
        Err(error) => {
            let site = site();
            let copy = &site.messages.orgs;
            page(
                match error.code.as_str() {
                    "not_found" => StatusCode::NOT_FOUND,
                    _ => StatusCode::CONFLICT,
                },
                &copy.join_title,
                &format!(
                    "<h1>{}</h1><p class=\"note\" id=\"invite-refused\">{}</p>",
                    escape(&copy.join_title),
                    escape(&error.message)
                ),
            )
        }
    }
}

pub async fn script() -> Response {
    let mut response = include_str!("assets/invite.js").into_response();
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
