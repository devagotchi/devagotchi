//! A project of an organisation: making one, its visibility, and who works on it, by
//! membership in the organisation or shared with someone outside it.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewProject {
    project: String,
    #[serde(default)]
    visibility: Option<String>,
}

pub(super) fn visibility_from(text: &str) -> BridgeResult<Visibility> {
    Visibility::parse(text).ok_or_else(|| {
        BridgeError::new(
            "invalid",
            format!("{text:?} is not a visibility: org or members"),
        )
    })
}

pub(super) async fn create_project(
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
    audit(&state, &subject, "org_project_created", &name, &result).await;
    answer(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisibilityIn {
    visibility: String,
}

pub(super) async fn set_visibility(
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
    audit(&state, &subject, "project_visibility", &name, &result).await;
    answer(result)
}

pub(super) fn principal_from(kind: &str) -> BridgeResult<Principal> {
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
pub(super) async fn project_member(
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

pub(super) async fn set_org_project_member(
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
    audit(&state, &subject, "project_member_set", &name, &result).await;
    answer(result)
}

pub(super) async fn remove_org_project_member(
    State(state): State<AppState>,
    Extension(subject): Extension<Subject>,
    headers: HeaderMap,
    Path((slug, project, kind, principal)): Path<(String, String, String, String)>,
) -> Response {
    let name = format!("{slug}/{project}");
    let result = project_member(&state, &subject, &headers, &name, &kind, &principal, None).await;
    audit(&state, &subject, "project_member_removed", &name, &result).await;
    answer(result)
}

pub(super) async fn set_shared_member(
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
    audit(&state, &subject, "project_member_set", &project, &result).await;
    answer(result)
}

pub(super) async fn remove_shared_member(
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
    )
    .await;
    answer(result)
}

/// A board's own members, for anyone who reaches the board.
pub(super) async fn shared_members(
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
