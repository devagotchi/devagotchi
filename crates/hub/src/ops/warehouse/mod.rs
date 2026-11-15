//! AgentHub's statistics over HTTP, from ClickHouse. The export itself is
//! `devagotchi_agenthub::clickhouse`, re-exported here as the hub has always named it.

pub use devagotchi_agenthub::clickhouse::*;

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{
    accounts::{Accounts, analytics_settings::StoredClickHouse},
    agenthub::mcp::Boards,
    sealed::Sealer,
    store::Store,
};

/// The name a ClickHouse password is sealed under (`crate::sealed`).
pub(crate) const SEALED_AS: &str = "clickhouse";

impl StoredClickHouse {
    /// As the export takes it.
    pub(crate) fn config(&self) -> ClickHouseConfig {
        ClickHouseConfig {
            url: self.url.clone(),
            database: self.database.clone(),
            user: (!self.user.is_empty()).then(|| self.user.clone()),
        }
    }
}

/// Export the boards' events to `config` from now on: checked first (ClickHouse answers and
/// lets this user make the database and table), then it replaces whatever export ran.
pub(crate) async fn start_checked(
    boards: &Boards,
    store: Store,
    config: &ClickHouseConfig,
    password: Option<String>,
) -> BridgeResult<()> {
    let warehouse = Arc::new(Warehouse::new(config, store, password)?);
    warehouse.check().await?;
    tokio::spawn(warehouse.clone().run());
    boards.set_warehouse(Some(warehouse));
    Ok(())
}

/// At the hub's start: the ClickHouse its owner set on the settings page, when the config
/// sets none. Not checked first: a ClickHouse that is down now is waited for, as the config's
/// is, and `/healthz` and the settings page say so.
pub(crate) async fn start_stored(
    boards: &Boards,
    accounts: &Accounts,
    sealer: &Sealer,
    store: Store,
) -> BridgeResult<bool> {
    let Some(stored) = accounts.clickhouse_setting().await? else {
        return Ok(false);
    };
    if !stored.enabled {
        return Ok(false);
    }
    let password = sealer.open_value(SEALED_AS, &stored.sealed_password);
    let warehouse = Arc::new(Warehouse::new(&stored.config(), store, password)?);
    tokio::spawn(warehouse.clone().run());
    boards.set_warehouse(Some(warehouse));
    Ok(true)
}

#[cfg(test)]
mod tests;

// ---- statistics over HTTP ----------------------------------------------------------

#[derive(serde::Deserialize)]
struct StatsQuery {
    #[serde(default)]
    days: Option<u32>,
}

/// `GET /api/v1/projects/{project}/stats?days=N` and
/// `GET /api/v1/orgs/{org}/projects/{project}/stats?days=N`: a board's statistics from
/// ClickHouse, for anyone who reads the board (`Hub::board_access`), over the last N days
/// (30 by default, at most 366).
pub(crate) fn routes() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/api/v1/projects/{project}/stats", get(project_stats))
        .route(
            "/api/v1/orgs/{org}/projects/{project}/stats",
            get(org_project_stats),
        )
}

async fn project_stats(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::Extension(subject): axum::Extension<crate::auth::Subject>,
    axum::extract::Path(project): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<StatsQuery>,
) -> axum::response::Response {
    stats_for(&state, &subject, project, query.days).await
}

async fn org_project_stats(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::Extension(subject): axum::Extension<crate::auth::Subject>,
    axum::extract::Path((org, project)): axum::extract::Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<StatsQuery>,
) -> axum::response::Response {
    stats_for(&state, &subject, format!("{org}/{project}"), query.days).await
}

async fn stats_for(
    state: &crate::AppState,
    subject: &crate::auth::Subject,
    project: String,
    days: Option<u32>,
) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};
    let days = days.unwrap_or(30).clamp(1, 366);
    let result: BridgeResult<Value> = async {
        let warehouse = state.hub.boards.warehouse().ok_or_else(|| {
            BridgeError::new(
                "not_offered",
                "this hub exports no statistics: it has no ClickHouse (clickhouse.url)",
            )
        })?;
        let hub = state.hub.clone();
        let who = subject.clone();
        let name = project.clone();
        let (tenant, board) = tokio::task::spawn_blocking(move || {
            let (board, _, _) = hub.board_access(&who, &name)?;
            let hosted = board.hosted_backend()?;
            Ok::<_, BridgeError>((hosted.tenant().to_string(), hosted.board_id().to_string()))
        })
        .await
        .unwrap_or_else(|join| {
            Err(BridgeError::new(
                "board_store",
                format!("board call did not complete: {join}"),
            ))
        })?;
        let mut stats = warehouse.board_stats(&tenant, &board, days).await?;
        stats["project"] = json!(project);
        Ok(stats)
    }
    .await;
    match result {
        Ok(stats) => axum::Json(stats).into_response(),
        Err(error) => {
            let status = match error.code.as_str() {
                "unknown_board" | "not_offered" => StatusCode::NOT_FOUND,
                "not_granted" | "waiting" => StatusCode::FORBIDDEN,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            };
            (status, axum::Json(error)).into_response()
        }
    }
}
