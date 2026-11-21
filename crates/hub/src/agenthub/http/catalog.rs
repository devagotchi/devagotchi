//! Source catalog routes delegate all state and validation to the catalog owner.
use super::{Body, Caller, Failure, Reply, input, invalid, response::answer};
use crate::{
    AppState,
    catalog::{Catalog, Changed, Kind, WriteOptions},
    identity::accounts::api_keys::Scope,
};
use axum::{
    Extension, Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<AppState> {
    let mut result = Router::new();
    for kind in Kind::ALL {
        let path = format!("/api/v1/{}", kind.name());
        let mut router = Router::new().route(&path, get(list).post(create)).route(
            &format!("{path}/{{id}}"),
            get(detail).put(update).patch(update).delete(remove),
        );
        if kind == Kind::GrantPresets {
            router = router.route(&format!("{path}/{{id}}/default"), post(default));
        }
        result = result.merge(router.layer(Extension(kind)));
    }
    result
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<String>,
}
fn catalog(state: &AppState, caller: &Caller, write: bool) -> Result<Catalog, Failure> {
    if let Some(error) = caller.hub.waiting_refusal(&caller.subject) {
        return Err(error.into());
    }
    if caller.subject.org.is_some()
        || caller
            .headers
            .contains_key(crate::agenthub::mcp::SEAT_HEADER)
    {
        return Err(devagotchi_contracts::BridgeError::new(
            "not_granted",
            "catalogs use the signed-in account's environment",
        )
        .into());
    }
    let accounts = if write {
        super::principals::person(state, caller, true)?
    } else {
        caller.require_scope(Scope::CatalogRead)?;
        state
            .passkeys
            .as_ref()
            .map(|sign_in| sign_in.accounts().clone())
            .ok_or_else(|| invalid("account catalogs are unavailable"))?
    };
    Ok(Catalog::new(accounts))
}
fn prepared(
    mut value: Value,
    caller: &Caller,
    id: Option<&str>,
) -> Result<(String, i64, Value, Option<String>), Failure> {
    value = input::normalize(value, input::Scope::default(), id, &caller.headers)?;
    input::version(&mut value, &caller.headers)?;
    let body = value.as_object_mut().unwrap();
    let id = body
        .remove("id")
        .and_then(|id| id.as_str().map(str::to_owned))
        .ok_or_else(|| invalid("catalog id is required"))?;
    let version = body
        .remove("version")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| invalid("catalog version is required"))?;
    let key = body
        .remove("request_id")
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("request_id must be a string"))
        })
        .transpose()?;
    Ok((id, version, value, key))
}
fn changed<T: serde::Serialize>(result: Changed<T>) -> Reply {
    let replayed = result.replayed;
    Ok(answer(
        serde_json::to_value(result).map_err(|_| invalid("cannot encode catalog response"))?,
        replayed,
    ))
}
fn confirmation(fields: &mut Value) -> Result<Option<String>, Failure> {
    fields
        .as_object_mut()
        .unwrap()
        .remove("confirm_default")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .filter(|value| value.len() <= 64)
                .ok_or_else(|| invalid("confirm_default must be a preset ID string"))
        })
        .transpose()
}
async fn list(
    State(state): State<AppState>,
    caller: Caller,
    Extension(kind): Extension<Kind>,
    query: Result<Query<Page>, QueryRejection>,
) -> Reply {
    super::reads::read(&caller)?;
    let Query(page) = query.map_err(|_| invalid("catalog query accepts an after cursor"))?;
    let mut rows = catalog(&state, &caller, false)?
        .list(&caller.subject.id, kind, page.after.as_deref())
        .await?;
    let next = (rows.len() > 100).then(|| rows[99].id.clone());
    rows.truncate(100);
    Ok(answer(json!({"rows":rows,"next":next}), false))
}
async fn detail(
    State(state): State<AppState>,
    caller: Caller,
    Extension(kind): Extension<Kind>,
    Path(id): Path<String>,
) -> Reply {
    super::reads::read(&caller)?;
    Ok(answer(
        json!(
            catalog(&state, &caller, false)?
                .get(&caller.subject.id, kind, &id)
                .await?
        ),
        false,
    ))
}
async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Extension(kind): Extension<Kind>,
    Body(value): Body,
) -> Reply {
    save(state, caller, kind, None, value).await
}
async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Extension(kind): Extension<Kind>,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    save(state, caller, kind, Some(id), value).await
}
async fn save(
    state: AppState,
    caller: Caller,
    kind: Kind,
    path: Option<String>,
    value: Value,
) -> Reply {
    let catalog = catalog(&state, &caller, true)?;
    let (id, version, mut fields, key) = prepared(value, &caller, path.as_deref())?;
    let confirm = confirmation(&mut fields)?;
    if confirm.is_some() && kind != Kind::GrantPresets {
        return Err(invalid("confirm_default is only valid for grant presets"));
    }
    if path.is_none() && version != 0 || path.is_some() && version <= 0 {
        return Err(invalid(
            "POST creates at version 0; PUT/PATCH require a positive version",
        ));
    }
    changed(
        catalog
            .put(
                &caller.subject.id,
                kind,
                &id,
                version,
                &fields,
                WriteOptions {
                    request_id: key.as_deref(),
                    confirm_default: confirm.as_deref(),
                },
            )
            .await?,
    )
}
async fn remove(
    State(state): State<AppState>,
    caller: Caller,
    Extension(kind): Extension<Kind>,
    Path(id): Path<String>,
) -> Reply {
    let catalog = catalog(&state, &caller, true)?;
    let (id, version, fields, key) = prepared(json!({}), &caller, Some(&id))?;
    if !fields.as_object().unwrap().is_empty() {
        return Err(invalid("DELETE accepts only id and If-Match"));
    }
    changed(
        catalog
            .remove(&caller.subject.id, kind, &id, version, key.as_deref())
            .await?,
    )
}
async fn default(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    let catalog = catalog(&state, &caller, true)?;
    let (id, version, mut fields, key) = prepared(value, &caller, Some(&id))?;
    let confirm = confirmation(&mut fields)?;
    if !fields.as_object().unwrap().is_empty() {
        return Err(invalid(
            "default selection accepts only id, version and confirm_default",
        ));
    }
    changed(
        catalog
            .choose_default(
                &caller.subject.id,
                &id,
                version,
                WriteOptions {
                    request_id: key.as_deref(),
                    confirm_default: confirm.as_deref(),
                },
            )
            .await?,
    )
}
