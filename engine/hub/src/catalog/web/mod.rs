//! Owner catalog editors use the same domain operations and receipts as the API.
use crate::web::app::{person, requests};
use crate::{
    AppState,
    catalog::{Catalog, FieldType, Kind, Row},
    identity::Subject,
    web::{
        encode,
        pages::{self, Tab, escape},
    },
};
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use devagotchi_contracts::BridgeError;
use serde::Deserialize;
mod form;
mod render;
use form::Draft;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/catalog", get(index))
        .route("/app/catalog/{kind}", get(list))
        .route("/app/catalog/{kind}/new", get(new).post(create))
        .route("/app/catalog/{kind}/records/{id}", get(editor).post(save))
}
fn invalid(message: &str) -> BridgeError {
    BridgeError::new("invalid", message)
}
fn kind(name: &str) -> Result<Kind, Response> {
    Kind::parse(name).ok_or_else(|| refused(&BridgeError::new("not_found", "unknown catalog")))
}
fn status(error: &BridgeError) -> StatusCode {
    match error.code.as_str() {
        "not_owner" | "not_granted" | "humans_only" | "policy_denied" => StatusCode::FORBIDDEN,
        "not_found" => StatusCode::NOT_FOUND,
        "version_conflict" | "request_conflict" | "in_use" => StatusCode::CONFLICT,
        "request_retired" => StatusCode::GONE,
        "account_store" | "runtime" | "catalog_seed" | "catalog_damaged" => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}
fn refused(error: &BridgeError) -> Response {
    let site = pages::site();
    let copy = &site.messages.app.catalog;
    pages::message(
        status(error),
        &copy.refused,
        &error.message,
        Some(("/app/catalog", &copy.back)),
    )
}
async fn access(
    state: &AppState,
    headers: &HeaderMap,
    changing: bool,
) -> Result<(Subject, Catalog), Response> {
    let subject = person(state, headers, "/app/catalog").await?;
    if subject.org.is_some()
        || headers.contains_key(crate::agenthub::mcp::SEAT_HEADER)
        || changing && !crate::web::from_this_hub(headers, &state.hub.config.public_url)
    {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    let Some(sign_in) = &state.passkeys else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    let catalog = Catalog::new(sign_in.accounts().clone());
    catalog
        .require_owner(&subject.id)
        .await
        .map_err(|e| refused(&e))?;
    Ok((subject, catalog))
}
async fn index(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = access(&state, &headers, false).await {
        return response;
    }
    render::index()
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<String>,
}
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(page): Query<Page>,
) -> Response {
    let (subject, catalog) = match access(&state, &headers, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let kind = match kind(&name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match catalog.list(&subject.id, kind, page.after.as_deref()).await {
        Ok(rows) => render::list(kind, &rows),
        Err(e) => refused(&e),
    }
}
async fn new(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if let Err(response) = access(&state, &headers, false).await {
        return response;
    }
    let kind = match kind(&name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    render::editor(kind, &Draft::new(kind), true, None)
}
async fn editor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((name, id)): Path<(String, String)>,
) -> Response {
    let (subject, catalog) = match access(&state, &headers, false).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let kind = match kind(&name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match catalog.get(&subject.id, kind, &id).await {
        Ok(row) => render::editor(kind, &Draft::from_record(kind, row), false, None),
        Err(e) => refused(&e),
    }
}
async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    change(state, headers, name, None, fields).await
}
async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((name, id)): Path<(String, String)>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    change(state, headers, name, Some(id), fields).await
}
async fn change(
    state: AppState,
    headers: HeaderMap,
    name: String,
    id: Option<String>,
    fields: Vec<(String, String)>,
) -> Response {
    let (subject, catalog) = match access(&state, &headers, true).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let kind = match kind(&name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (draft, parse_error) = Draft::submitted(kind, fields);
    let result = async {
        if let Some(error) = parse_error {
            return Err(error);
        }
        let version = draft
            .version
            .parse::<i64>()
            .map_err(|_| invalid("invalid catalog version"))?;
        if id.as_ref().is_some_and(|id| id != &draft.id)
            || id.is_none() && version != 0
            || id.is_some() && version <= 0
        {
            return Err(invalid(
                "catalog address or version disagrees with the form",
            ));
        }
        let key = Some(draft.request_id.as_str());
        match draft.action.as_str() {
            "save" => {
                let changed = catalog
                    .put(
                        &subject.id,
                        kind,
                        &draft.id,
                        version,
                        &draft.value(kind)?,
                        crate::catalog::WriteOptions {
                            request_id: key,
                            confirm_default: Some(&draft.confirm_default),
                        },
                    )
                    .await?;
                Ok(requests::redirect(
                    &render::record_url(kind, &changed.result.id),
                    changed.replayed,
                ))
            }
            "remove" if id.is_some() => {
                let changed = catalog
                    .remove(&subject.id, kind, &draft.id, version, key)
                    .await?;
                Ok(requests::redirect(
                    &format!("/app/catalog/{}", kind.name()),
                    changed.replayed,
                ))
            }
            "default" if id.is_some() && kind == Kind::GrantPresets => {
                let changed = catalog
                    .choose_default(
                        &subject.id,
                        &draft.id,
                        version,
                        crate::catalog::WriteOptions {
                            request_id: key,
                            confirm_default: Some(&draft.confirm_default),
                        },
                    )
                    .await?;
                Ok(requests::redirect(
                    &render::record_url(kind, &draft.id),
                    changed.replayed,
                ))
            }
            _ => Err(invalid("unknown catalog action")),
        }
    }
    .await;
    result.unwrap_or_else(|e| render::editor(kind, &draft, id.is_none(), Some(&e)))
}
