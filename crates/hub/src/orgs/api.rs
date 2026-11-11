//! `/api/v1/orgs/…`, `/api/v1/invites/accept` and `/api/v1/projects/…/members`: the
//! organisations of this hub over JSON, for a bearer token checked as `/mcp` checks it
//! (`require_bearer`) or a person's browser session, whose changes must come from this
//! hub's own pages (`bearer_or_session`). Reads answer any member; changes to who works
//! what are a person's:
//! a token the hub issued to an AI client (`via: hub`) or a call naming a seat
//! (`X-Devagotchi-Seat`) is refused `people_only`, and the store refuses anyone below the
//! role a change needs (`crate::orgs`). Every change lands on the organisation's own chain
//! and on the hub's audit chain.

use axum::{
    Extension, Json, Router,
    extract::{Path, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Deserialize;
use serde_json::json;

use super::{Org, Principal, ProjectRef, Role, Visibility, owner_of};
use crate::{AppState, audit::AuditEntry, auth::Subject, agenthub::mcp::SEAT_HEADER};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/orgs", get(list).post(create))
        .route("/api/v1/orgs/{org}", get(show))
        .route(
            "/api/v1/orgs/{org}/members/{subject}",
            put(set_member).delete(remove_member),
        )
        .route("/api/v1/orgs/{org}/teams", post(create_team))
        .route("/api/v1/orgs/{org}/teams/{team}", delete(delete_team))
        .route(
            "/api/v1/orgs/{org}/teams/{team}/members/{subject}",
            put(add_team_member).delete(remove_team_member),
        )
        .route("/api/v1/orgs/{org}/projects", post(create_project))
        .route("/api/v1/orgs/{org}/projects/{project}", put(set_visibility))
        .route(
            "/api/v1/orgs/{org}/projects/{project}/members/{kind}/{principal}",
            put(set_org_project_member).delete(remove_org_project_member),
        )
        .route(
            "/api/v1/orgs/{org}/invites",
            get(invites).post(create_invite),
        )
        .route("/api/v1/orgs/{org}/invites/{id}", delete(revoke_invite))
        .route("/api/v1/invites/accept", post(accept))
        .route("/api/v1/projects/{project}/members", get(shared_members))
        .route(
            "/api/v1/projects/{project}/members/{kind}/{principal}",
            put(set_shared_member).delete(remove_shared_member),
        )
}

/// Who is calling: a bearer token, checked as `/mcp` checks it, or else the person signed in
/// on this browser. A session's changes must come from this hub's own pages.
pub(crate) async fn bearer_or_session(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if !request.headers().contains_key(header::AUTHORIZATION)
        && let Some(subject) = crate::web::session_subject(&state, request.headers()).await
    {
        if request.method() != Method::GET
            && !crate::web::from_this_hub(request.headers(), &state.hub.config.public_url)
        {
            return (
                StatusCode::FORBIDDEN,
                Json(BridgeError::new(
                    "cross_site",
                    "a change made with a browser session comes from this hub's own pages",
                )),
            )
                .into_response();
        }
        request.extensions_mut().insert(subject);
        return next.run(request).await;
    }
    crate::require_bearer(State(state), request, next).await
}

fn status_of(code: &str) -> StatusCode {
    match code {
        "invalid" => StatusCode::UNPROCESSABLE_ENTITY,
        "not_found" | "unknown_board" | "not_offered" => StatusCode::NOT_FOUND,
        "not_permitted" | "people_only" | "read_only" | "not_granted" | "waiting" => {
            StatusCode::FORBIDDEN
        }
        "name_taken" | "last_owner" | "spent" | "revoked" | "expired" => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn answer(result: BridgeResult<Response>) -> Response {
    match result {
        Ok(response) => response,
        Err(error) => {
            let status = status_of(&error.code);
            if status == StatusCode::SERVICE_UNAVAILABLE {
                eprintln!("devagotchi hub: organisations: {error}");
            }
            (status, Json(error)).into_response()
        }
    }
}

fn orgs(state: &AppState) -> BridgeResult<super::Orgs> {
    state.hub.boards.orgs().cloned().ok_or_else(|| {
        BridgeError::new(
            "not_offered",
            "this hub keeps no organisations: it runs without its store",
        )
    })
}

/// The caller, when it may change who works what: a person, not an AI client's token or
/// a seat, and not waiting to be let in.
fn person(state: &AppState, subject: &Subject, headers: &HeaderMap) -> BridgeResult<()> {
    if let Some(refusal) = state.hub.waiting_refusal(subject) {
        return Err(BridgeError::new("waiting", refusal.message));
    }
    if subject.via == "hub" || headers.contains_key(SEAT_HEADER) {
        return Err(BridgeError::new(
            "people_only",
            "a person changes who works what, from their own sign-in; an AI client's token or a seat does not",
        ));
    }
    Ok(())
}

fn audit(
    state: &AppState,
    subject: &Subject,
    what: &str,
    on: &str,
    result: &BridgeResult<Response>,
) {
    let outcome = match result {
        Ok(_) => "ok".to_string(),
        Err(error) => error.code.clone(),
    };
    if let Err(error) = state.hub.audit.record(&AuditEntry {
        subject: &subject.id,
        via: subject.via,
        tool: what,
        broker: on,
        operation_id: "",
        outcome: &outcome,
    }) {
        eprintln!("devagotchi hub: organisations: audit: {error}");
    }
}

async fn org_named(orgs: &super::Orgs, slug: &str) -> BridgeResult<Org> {
    orgs.by_slug(slug).await?.ok_or_else(|| {
        BridgeError::new("not_found", format!("no organisation {slug:?} on this hub"))
    })
}

/// The organisation `slug`, when the caller belongs to it; with the caller's role there.
async fn member_of(orgs: &super::Orgs, slug: &str, subject: &Subject) -> BridgeResult<(Org, Role)> {
    let org = org_named(orgs, slug).await?;
    let role = orgs.role_in(&org, &subject.id).await?.ok_or_else(|| {
        BridgeError::new("not_found", format!("no organisation {slug:?} on this hub"))
    })?;
    Ok((org, role))
}

fn ok() -> Response {
    Json(json!({ "ok": true })).into_response()
}

/// The organisation's projects, by name, from the boards it holds.
async fn project_names(state: &AppState, org: &Org) -> BridgeResult<Vec<String>> {
    let boards = state.hub.boards.clone();
    let owner = owner_of(&org.id);
    tokio::task::spawn_blocking(move || boards.account_boards(&owner))
        .await
        .unwrap_or_else(|join| {
            Err(BridgeError::new(
                "board_store",
                format!("board call did not complete: {join}"),
            ))
        })
}

// ---- organisations ----------------------------------------------------------------

async fn list(State(state): State<AppState>, Extension(subject): Extension<Subject>) -> Response {
    answer(
        async {
            let orgs = orgs(&state)?;
            let held = orgs.of_subject(&subject.id).await?;
            Ok(Json(json!({
                "orgs": held.into_iter().map(|(org, role)| json!({
                    "org": org.slug, "name": org.name, "role": role,
                })).collect::<Vec<_>>()
            }))
            .into_response())
        }
        .await,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewOrg {
    org: String,
    name: String,
}

async fn create(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Json(input): Json<NewOrg>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let org = orgs(&state)?
            .create(&subject.id, &input.org, &input.name)
            .await?;
        Ok((
            StatusCode::CREATED,
            Json(json!({ "org": org.slug, "name": org.name, "role": Role::Owner })),
        )
            .into_response())
    }
    .await;
    audit(&state, &subject, "org_create", &input.org, &result);
    answer(result)
}

async fn show(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    Path(slug): Path<String>,
) -> Response {
    answer(
        async {
            let orgs = orgs(&state)?;
            let (org, role) = member_of(&orgs, &slug, &subject).await?;
            let names = project_names(&state, &org).await?;
            let visible: Vec<String> = orgs
                .visible(&org, &names, &subject.id)
                .await?
                .into_iter()
                .map(|(project, _)| project)
                .collect();
            Ok(Json(json!({
                "org": org.slug,
                "name": org.name,
                "role": role,
                "members": orgs.members(&org).await?,
                "teams": orgs.teams(&org).await?,
                "projects": orgs.projects(&org, &visible).await?,
            }))
            .into_response())
        }
        .await,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleIn {
    role: String,
}

fn role_from(text: &str) -> BridgeResult<Role> {
    Role::parse(text).ok_or_else(|| {
        BridgeError::new(
            "invalid",
            format!("{text:?} is not a role: owner, admin, member, agent or viewer"),
        )
    })
}

async fn set_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, who)): Path<(String, String)>,
    Json(input): Json<RoleIn>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        orgs.set_member(&org, &subject.id, &who, role_from(&input.role)?)
            .await?;
        Ok(ok())
    }
    .await;
    audit(&state, &subject, "org_member_set", &slug, &result);
    answer(result)
}

async fn remove_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, who)): Path<(String, String)>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        orgs.remove_member(&org, &subject.id, &who).await?;
        Ok(ok())
    }
    .await;
    audit(&state, &subject, "org_member_removed", &slug, &result);
    answer(result)
}

