use super::*;

pub(super) async fn list(caller: Caller, scope: Scope) -> Reply {
    super::super::reads::read(&caller)?;
    let project = project(scope)?;
    owner(&caller, &project)?;
    caller
        .execute(
            "board_settings",
            None,
            &project,
            None,
            json!({"project":project}),
            |board, _| board.shared_workflows(),
        )
        .await
}

pub(super) async fn create(caller: Caller, scope: Scope, Body(value): Body) -> Reply {
    save(caller, scope, None, value).await
}

pub(super) async fn update(
    caller: Caller,
    scope: Scope,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    save(caller, scope, Some(id), value).await
}

async fn save(caller: Caller, scope: Scope, id: Option<String>, value: Value) -> Reply {
    let input: WorkflowInput = input::decode(prepare(value, scope, &caller)?)?;
    owner(&caller, &input.project)?;
    if let Some(id) = &id {
        positive(input.version)?;
        if input.workflow.id != *id {
            return Err(invalid("workflow id disagrees with the path"));
        }
    } else if input.version != 0 {
        return Err(invalid("POST creates with version 0"));
    }
    let identity = input::encoded(&input)?;
    let project = input.project.clone();
    let operation = if id.is_some() {
        "edit_shared_workflow"
    } else {
        "create_shared_workflow"
    };
    caller
        .execute(
            "board_settings",
            Some(operation),
            &project,
            None,
            identity,
            move |board, actor| {
                if let Some(id) = id {
                    board.edit_shared_workflow(actor, &id, input.version, &input.workflow)
                } else {
                    board.create_shared_workflow(actor, &input.workflow)
                }
            },
        )
        .await
}

pub(super) async fn delete(caller: Caller, scope: Scope, Path(id): Path<String>) -> Reply {
    let input: VersionInput = input::decode(prepare(json!({}), scope, &caller)?)?;
    positive(input.version)?;
    owner(&caller, &input.project)?;
    // The selected catalog id is part of the permanent request identity.
    let mut identity = input::encoded(&input)?;
    identity["id"] = id.clone().into();
    let project = input.project.clone();
    caller
        .execute(
            "board_settings",
            Some("delete_shared_workflow"),
            &project,
            None,
            identity,
            move |board, actor| board.delete_shared_workflow(actor, &id, input.version),
        )
        .await
}
