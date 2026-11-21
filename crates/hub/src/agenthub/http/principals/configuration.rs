use super::*;
use axum::routing::{post, put};
use devagotchi_agenthub::policy::{
    Rule,
    catalog::{PrincipalSettings, Role},
};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/principals/configuration", get(read))
        .route("/api/v1/roles", post(create_role))
        .route("/api/v1/roles/{id}", put(update_role).delete(delete_role))
        .route("/api/v1/policies", post(create_rule))
        .route(
            "/api/v1/policies/{id}",
            put(update_rule).delete(delete_rule),
        )
        .route("/api/v1/settings/principals", put(settings))
}

async fn read(State(state): State<AppState>, caller: Caller) -> Reply {
    let accounts = person(&state, &caller, false)?;
    Ok(answer(
        json!(accounts.principal_configuration(&caller.subject.id).await?),
        false,
    ))
}

// Typed domain mutations also own validation, current policy, CAS, audit and replay.
async fn role(state: AppState, caller: Caller, id: Option<String>, value: Value) -> Reply {
    let accounts = person(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, id.as_deref())?;
    if value
        .as_object()
        .unwrap()
        .keys()
        .any(|key| !["id", "description", "version"].contains(&key.as_str()))
    {
        return Err(invalid("role accepts id, description and version"));
    }
    let role: Role = input::decode(value)?;
    check_version(id.is_some(), role.version)?;
    mutation(
        accounts
            .put_principal_role_requested(&caller.subject.id, &role, key.as_deref())
            .await?,
    )
}

async fn rule(state: AppState, caller: Caller, id: Option<String>, value: Value) -> Reply {
    let accounts = person(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, id.as_deref())?;
    let rule: Rule = input::decode(value)?;
    check_version(id.is_some(), rule.version)?;
    mutation(
        accounts
            .put_principal_rule_requested(&caller.subject.id, &rule, key.as_deref())
            .await?,
    )
}

fn check_version(update: bool, version: i64) -> Result<(), Failure> {
    if update && version <= 0 || !update && version != 0 {
        return Err(invalid(
            "POST creates with version 0; PUT requires the current positive version",
        ));
    }
    Ok(())
}

async fn create_role(State(state): State<AppState>, caller: Caller, Body(value): Body) -> Reply {
    role(state, caller, None, value).await
}
async fn update_role(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    role(state, caller, Some(id), value).await
}
async fn create_rule(State(state): State<AppState>, caller: Caller, Body(value): Body) -> Reply {
    rule(state, caller, None, value).await
}
async fn update_rule(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    rule(state, caller, Some(id), value).await
}

async fn removal(state: AppState, caller: Caller, id: String, role: bool) -> Reply {
    let accounts = person(&state, &caller, true)?;
    let (value, key) = prepared(json!({}), &caller, None)?;
    let version = value
        .get("version")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid("DELETE requires If-Match with the current version"))?;
    if role {
        mutation(
            accounts
                .delete_principal_role_requested(&caller.subject.id, &id, version, key.as_deref())
                .await?,
        )
    } else {
        mutation(
            accounts
                .delete_principal_rule_requested(&caller.subject.id, &id, version, key.as_deref())
                .await?,
        )
    }
}
async fn delete_role(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
) -> Reply {
    removal(state, caller, id, true).await
}
async fn delete_rule(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
) -> Reply {
    removal(state, caller, id, false).await
}
async fn settings(State(state): State<AppState>, caller: Caller, Body(value): Body) -> Reply {
    let accounts = person(&state, &caller, true)?;
    let (value, key) = prepared(value, &caller, None)?;
    let settings: PrincipalSettings = input::decode(value)?;
    mutation(
        accounts
            .put_principal_settings_requested(&caller.subject.id, &settings, key.as_deref())
            .await?,
    )
}