// ---- teams --------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewTeam {
    team: String,
    name: String,
}

async fn create_team(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(input): Json<NewTeam>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        orgs.create_team(&org, &subject.id, &input.team, &input.name)
            .await?;
        Ok((StatusCode::CREATED, Json(json!({ "team": input.team }))).into_response())
    }
    .await;
    audit(&state, &subject, "org_team_created", &slug, &result);
    answer(result)
}

async fn delete_team(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, team)): Path<(String, String)>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        orgs.delete_team(&org, &subject.id, &team).await?;
        Ok(ok())
    }
    .await;
    audit(&state, &subject, "org_team_deleted", &slug, &result);
    answer(result)
}

async fn team_member(
    state: &AppState,
    subject: &Subject,
    headers: &HeaderMap,
    slug: &str,
    team: &str,
    who: &str,
    on: bool,
) -> BridgeResult<Response> {
    person(state, subject, headers)?;
    let orgs = orgs(state)?;
    let org = org_named(&orgs, slug).await?;
    orgs.set_team_member(&org, &subject.id, team, who, on)
        .await?;
    Ok(ok())
}

async fn add_team_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, team, who)): Path<(String, String, String)>,
) -> Response {
    let result = team_member(&state, &subject, &headers, &slug, &team, &who, true).await;
    audit(&state, &subject, "org_team_member_added", &slug, &result);
    answer(result)
}

