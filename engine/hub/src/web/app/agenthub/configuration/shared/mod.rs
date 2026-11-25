//! Owns no tables. Serves account-owned shared workflow forms and versioned edits.
//! Settings: shared definitions and bindings on board configuration pages.
//! Depends on live browser identity, the board resolver and AgentHub's catalog owner.
use super::*;
use crate::agenthub::{SharedWorkflow, WorkflowConfiguration};

mod form;
pub(super) mod render;
#[cfg(test)]
mod tests;
use form::Change;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/boards/{project}/workflows", get(catalog).post(change))
        .route(
            "/app/boards/{project}/workflows/{id}",
            get(editor).post(save),
        )
}

async fn owner(
    state: &AppState,
    headers: &HeaderMap,
    project: &str,
    changing: bool,
) -> Result<(Arc<Board>, String), Response> {
    let (board, me, catalog) = access(state, headers, project, true, changing).await?;
    if !catalog {
        return Err(refused(
            StatusCode::FORBIDDEN,
            &words().messages.app.configuration.shared_authority,
        ));
    }
    Ok((board, me))
}
async fn snapshot(
    board: Arc<Board>,
) -> Result<(WorkflowConfiguration, Vec<SharedWorkflow>), Response> {
    match tokio::task::spawn_blocking(move || {
        Ok::<_, BridgeError>((board.workflow_configuration()?, board.shared_workflows()?))
    })
    .await
    {
        Ok(Ok(value)) => Ok(value),
        error => {
            eprintln!("devagotchi hub: shared workflow catalog: {error:?}");
            Err(unavailable())
        }
    }
}
fn saved_notice(shown: &Shown) -> String {
    if shown.saved.as_deref() == Some("1") {
        words().messages.app.configuration.saved.clone()
    } else {
        String::new()
    }
}
async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    let (board, _) = match owner(&state, &headers, &project, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (current, items) = match snapshot(board).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    render::catalog(
        &project,
        &current,
        &items,
        None,
        StatusCode::OK,
        &saved_notice(&shown),
    )
}
async fn change(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let (board, me) = match owner(&state, &headers, &project, true).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let draft = match Change::parse(pairs) {
        Ok(v) => v,
        Err(()) => return invalid_form(),
    };
    let (current, items) = match snapshot(board.clone()).await {
        Ok(v) => v,
        Err(_) => return render::unavailable_change(&project, &draft),
    };
    let submitted = draft.clone();
    let result = tokio::task::spawn_blocking(move || submitted.apply(&board, &me))
        .await
        .unwrap_or_else(|e| Err(BridgeError::new("runtime", e.to_string())));
    match result {
        Ok(replayed) => crate::web::app::requests::redirect(
            &format!("{}?saved=1", render::href(&project)),
            replayed,
        ),
        Err(error) => {
            let (status, message) = failure(&error);
            render::catalog(&project, &current, &items, Some(&draft), status, &message)
        }
    }
}
async fn editor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
    Query(shown): Query<Shown>,
) -> Response {
    let (board, _) = match owner(&state, &headers, &project, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (_, items) = match snapshot(board).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(item) = items.iter().find(|w| w.workflow.id == id) else {
        return refused(
            StatusCode::NOT_FOUND,
            &words().messages.app.configuration.shared_missing,
        );
    };
    let draft = WorkflowDraft {
        version: item.version,
        request_id: Some(crate::web::app::requests::request_id()),
        workflow: item.workflow.clone(),
    };
    render::editor(
        &project,
        &id,
        &draft,
        Some(&item.used_by),
        StatusCode::OK,
        &saved_notice(&shown),
    )
}
async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let (board, me) = match owner(&state, &headers, &project, true).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (mut draft, action) = match Fields::new(pairs).and_then(WorkflowDraft::parse) {
        Ok(v) => v,
        Err(()) => return invalid_form(),
    };
    if draft.workflow.id != id {
        return invalid_form();
    }
    let reader = board.clone();
    let items = match tokio::task::spawn_blocking(move || reader.shared_workflows()).await {
        Ok(Ok(v)) => v,
        error => {
            eprintln!("devagotchi hub: shared workflow read: {error:?}");
            return render::editor(
                &project,
                &id,
                &draft,
                None,
                StatusCode::SERVICE_UNAVAILABLE,
                &words().messages.app.configuration.refused,
            );
        }
    };
    let used_by = items
        .iter()
        .find(|w| w.workflow.id == id)
        .map(|item| item.used_by.as_slice());
    if action != "save" && used_by.is_none() {
        return render::editor(
            &project,
            &id,
            &draft,
            None,
            StatusCode::NOT_FOUND,
            &words().messages.app.configuration.shared_missing,
        );
    };
    if action != "save" {
        let site = words();
        let c = &site.messages.app.configuration;
        return if draft.change_rows(&action, &c.new_state).is_ok() {
            render::editor(&project, &id, &draft, used_by, StatusCode::OK, &c.draft)
        } else {
            render::editor(
                &project,
                &id,
                &draft,
                used_by,
                StatusCode::BAD_REQUEST,
                &c.invalid_form,
            )
        };
    }
    let submitted = draft.clone();
    let selected = id.clone();
    let result = tokio::task::spawn_blocking(move || {
        let requested = submitted
            .request_id
            .as_deref()
            .map(|key| {
                board.requested(
                    &me,
                    key,
                    "edit_shared_workflow",
                    &(&selected, submitted.version, &submitted.workflow),
                )
            })
            .transpose()?;
        let board = requested.as_ref().unwrap_or(&board);
        board
            .edit_shared_workflow(
                &Actor::human(&me),
                &selected,
                submitted.version,
                &submitted.workflow,
            )
            .map(|_| board.hosted().replayed())
    })
    .await
    .unwrap_or_else(|e| Err(BridgeError::new("runtime", e.to_string())));
    match result {
        Ok(replayed) => crate::web::app::requests::redirect(
            &format!("{}?saved=1", render::edit_href(&project, &id)),
            replayed,
        ),
        Err(error) => {
            let (status, message) = failure(&error);
            render::editor(&project, &id, &draft, used_by, status, &message)
        }
    }
}
