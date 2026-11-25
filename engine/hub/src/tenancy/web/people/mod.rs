//! Owns no tables. Serves /admin/users in single mode. Settings: password minimum on
//! Hub settings. Depends on accounts::people's authority and the shared owner guard.
use crate::{AppState, identity::accounts::people::LocalUser};
use crate::{
    settings::web::owner::required as owner,
    web::{
        encode,
        pages::{self, Tab, escape},
        routes::{see_other, unavailable},
    },
};
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

mod render;
#[cfg(test)]
mod tests;

#[derive(Default, Deserialize)]
struct Shown {
    after: Option<String>,
    saved: Option<String>,
}
#[derive(Default, Deserialize)]
struct NewUser {
    name: String,
    email: String,
    password: String,
}
#[derive(Deserialize)]
struct Edit {
    name: String,
    version: i64,
    #[serde(default)]
    enabled: bool,
}
#[derive(Deserialize)]
struct Reset {
    password: String,
    version: i64,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list).post(create))
        .route("/admin/users/new", get(new))
        .route("/admin/users/{id}", get(detail).post(update))
        .route("/admin/users/{id}/password", axum::routing::post(reset))
}

async fn change(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::identity::Subject, Response> {
    let actor = owner(state, headers).await?;
    let origin = crate::identity::web::passkeys::origin_of(&state.hub.config.public_url);
    if origin.is_none()
        || headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != origin.as_deref()
        || headers
            .get("sec-fetch-site")
            .is_some_and(|v| v != "same-origin")
    {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(actor)
}

pub(crate) fn section() -> (String, String, String) {
    let site = pages::site();
    let c = &site.messages.app.people;
    (
        String::new(),
        format!(
            "<a class=\"chip\" href=\"#people\">{}</a>",
            escape(&c.title)
        ),
        format!(
            "<section class=\"card\" id=\"people\"><h2>{}</h2><p>{}</p><a href=\"/admin/users\">{}</a></section>",
            escape(&c.title),
            escape(&c.description),
            escape(&c.manage)
        ),
    )
}

fn error(error: &devagotchi_contracts::BridgeError) -> Response {
    let status = match error.code.as_str() {
        "not_found" | "not_available" | "not_granted" => return super::operator::hidden(),
        "version_conflict" => StatusCode::CONFLICT,
        "invalid_account" | "invalid_email" | "password_short" | "email_taken"
        | "owner_protected" | "no_password" => StatusCode::BAD_REQUEST,
        _ => {
            eprintln!("local user administration: {error}");
            return unavailable();
        }
    };
    let site = pages::site();
    let c = &site.messages.app.people;
    pages::app_page(
        status,
        &c.title,
        &format!(
            "<h1>{}</h1><p role=\"alert\">{} {}</p><a href=\"/admin/users\">{}</a>",
            escape(&c.title),
            escape(&c.refused),
            escape(&error.message),
            escape(&c.back)
        ),
        Tab::Me,
    )
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let actor = match owner(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let accounts = state
        .passkeys
        .as_ref()
        .expect("owner has accounts")
        .accounts();
    match accounts
        .local_users(&actor.id, shown.after.as_deref())
        .await
    {
        Ok(users) => render::list(&users),
        Err(e) => error(&e),
    }
}

async fn new(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(r) = owner(&state, &headers).await {
        return r;
    }
    render::new(
        &NewUser::default(),
        state.hub.config.live().password_min,
        None,
    )
}

async fn detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(shown): Query<Shown>,
) -> Response {
    let actor = match owner(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let accounts = state
        .passkeys
        .as_ref()
        .expect("owner has accounts")
        .accounts();
    match accounts.local_user(&actor.id, &id).await {
        Ok(user) => render::detail(
            &user,
            state.hub.config.live().password_min,
            shown.saved.as_deref(),
        ),
        Err(e) => error(&e),
    }
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<NewUser>,
) -> Response {
    let actor = match change(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let accounts = state
        .passkeys
        .as_ref()
        .expect("owner has accounts")
        .accounts();
    let result = accounts
        .create_local_user(&actor.id, &input.name, &input.email, &input.password)
        .await;
    let outcome = result
        .as_ref()
        .map(|_| "ok")
        .unwrap_or_else(|e| e.code.as_str());
    let target = result
        .as_ref()
        .map(|u| u.account.id.as_str())
        .unwrap_or_default();
    state
        .hub
        .record_completion(&actor, "admin.user.create", "", target, outcome)
        .await;
    match result {
        Ok(user) => see_other(
            &format!("/admin/users/{}?saved=created", encode(&user.account.id)),
            &[],
        ),
        Err(e)
            if matches!(
                e.code.as_str(),
                "invalid_account" | "invalid_email" | "password_short" | "email_taken"
            ) =>
        {
            render::new(
                &input,
                state.hub.config.live().password_min,
                Some(&e.message),
            )
        }
        Err(e) => error(&e),
    }
}

async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Edit>,
) -> Response {
    let actor = match change(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let accounts = state
        .passkeys
        .as_ref()
        .expect("owner has accounts")
        .accounts();
    let result = accounts
        .update_local_user(&actor.id, &id, input.version, &input.name, input.enabled)
        .await;
    state
        .hub
        .record_completion(
            &actor,
            "admin.user.update",
            "",
            &id,
            result
                .as_ref()
                .map(|_| "ok")
                .unwrap_or_else(|e| e.code.as_str()),
        )
        .await;
    match result {
        Ok(_) => see_other(&format!("/admin/users/{}?saved=updated", encode(&id)), &[]),
        Err(e) => error(&e),
    }
}

async fn reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Reset>,
) -> Response {
    let actor = match change(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let accounts = state
        .passkeys
        .as_ref()
        .expect("owner has accounts")
        .accounts();
    let result = accounts
        .reset_local_password(&actor.id, &id, input.version, &input.password)
        .await;
    state
        .hub
        .record_completion(
            &actor,
            "admin.user.password_reset",
            "",
            &id,
            result
                .as_ref()
                .map(|_| "ok")
                .unwrap_or_else(|e| e.code.as_str()),
        )
        .await;
    match result {
        Ok(_) if id == actor.id => see_other("/auth/login?return_to=%2Fadmin%2Fusers", &[]),
        Ok(_) => see_other(&format!("/admin/users/{}?saved=reset", encode(&id)), &[]),
        Err(e) => error(&e),
    }
}