async fn remove_team_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, team, who)): Path<(String, String, String)>,
) -> Response {
    let result = team_member(&state, &subject, &headers, &slug, &team, &who, false).await;
    audit(&state, &subject, "org_team_member_removed", &slug, &result);
    answer(result)
}

// ---- projects -----------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewProject {
    project: String,
    #[serde(default)]
    visibility: Option<String>,
}

fn visibility_from(text: &str) -> BridgeResult<Visibility> {
    Visibility::parse(text).ok_or_else(|| {
        BridgeError::new(
            "invalid",
            format!("{text:?} is not a visibility: org or members"),
        )
    })
}

async fn create_project(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(input): Json<NewProject>,
) -> Response {
    let name = format!("{slug}/{}", input.project);
    let result = async {
        person(&state, &subject, &headers)?;
        let visibility = input
            .visibility
            .as_deref()
            .map(visibility_from)
            .transpose()?;
        let bound = state.hub.bind_org(&subject, &slug, &input.project).await?;
        if let Some(visibility) = visibility.filter(|v| *v != Visibility::Org) {
            let orgs = orgs(&state)?;
            let org = org_named(&orgs, &slug).await?;
            orgs.set_visibility(&org, &subject.id, &input.project, visibility)
                .await?;
        }
        Ok((
            if bound.created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(json!({ "project": bound.project, "created": bound.created })),
        )
            .into_response())
    }
    .await;
    audit(&state, &subject, "org_project_created", &name, &result);
    answer(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VisibilityIn {
    visibility: String,
}

async fn set_visibility(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, project)): Path<(String, String)>,
    Json(input): Json<VisibilityIn>,
) -> Response {
    let name = format!("{slug}/{project}");
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        orgs.set_visibility(
            &org,
            &subject.id,
            &project,
            visibility_from(&input.visibility)?,
        )
        .await?;
        Ok(ok())
    }
    .await;
    audit(&state, &subject, "project_visibility", &name, &result);
    answer(result)
}

fn principal_from(kind: &str) -> BridgeResult<Principal> {
    Principal::parse(kind).ok_or_else(|| {
        BridgeError::new(
            "invalid",
            format!("{kind:?} is not a member kind: subject or team"),
        )
    })
}

/// Set or remove (`role` None) one row of a project's own members, for the board `name`
/// (an organisation's `<slug>/<project>` or a configured one): the caller's role there is
/// what the store holds and, on a configured board, what the hub config grants.
async fn project_member(
    state: &AppState,
    subject: &Subject,
    headers: &HeaderMap,
    name: &str,
    kind: &str,
    principal: &str,
    role: Option<&str>,
) -> BridgeResult<Response> {
    person(state, subject, headers)?;
    let orgs = orgs(state)?;
    let kind = principal_from(kind)?;
    let role = role.map(role_from).transpose()?;
    let hub = state.hub.clone();
    let project = name.to_string();
    let who = subject.clone();
    let (at, floor): (ProjectRef, Option<Role>) = tokio::task::spawn_blocking(move || {
        let at = hub.project_ref(&project)?.ok_or_else(|| {
            BridgeError::new(
                "not_found",
                format!("no board {project:?} whose members this hub keeps"),
            )
        })?;
        // A configured board's grant counts as the caller's floor there.
        let floor = (!project.contains('/'))
            .then(|| {
                hub.board_access(&who, &project)
                    .ok()
                    .map(|(_, _, role)| role)
            })
            .flatten();
        Ok::<_, BridgeError>((at, floor))
    })
    .await
    .unwrap_or_else(|join| {
        Err(BridgeError::new(
            "board_store",
            format!("board call did not complete: {join}"),
        ))
    })?;
    match role {
        Some(role) => {
            orgs.set_project_member(&at, &subject.id, floor, kind, principal, role)
                .await?
        }
        None => {
            orgs.remove_project_member(&at, &subject.id, floor, kind, principal)
                .await?
        }
    }
    Ok(ok())
}

