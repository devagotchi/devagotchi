//! Account project lifecycle; the same hub/domain operations serve the browser.
use super::{Body, Caller, Failure, Reply, Scope, input, invalid, response::answer};
use crate::{
    AppState,
    agenthub::{hosted::ProjectEdit, mcp::SEAT_HEADER},
};
use axum::{
    Router,
    extract::{Path, Query},
    routing::get,
};
use devagotchi_contracts::BridgeError;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/projects", get(list).post(create))
        .route(
            "/api/v1/projects/{project}",
            get(detail).put(update).delete(remove),
        )
        .route("/api/v1/projects/archive/{generation}/events", get(history))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    retired: bool,
    after: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    #[serde(default)]
    after_seq: i64,
}
fn person(caller: &Caller) -> Result<(), Failure> {
    if caller.headers.contains_key(SEAT_HEADER) {
        return Err(
            BridgeError::new("humans_only", "a seat cannot administer account projects").into(),
        );
    }
    Ok(())
}
fn request(
    mut value: Value,
    caller: &Caller,
    id: Option<&str>,
) -> Result<(Value, Option<String>), Failure> {
    value = input::normalize(value, Scope::default(), id, &caller.headers)?;
    input::version(&mut value, &caller.headers)?;
    let key = value
        .as_object_mut()
        .unwrap()
        .remove("request_id")
        .map(|key| {
            key.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("request_id must be a string"))
        })
        .transpose()?;
    Ok((value, key))
}
fn mutation(value: (crate::agenthub::hosted::ProjectRecord, bool)) -> Reply {
    Ok(answer(
        json!({"result":value.0,"replayed":value.1}),
        value.1,
    ))
}

async fn list(caller: Caller, Query(page): Query<Page>) -> Reply {
    person(&caller)?;
    super::reads::read(&caller)?;
    let mut projects = caller
        .hub
        .owned_projects(&caller.subject, page.retired, page.after.as_deref())
        .await?;
    let next =
        (projects.len() > 100).then(|| format!("{}/{}", projects[99].id, projects[99].board_id));
    projects.truncate(100);
    Ok(answer(json!({"projects":projects,"next":next}), false))
}
async fn detail(caller: Caller, Path(project): Path<String>) -> Reply {
    person(&caller)?;
    super::reads::read(&caller)?;
    Ok(answer(
        json!(caller.hub.owned_project(&caller.subject, &project).await?),
        false,
    ))
}
async fn create(caller: Caller, Body(value): Body) -> Reply {
    save(caller, None, value).await
}
async fn update(caller: Caller, Path(project): Path<String>, Body(value): Body) -> Reply {
    save(caller, Some(project), value).await
}
async fn save(caller: Caller, id: Option<String>, value: Value) -> Reply {
    person(&caller)?;
    let (value, key) = request(value, &caller, id.as_deref())?;
    let edit: ProjectEdit = input::decode(value)?;
    if id.is_none() && edit.version != 0 || id.is_some() && edit.version <= 0 {
        return Err(invalid(
            "POST creates with version 0; PUT needs the current positive version",
        ));
    }
    mutation(
        caller
            .hub
            .save_owned_project(&caller.subject, &edit, key.as_deref())
            .await?,
    )
}
async fn remove(caller: Caller, Path(project): Path<String>) -> Reply {
    person(&caller)?;
    let (value, key) = request(json!({}), &caller, None)?;
    let version = value
        .get("version")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid("DELETE requires If-Match with the current version"))?;
    mutation(
        caller
            .hub
            .remove_owned_project(&caller.subject, &project, version, key.as_deref())
            .await?,
    )
}
async fn history(
    caller: Caller,
    Path(generation): Path<String>,
    Query(query): Query<History>,
) -> Reply {
    person(&caller)?;
    super::reads::read(&caller)?;
    let (project, events) = caller
        .hub
        .owned_project_history(&caller.subject, &generation, query.after_seq)
        .await?;
    let next = events.last().map(|event| event.seq);
    Ok(answer(
        json!({"project":project,"events":events,"next":next}),
        false,
    ))
}
