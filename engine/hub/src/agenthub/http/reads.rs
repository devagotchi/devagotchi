use super::{Caller, Reply, Scope, invalid};
use crate::agenthub::{EventFilter, Filter};
use axum::extract::{Path, Query, rejection::QueryRejection};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct List {
    project: String,
    state: Option<String>,
    holder: Option<String>,
    label: Option<String>,
    q: Option<String>,
    ready: Option<u8>,
    limit: Option<usize>,
}

pub(super) fn read(caller: &Caller) -> Result<(), super::Failure> {
    if caller.headers.contains_key("idempotency-key") {
        return Err(invalid("reads do not accept Idempotency-Key"));
    }
    Ok(())
}

pub(super) async fn list(caller: Caller, query: Result<Query<List>, QueryRejection>) -> Reply {
    read(&caller)?;
    let Query(input) = query.map_err(|_| invalid("supply project and valid task filters"))?;
    if input.ready.is_some_and(|ready| ready > 1) {
        return Err(invalid("ready must be 0 or 1"));
    }
    if input.limit.is_some_and(|limit| limit == 0 || limit > 500) {
        return Err(invalid("limit must be 1 to 500"));
    }
    let project = input.project.clone();
    caller.execute("board_list", None, &project, None, json!({"project":project}),
        move |board, actor| {
            let filter = Filter {
                state: input.state,
                holder: input.holder.map(|holder| if holder == "me" || holder == "@me" { actor.id.into() } else { holder }),
                label: input.label, text: input.q, ready: input.ready == Some(1),
                limit: Some(input.limit.unwrap_or(100)), ..Default::default()
            };
            // Read a conservative reconnect cursor first; a later cursor could skip
            // a mutation committed between reading tasks and reading the event head.
            let latest = board.events_matching(&EventFilter { descending: true, limit: Some(1), ..Default::default() })?;
            let tasks = board.list(&filter)?;
            Ok(json!({"tasks":tasks, "head_seq":latest.first().map(|event|event.seq).unwrap_or(0), "server_now":crate::login::unix_now()*1000}))
        }).await
}

pub(super) async fn detail(caller: Caller, Path(id): Path<String>, scope: Scope) -> Reply {
    read(&caller)?;
    let project = scope
        .project
        .ok_or_else(|| invalid("project is required"))?;
    let target = id.clone();
    caller
        .execute(
            "board_get",
            None,
            &project,
            Some(&target),
            json!({"project":project,"id":id}),
            move |board, _| {
                let view = board.get(&id)?;
                let events = board.events_matching(&EventFilter {
                    ticket: Some(id),
                    descending: true,
                    limit: Some(200),
                    ..Default::default()
                })?;
                Ok(
                    json!({"task":view.ticket, "notes":view.notes, "depends_on":view.deps,
                "dependents":view.dependents,"children":view.children,"provenance":view.provenance,
                "events":events, "server_now":crate::login::unix_now()*1000}),
                )
            },
        )
        .await
}