async fn set_org_project_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, project, kind, principal)): Path<(String, String, String, String)>,
    Json(input): Json<RoleIn>,
) -> Response {
    let name = format!("{slug}/{project}");
    let result = project_member(
        &state,
        &subject,
        &headers,
        &name,
        &kind,
        &principal,
        Some(&input.role),
    )
    .await;
    audit(&state, &subject, "project_member_set", &name, &result);
    answer(result)
}

async fn remove_org_project_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, project, kind, principal)): Path<(String, String, String, String)>,
) -> Response {
    let name = format!("{slug}/{project}");
    let result = project_member(&state, &subject, &headers, &name, &kind, &principal, None).await;
    audit(&state, &subject, "project_member_removed", &name, &result);
    answer(result)
}

async fn set_shared_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((project, kind, principal)): Path<(String, String, String)>,
    Json(input): Json<RoleIn>,
) -> Response {
    let result = project_member(
        &state,
        &subject,
        &headers,
        &project,
        &kind,
        &principal,
        Some(&input.role),
    )
    .await;
    audit(&state, &subject, "project_member_set", &project, &result);
    answer(result)
}

async fn remove_shared_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((project, kind, principal)): Path<(String, String, String)>,
) -> Response {
    let result = project_member(
        &state, &subject, &headers, &project, &kind, &principal, None,
    )
    .await;
    audit(
        &state,
        &subject,
        "project_member_removed",
        &project,
        &result,
    );
    answer(result)
}

/// A board's own members, for anyone who reaches the board.
async fn shared_members(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    Path(project): Path<String>,
) -> Response {
    answer(
        async {
            let orgs = orgs(&state)?;
            let hub = state.hub.clone();
            let name = project.clone();
            let who = subject.clone();
            let at = tokio::task::spawn_blocking(move || {
                hub.board_access(&who, &name)?;
                hub.project_ref(&name)
            })
            .await
            .unwrap_or_else(|join| {
                Err(BridgeError::new(
                    "board_store",
                    format!("board call did not complete: {join}"),
                ))
            })?
            .ok_or_else(|| {
                BridgeError::new(
                    "not_found",
                    format!("no board {project:?} whose members this hub keeps"),
                )
            })?;
            Ok(
                Json(json!({ "project": project, "members": orgs.project_members(&at).await? }))
                    .into_response(),
            )
        }
        .await,
    )
}

// ---- invites ------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewInvite {
    role: String,
    #[serde(default)]
    note: Option<String>,
}

async fn create_invite(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(input): Json<NewInvite>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        let invite = orgs
            .invite(
                &org,
                &subject.id,
                role_from(&input.role)?,
                input.note.as_deref(),
            )
            .await?;
        let base = state.hub.config.public_url.trim_end_matches('/');
        Ok((
            StatusCode::CREATED,
            Json(json!({
                "id": invite.id,
                "token": invite.token,
                "role": invite.role,
                "expires_at": invite.expires_at,
                "url": format!("{base}/invite#{}", invite.token),
            })),
        )
            .into_response())
    }
    .await;
    audit(&state, &subject, "org_invite_created", &slug, &result);
    answer(result)
}

async fn invites(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    Path(slug): Path<String>,
) -> Response {
    answer(
        async {
            let orgs = orgs(&state)?;
            let org = org_named(&orgs, &slug).await?;
            Ok(Json(json!({ "invites": orgs.invites(&org, &subject.id).await? })).into_response())
        }
        .await,
    )
}

async fn revoke_invite(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, id)): Path<(String, String)>,
) -> Response {
    let result = async {
        person(&state, &subject, &headers)?;
        let orgs = orgs(&state)?;
        let org = org_named(&orgs, &slug).await?;
        orgs.revoke_invite(&org, &subject.id, &id).await?;
        Ok(ok())
    }
    .await;
    audit(&state, &subject, "org_invite_revoked", &slug, &result);
    answer(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptIn {
    token: String,
}

async fn accept(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Json(input): Json<AcceptIn>,
) -> Response {
    let mut slug = String::new();
    let result = async {
        person(&state, &subject, &headers)?;
        let (org, role) = orgs(&state)?.accept(&input.token, &subject.id).await?;
        slug = org.slug.clone();
        Ok(Json(json!({ "org": org.slug, "name": org.name, "role": role })).into_response())
    }
    .await;
    audit(&state, &subject, "org_invite_accepted", &slug, &result);
    answer(result)
}
