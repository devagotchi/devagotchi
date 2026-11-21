use super::*;
use crate::identity::accounts::api_keys::{NewKey, Scope as KeyScope};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Issue {
    name: String,
    scopes: Vec<KeyScope>,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    machines: Vec<String>,
    hours: u64,
}

pub(super) async fn list(
    State(state): State<AppState>,
    caller: Caller,
    Query(scope): Query<Scope>,
    Path(id): Path<String>,
) -> Reply {
    let accounts = person(&state, &caller, false)?;
    let mut keys = accounts
        .agent_keys(
            &caller.subject.id,
            target(&caller, &scope),
            &id,
            scope.after.as_deref(),
        )
        .await?;
    let next = (keys.len() > 100).then(|| keys[99].id.clone());
    keys.truncate(100);
    Ok(answer(json!({"tokens":keys,"next":next}), false))
}

pub(super) async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Query(scope): Query<Scope>,
    Path(id): Path<String>,
    Body(value): Body,
) -> Reply {
    let accounts = person(&state, &caller, true)?;
    if input::header(&caller.headers, "idempotency-key")?.is_some()
        || value.get("request_id").is_some()
    {
        return Err(invalid(
            "token issuance cannot replay a secret; omit the request key",
        ));
    }
    let issue: Issue = input::decode(value)?;
    let (info, token) = accounts
        .create_agent_key(
            &caller.subject.id,
            target(&caller, &scope),
            &NewKey {
                name: issue.name,
                scopes: issue.scopes,
                projects: issue.projects,
                machines: issue.machines,
                agent_id: Some(id),
                hours: issue.hours,
            },
        )
        .await?;
    Ok(answer(json!({"info":info,"token":token}), false))
}

pub(super) async fn revoke(
    State(state): State<AppState>,
    caller: Caller,
    Query(scope): Query<Scope>,
    Path(id): Path<String>,
) -> Reply {
    let accounts = person(&state, &caller, true)?;
    let key = input::header(&caller.headers, "idempotency-key")?;
    mutation(
        accounts
            .revoke_managed_key(&caller.subject.id, target(&caller, &scope), &id, key)
            .await?,
    )
}
