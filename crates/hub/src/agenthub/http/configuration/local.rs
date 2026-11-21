use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsInput {
    project: String,
    request_id: Option<String>,
    version: i64,
    settings: SettingsEdit,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindInput {
    project: String,
    request_id: Option<String>,
    version: i64,
    workflow_id: String,
    workflow_version: i64,
}

pub(super) async fn read(caller: Caller, Path(project): Path<String>) -> Reply {
    super::super::reads::read(&caller)?;
    owner(&caller, &project)?;
    caller
        .execute(
            "board_settings",
            None,
            &project,
            None,
            json!({"project":project}),
            |board, _| board.workflow_configuration(),
        )
        .await
}

pub(super) async fn settings(
    caller: Caller,
    Path(project): Path<String>,
    Body(value): Body,
) -> Reply {
    owner(&caller, &project)?;
    let input: SettingsInput = input::decode(prepare(
        value,
        Scope {
            project: Some(project.clone()),
        },
        &caller,
    )?)?;
    positive(input.version)?;
    let identity = input::encoded(&input)?;
    caller
        .execute(
            "board_settings",
            Some("edit_settings"),
            &project,
            None,
            identity,
            move |board, actor| board.edit_settings(actor, input.version, &input.settings),
        )
        .await
}

pub(super) async fn workflow(
    caller: Caller,
    Path(project): Path<String>,
    Body(value): Body,
) -> Reply {
    owner(&caller, &project)?;
    let input: WorkflowInput = input::decode(prepare(
        value,
        Scope {
            project: Some(project.clone()),
        },
        &caller,
    )?)?;
    positive(input.version)?;
    let identity = input::encoded(&input)?;
    caller
        .execute(
            "board_settings",
            Some("edit_workflow"),
            &project,
            None,
            identity,
            move |board, actor| board.edit_workflow(actor, input.version, &input.workflow),
        )
        .await
}

pub(super) async fn bind(caller: Caller, Path(project): Path<String>, Body(value): Body) -> Reply {
    owner(&caller, &project)?;
    let input: BindInput = input::decode(prepare(
        value,
        Scope {
            project: Some(project.clone()),
        },
        &caller,
    )?)?;
    positive(input.version)?;
    positive(input.workflow_version)?;
    let identity = input::encoded(&input)?;
    caller
        .execute(
            "board_settings",
            Some("bind_workflow"),
            &project,
            None,
            identity,
            move |board, actor| {
                board.bind_workflow(
                    actor,
                    input.version,
                    &input.workflow_id,
                    input.workflow_version,
                )
            },
        )
        .await
}

pub(super) async fn detach(
    caller: Caller,
    Path(project): Path<String>,
    Body(value): Body,
) -> Reply {
    owner(&caller, &project)?;
    let input: VersionInput = input::decode(prepare(
        value,
        Scope {
            project: Some(project.clone()),
        },
        &caller,
    )?)?;
    positive(input.version)?;
    let identity = input::encoded(&input)?;
    caller
        .execute(
            "board_settings",
            Some("detach_workflow"),
            &project,
            None,
            identity,
            move |board, actor| board.detach_workflow(actor, input.version),
        )
        .await
}
