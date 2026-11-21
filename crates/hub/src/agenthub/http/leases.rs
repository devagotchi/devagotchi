use super::{
    Body, Caller, Failure, Reply, Scope,
    input::{decode, encoded, normalize, ttl},
};
use crate::agenthub::{
    DepKind, NextFilter,
    mcp::{
        ClaimIn, ClaimOut, CompleteIn, DepIn, DepsOut, ForceIn, HandoffIn, HeartbeatIn, NoteIn,
        ReleaseIn,
    },
};
use axum::extract::Path;
use devagotchi_contracts::BridgeError;
use serde_json::Value;

pub(super) async fn claim_next(caller: Caller, scope: Scope, Body(body): Body) -> Reply {
    let mut body = normalize(body, scope, None, &caller.headers)?;
    ttl(&mut body)?;
    let input: ClaimIn = decode(body)?;
    if input.id.is_some() {
        return Err(super::invalid("claim-next does not accept an id"));
    }
    claim(&caller, input).await
}

async fn claim(caller: &Caller, input: ClaimIn) -> Reply {
    let identity = encoded(&input)?;
    let project = input.project.clone();
    let target = input.id.clone();
    let operation = if target.is_some() {
        "claim"
    } else {
        "claim_next"
    };
    caller
        .execute(
            "board_claim",
            Some(operation),
            &project,
            target.as_deref(),
            identity,
            move |board, actor| {
                if let Some(id) = input.id {
                    // Individual claim uses the lease record, like MCP and the CLI.
                    serde_json::to_value(board.claim(actor, &id, input.ttl_ms)?)
                        .map_err(|_| BridgeError::new("board_store", "cannot encode lease"))
                } else {
                    let lease = board.claim_next(
                        actor,
                        &NextFilter {
                            label: input.label,
                            states: input.states,
                            parent: input.parent,
                            ttl_ms: input.ttl_ms,
                        },
                    )?;
                    serde_json::to_value(ClaimOut {
                        claimed: lease.is_some(),
                        lease,
                    })
                    .map_err(|_| BridgeError::new("board_store", "cannot encode lease"))
                }
            },
        )
        .await
}

pub(super) async fn operation(
    caller: Caller,
    Path((id, operation)): Path<(String, String)>,
    scope: Scope,
    Body(body): Body,
) -> Reply {
    let mut body = normalize(body, scope, Some(&id), &caller.headers)?;
    // Source REST release permits an omitted reason; the shared typed contract uses "".
    if operation == "release" && body.get("reason").is_none() {
        body["reason"] = "".into();
    }
    match operation.as_str() {
        "claim" => {
            let mut body = body;
            ttl(&mut body)?;
            claim(&caller, decode(body)?).await
        }
        "heartbeat" => {
            let input: HeartbeatIn = decode(body)?;
            let identity = encoded(&input)?;
            caller
                .execute(
                    "board_heartbeat",
                    Some("heartbeat"),
                    &input.project,
                    Some(&id),
                    identity,
                    move |board, actor| {
                        board.heartbeat(
                            actor,
                            &input.id,
                            input.fence.unwrap_or(i64::MIN),
                            input.progress,
                        )
                    },
                )
                .await
        }
        "release" => {
            let input: ReleaseIn = decode(body)?;
            let identity = encoded(&input)?;
            caller
                .execute(
                    "board_release",
                    Some("release"),
                    &input.project,
                    Some(&id),
                    identity,
                    move |board, actor| {
                        board.release(
                            actor,
                            &input.id,
                            input.fence.unwrap_or(i64::MIN),
                            &input.reason,
                        )
                    },
                )
                .await
        }
        "complete" => {
            let input: CompleteIn = decode(body)?;
            let identity = encoded(&input)?;
            caller
                .execute(
                    "board_complete",
                    Some("complete"),
                    &input.project,
                    Some(&id),
                    identity,
                    move |board, actor| {
                        board.complete(
                            actor,
                            &input.id,
                            input.fence.unwrap_or(i64::MIN),
                            &input.summary,
                        )
                    },
                )
                .await
        }
        "handoff" => {
            let input: HandoffIn = decode(body)?;
            let identity = encoded(&input)?;
            caller
                .execute(
                    "board_handoff",
                    Some("handoff"),
                    &input.project,
                    Some(&id),
                    identity,
                    move |board, actor| {
                        board.handoff(
                            actor,
                            &input.id,
                            input.fence.unwrap_or(i64::MIN),
                            &input.note,
                            input.to.as_deref(),
                        )
                    },
                )
                .await
        }
        "force-release" => {
            let input: ForceIn = decode(body)?;
            let identity = encoded(&input)?;
            caller
                .execute(
                    "board_force_release",
                    Some("force_release"),
                    &input.project,
                    Some(&id),
                    identity,
                    move |board, actor| board.force_release(actor, &input.id, &input.reason),
                )
                .await
        }
        "notes" => note(&caller, &id, body).await,
        "deps" => {
            let input: DepIn = decode(body)?;
            let identity = encoded(&input)?;
            caller
                .execute(
                    "board_dep",
                    Some(if input.remove {
                        "remove_dep"
                    } else {
                        "add_dep"
                    }),
                    &input.project,
                    Some(&id),
                    identity,
                    move |board, actor| {
                        let kind = match input.kind.as_deref() {
                            None => DepKind::Blocks,
                            Some(kind) => DepKind::parse(kind).ok_or_else(|| {
                                BridgeError::new("invalid", "unknown dependency kind")
                            })?,
                        };
                        let deps = if input.remove {
                            board.remove_dep(actor, &input.id, &input.depends_on, kind)
                        } else {
                            board.add_dep(actor, &input.id, &input.depends_on, kind)
                        }?;
                        Ok(DepsOut { deps })
                    },
                )
                .await
        }
        _ => Err(Failure(BridgeError::new(
            "not_found",
            "unknown task operation",
        ))),
    }
}

async fn note(caller: &Caller, id: &str, body: Value) -> Reply {
    let input: NoteIn = decode(body)?;
    let identity = encoded(&input)?;
    let (project, recipient) = (input.project.clone(), input.to.clone());
    let result = caller
        .execute(
            "board_note",
            Some("add_note"),
            &project,
            Some(id),
            identity,
            move |board, actor| {
                board.add_note(
                    actor,
                    &input.id,
                    input.kind.as_deref().unwrap_or("note"),
                    &input.body,
                    input.to.as_deref(),
                    input.fence,
                )
            },
        )
        .await;
    if let (Ok(response), Some(recipient)) = (&result, recipient)
        && !response.headers().contains_key("idempotent-replayed")
        && let Ok((board, _, _)) = caller.hub.board_access(&caller.subject, &project)
    {
        caller.hub.wake_recipient(&project, board, &recipient);
    }
    result
}
