use super::{
    Body, Caller, Reply, Scope,
    input::{decode, encoded, normalize, version},
};
use crate::agenthub::{
    AnchorPatch, Anchors, NewTicket, Patch,
    mcp::{CreateIn, UpdateIn},
};
use axum::extract::Path;

pub(super) async fn create(caller: Caller, scope: Scope, Body(body): Body) -> Reply {
    let input: CreateIn = decode(normalize(body, scope, None, &caller.headers)?)?;
    let identity = encoded(&input)?;
    let project = input.project.clone();
    let agent = input
        .agent
        .or_else(|| caller.subject.agent.clone())
        .or(caller.seat()?)
        .or_else(|| matches!(caller.subject.via, "hub" | "key").then(|| "ai".into()));
    let new = NewTicket {
        title: input.title,
        description: input.description,
        kind: input.kind,
        priority: input.priority,
        labels: input.labels,
        parent: input.parent,
        depends_on: input.depends_on,
        assignee: input.assignee,
        discovered_from: input.discovered_from,
        source: input.origin,
        anchors: Anchors {
            machine: input.machine,
            repo: input.repo,
            environment: input.environment,
            agent,
            session: input.session,
            fingerprint: input.fingerprint,
        },
        ..Default::default()
    };
    caller
        .execute(
            "board_create",
            Some("create_task"),
            &project,
            None,
            identity,
            move |board, actor| board.create(actor, new),
        )
        .await
}

pub(super) async fn update(
    caller: Caller,
    Path(id): Path<String>,
    scope: Scope,
    Body(body): Body,
) -> Reply {
    let mut body = normalize(body, scope, Some(&id), &caller.headers)?;
    version(&mut body, &caller.headers)?;
    let input: UpdateIn = decode(body)?;
    let identity = encoded(&input)?;
    let project = input.project.clone();
    let target = id.clone();
    let patch = Patch {
        fence: input.fence,
        progress: input.progress,
        title: input.title,
        description: input.description,
        priority: input.priority,
        labels: input.labels,
        assignee: if input.clear_assignee {
            Some(None)
        } else {
            input.assignee.map(Some)
        },
        state: input.state,
        anchors: AnchorPatch {
            machine: input.machine.map(Some),
            repo: input.repo.map(Some),
            environment: input.environment.map(Some),
            agent: input.agent.map(Some),
            session: input.session.map(Some),
            fingerprint: input.fingerprint.map(Some),
        },
        ..Default::default()
    };
    let result = caller
        .execute(
            "board_update",
            Some("update_task"),
            &project,
            Some(&target),
            identity,
            move |board, actor| board.update(actor, &id, input.version, patch),
        )
        .await;
    result.map_err(|error| {
        if caller.headers.contains_key("if-match") && error.0.code == "version_conflict" {
            super::Failure(devagotchi_contracts::BridgeError::new(
                "precondition_failed",
                "If-Match no longer matches the task version",
            ))
        } else {
            error
        }
    })
}
