//! Versioned projections from existing owners; a failed section fails the whole read.
use super::*;
use crate::agenthub::StateKind;
use axum::extract::{Query, State, rejection::QueryRejection};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    project: String,
    projects_after: Option<String>,
    principals_after: Option<String>,
}

pub(super) async fn read(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<Selection>, QueryRejection>,
) -> Reply {
    super::super::reads::read(&caller)?;
    let Query(query) =
        query.map_err(|_| invalid("supply project and valid configuration cursors"))?;
    let accounts = super::super::principals::person(&state, &caller, false)?;
    owner(&caller, &query.project)?;
    // The environment policy owner checks live identity; an ordinary person's board
    // ownership does not grant environment-wide roster/policy administration.
    let policy = accounts.principal_configuration(&caller.subject.id).await?;
    let mut principals = accounts
        .principal_roster(
            &caller.subject.id,
            None,
            query.principals_after.as_deref(),
            None,
        )
        .await?;
    let mut projects = caller
        .hub
        .owned_projects(&caller.subject, false, query.projects_after.as_deref())
        .await?;
    let principal_next = (principals.len() > 100)
        .then(|| format!("{}/{}", principals[99].account, principals[99].id));
    let project_next =
        (projects.len() > 100).then(|| format!("{}/{}", projects[99].id, projects[99].board_id));
    principals.truncate(100);
    projects.truncate(100);
    caller.execute("board_settings",None,&query.project,None,json!({"project":query.project}),move |board,_| {
        let configuration=board.workflow_configuration()?;
        let workflows=board.shared_workflows()?;
        Ok(json!({"workflows":workflows,"projects":projects,"roles":policy.roles,
            "principals":principals,"policies":policy.rules,
            "settings":{"principals":policy.settings,"board":configuration},
            "state_kinds":[StateKind::Queue,StateKind::Active,StateKind::Review,StateKind::Blocked,StateKind::Done,StateKind::Attention],
            "next":{"projects":project_next,"principals":principal_next}}))
    }).await
}
