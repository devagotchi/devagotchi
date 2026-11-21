use super::{Caller, Reply, Scope, invalid};
use crate::agenthub::EventFilter;
use axum::extract::{Query, rejection::QueryRejection};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Events {
    project: String,
    #[serde(default)]
    after_seq: i64,
    before_seq: Option<i64>,
    actor: Option<String>,
    #[serde(alias = "action_prefix")]
    action: Option<String>,
    #[serde(alias = "ticket")]
    task_id: Option<String>,
    order: Option<String>,
    limit: Option<usize>,
}

pub(super) async fn list(caller: Caller, query: Result<Query<Events>, QueryRejection>) -> Reply {
    super::reads::read(&caller)?;
    let Query(input) = query.map_err(|_| invalid("supply project and valid event filters"))?;
    if input.after_seq < 0 || input.before_seq.is_some_and(|seq| seq < 1) {
        return Err(invalid(
            "after_seq must be nonnegative and before_seq must be positive",
        ));
    }
    if input.limit.is_some_and(|limit| limit == 0 || limit > 1000) {
        return Err(invalid("limit must be 1 to 1000"));
    }
    if input
        .order
        .as_deref()
        .is_some_and(|order| !matches!(order, "asc" | "desc"))
    {
        return Err(invalid("order must be asc or desc"));
    }
    let filter = EventFilter {
        after_seq: input.after_seq,
        before_seq: input.before_seq,
        actor: input.actor,
        action_prefix: input.action,
        ticket: input.task_id,
        descending: input.order.as_deref() == Some("desc"),
        limit: Some(input.limit.unwrap_or(100)),
    };
    caller.execute("board_events", None, &input.project, None, json!({"project":input.project}),
        move |board, _| {
            let head = board.events_matching(&EventFilter { descending:true, limit:Some(1), ..Default::default() })?;
            let events = board.events_matching(&filter)?;
            Ok(json!({"events":events,"head_seq":head.first().map(|event| event.seq).unwrap_or(0)}))
        }).await
}

pub(super) async fn verify(caller: Caller, scope: Scope) -> Reply {
    super::reads::read(&caller)?;
    let project = scope
        .project
        .ok_or_else(|| invalid("project is required"))?;
    caller
        .execute(
            "board_verify",
            None,
            &project,
            None,
            json!({"project":project}),
            |board, _| board.verify_chain(),
        )
        .await
}
