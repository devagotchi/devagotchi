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
use crate::{AppState, agenthub::mcp::SEAT_HEADER, auth::Subject, ops::audit::AuditEntry};

mod projects;
use projects::{
    create_project, remove_org_project_member, remove_shared_member, set_org_project_member,
    set_shared_member, set_visibility, shared_members,
};

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

/// Record `what` on `on` and its outcome on the audit chain. The outcome is read from
/// `result` now, so the future holds no reference to the response.
fn audit<'a>(
    state: &'a AppState,
    subject: &'a Subject,
    what: &'a str,
    on: &'a str,
    result: &BridgeResult<Response>,
) -> impl Future<Output = ()> + Send + 'a {
    let outcome = match result {
        Ok(_) => "ok".to_string(),
        Err(error) => error.code.clone(),
    };
    async move {
        let entry = AuditEntry {
            subject: &subject.id,
            via: subject.via,
            tool: what,
            broker: on,
            operation_id: "",
            outcome: &outcome,
        };
        if let Err(error) = state.hub.audit.record(&entry).await {
            eprintln!("devagotchi hub: organisations: audit: {error}");
        }
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
    audit(&state, &subject, "org_create", &input.org, &result).await;
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
    audit(&state, &subject, "org_member_set", &slug, &result).await;
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
    audit(&state, &subject, "org_member_removed", &slug, &result).await;
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
    audit(&state, &subject, "org_team_created", &slug, &result).await;
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
    audit(&state, &subject, "org_team_deleted", &slug, &result).await;
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
    audit(&state, &subject, "org_team_member_added", &slug, &result).await;
    answer(result)
}

async fn remove_team_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, team, who)): Path<(String, String, String)>,
) -> Response {
    let result = team_member(&state, &subject, &headers, &slug, &team, &who, false).await;
    audit(&state, &subject, "org_team_member_removed", &slug, &result).await;
    answer(result)
}

// ---- projects -----------------------------------------------------------------------

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
    audit(&state, &subject, "org_invite_created", &slug, &result).await;
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
    audit(&state, &subject, "org_invite_revoked", &slug, &result).await;
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
    audit(&state, &subject, "org_invite_accepted", &slug, &result).await;
    answer(result)
}
