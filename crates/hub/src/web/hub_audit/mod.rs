//! Owns no tables. Serves `/admin/audit` and its change waits. Settings: none.
//! Depends on the existing hub owner guard and ops::audit's PostgreSQL chain.
use super::{
    encode,
    hub_settings_owner::required as owner,
    pages::{self, Tab, escape},
    routes::unavailable,
};
use crate::{AppState, ops::audit::HistoryFilter};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

mod render;
#[cfg(test)]
mod tests;

#[derive(Default, Deserialize)]
struct Shown {
    actor: Option<String>,
    action: Option<String>,
    machine: Option<String>,
    outcome: Option<String>,
    before: Option<u64>,
    #[serde(default)]
    verify: bool,
}

impl Shown {
    fn href(&self, before: Option<u64>, verify: bool) -> String {
        let mut query = Vec::new();
        for (key, value) in [
            ("actor", &self.actor),
            ("action", &self.action),
            ("machine", &self.machine),
            ("outcome", &self.outcome),
        ] {
            if let Some(value) = value.as_deref().filter(|v| !v.is_empty()) {
                query.push(format!("{key}={}", encode(value)));
            }
        }
        if let Some(before) = before {
            query.push(format!("before={before}"));
        }
        if verify {
            query.push("verify=true".into());
        }
        format!("/admin/audit?{}", query.join("&"))
    }

    fn filter(&self) -> HistoryFilter {
        let nonempty = |v: &Option<String>| v.clone().filter(|v| !v.is_empty());
        HistoryFilter {
            subject: nonempty(&self.actor),
            action_prefix: nonempty(&self.action),
            broker: nonempty(&self.machine),
            outcome: nonempty(&self.outcome),
            before: self.before,
            limit: 101,
        }
    }
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/audit", get(page))
        .route("/admin/audit/changes", get(changes))
}

async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> Response {
    let me = match owner(&state, &headers).await {
        Ok(me) => me,
        Err(response) => return response,
    };
    let site = pages::site();
    let copy = &site.messages.app;
    if shown.before == Some(0) {
        return pages::app_page(
            StatusCode::BAD_REQUEST,
            &copy.hub_audit.title,
            &escape(&copy.audit.invalid_cursor),
            Tab::Me,
        );
    }
    let audit = &state.hub.audit;
    let read = async {
        let head = audit.last_seq().await?;
        let events = audit.history(&shown.filter()).await?;
        let chain = if shown.verify {
            Some(audit.verify().await?)
        } else {
            None
        };
        devagotchi_contracts::BridgeResult::Ok((head, events, chain))
    }
    .await;
    match read {
        Ok((head, events, chain)) => pages::app_page(
            StatusCode::OK,
            &copy.hub_audit.title,
            &render::body(&shown, head, &events, chain.as_ref(), &site, &me.id),
            Tab::Me,
        ),
        Err(error) => {
            eprintln!("hub audit history: {error}");
            unavailable()
        }
    }
}

#[derive(Deserialize)]
struct After {
    after: u64,
}

async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<After>,
) -> Response {
    if let Err(response) = owner(&state, &headers).await {
        return response;
    }
    let mut changed = state.hub.audit.changes();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
    loop {
        changed.borrow_and_update();
        if let Err(response) = owner(&state, &headers).await {
            return response;
        }
        match state.hub.audit.after(input.after, 1).await {
            Ok(events) if !events.is_empty() => {
                return (
                    [("cache-control", "no-store")],
                    Json(serde_json::json!({"changed":true})),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("hub audit wait: {error}");
                return unavailable();
            }
        }
        if !matches!(
            tokio::time::timeout_at(deadline, changed.changed()).await,
            Ok(Ok(()))
        ) {
            return (
                [("cache-control", "no-store")],
                Json(serde_json::json!({"changed":false})),
            )
                .into_response();
        }
    }
}
