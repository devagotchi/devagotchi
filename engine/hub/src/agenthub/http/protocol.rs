//! Board-specific instructions and the existing connector catalog, without credentials.
use axum::extract::State;
use serde_json::json;

use super::{Caller, Reply, Scope, invalid};
use crate::AppState;

pub(super) async fn get(State(state): State<AppState>, caller: Caller, scope: Scope) -> Reply {
    super::reads::read(&caller)?;
    let project = scope
        .project
        .ok_or_else(|| invalid("project is required"))?;
    let url = format!("{}/mcp", state.hub.config.public_url.trim_end_matches('/'));
    let connect = json!({
        "schema": devagotchi_connectors::SCHEMA,
        "mcp_url": url,
        "connectors": devagotchi_connectors::all().iter().map(|client| client.render(&url)).collect::<Vec<_>>()
    });
    let name = project.clone();
    caller
        .execute(
            "board_prime",
            None,
            &project,
            None,
            json!({"project":project}),
            move |board, _| {
                let settings = board.settings()?;
                let beat = settings.heartbeat_every_ms(settings.lease_ttl_ms);
                Ok(json!({"project":name, "heartbeat_hint_ms":beat,
            "agents_md": instructions(&name, beat), "connect":connect}))
            },
        )
        .await
}

fn instructions(project: &str, beat: i64) -> String {
    let project = serde_json::to_string(project).expect("string is JSON");
    format!(
        "## AgentHub coordination protocol\n\
Identity comes from your authenticated credential. Use project {project} in every board tool.\n\
1. Read board_prime or board_ready, then board_claim. Do not work a task without its lease.\n\
2. Use board_heartbeat with the claim's fence. The default heartbeat hint is {beat} ms; \
follow the actual lease response's interval and expiry. A replay returns the original lease; \
it does not extend it.\n\
3. Send the held task's fence with writes, and its current version with board_update. \
On stale_fence, lease_expired or not_holder, stop and read the task again. \
On version_conflict, reconcile the new state before making a new request.\n\
4. Record progress, decisions and blockers with board_note. Use board_complete with a \
summary and verification evidence, or board_handoff with work done, next steps and gotchas.\n\
5. Give each mutation a request_id. After a timeout or lost response, repeat the exact \
original input with the same key. Changed intent needs a new key. For a bound local \
project, devagotchi board prints its request key; retry with --request-id and preserve \
the private .devagotchi/requests preparation files.\n\
6. Ask a person to resolve another agent's lease; do not force-release it.\n"
    )
}
