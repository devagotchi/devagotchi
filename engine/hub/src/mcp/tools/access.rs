//! Source machine-access tools, using existing PostgreSQL domain owners and live identity.
use super::access_caller::AccessCaller;
use crate::{
    catalog::access::Selection,
    identity::accounts::api_keys::Scope,
    machines::access::{Access, AccessPageQuery, RequestInput, TaskRequest},
    mcp::Hub,
};
use devagotchi_contracts::{BridgeError, GrantDuration};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestAccess {
    /// Stable retry key for this exact request; preserve it after an uncertain response.
    request_id: String,
    /// Machine name returned by machines_list.
    machine: String,
    /// Explicit capability items, or a preset ID with its current version.
    selection: Selection,
    /// once, task, an offered number of hours, or until_revoked; omitted uses catalog defaults.
    duration: Option<GrantDuration>,
    /// Human-readable purpose, at least 20 characters.
    reason: String,
    /// Original task project, ticket ID and current lease fence, when task-bound.
    task: Option<TaskRequest>,
    /// Optional positive maximum of distinct operation admissions.
    uses_max: Option<u32>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WaitAccess {
    /// Request or group ID returned by request_access.
    request_id: String,
    /// Long-poll seconds, from 0 through 25; omitted returns immediately.
    #[serde(default)]
    timeout_s: u32,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Grants {
    /// Restrict results to this machine; required by machine-limited API keys.
    machine: Option<String>,
    /// Continuation cursor from the previous grants page.
    after: Option<String>,
    /// Grants per page, from 1 through 100; omitted returns at most 100.
    limit: Option<u32>,
}
#[tool_router(router = access_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "request_access",
        title = "Request machine access",
        description = "Ask a person for machine access. Select typed capability items or a versioned preset; give a reason of at least 20 characters and a stable request_id. Task access requires the original project, ticket and current fence. Returns pending requests and actual machine sandbox availability; it grants no execution authority. Then use wait_for_access.",
        annotations(
            title = "Request machine access",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn request_access(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(input): Parameters<RequestAccess>,
    ) -> Result<Json<serde_json::Map<String, Value>>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result: Result<Json<Value>, BridgeError> = async {
        let mut caller = AccessCaller::new(&context).await?;
        caller.scope(Scope::ApprovalsRequest)?;
        let project = input.task.as_ref().map(|t| t.project.clone());
        caller.request_scope(&input.machine, project.as_deref())?;
        let access = Access::new(caller.accounts.clone());
        let machine = self.stored_name(&caller.subject, &input.machine);
        let binding = access.request_machine(&caller.subject.id, &caller.actor(), &machine).await?;
        let facts = access.broker_facts(&caller.subject.id, &caller.actor(), &binding, &self.links).await?;
        let board = if let Some(project) = project.as_ref() {
            let boards = self.boards.clone(); let account = caller.subject.id.clone(); let project = project.clone();
            Some(tokio::task::spawn_blocking(move || boards.open_account(&account, &project)).await
                .map_err(|_| BridgeError::new("board_store", "task board resolution failed"))??
                .ok_or_else(|| BridgeError::new("not_found", "task project is unavailable"))?)
        } else { None };
        caller.refresh().await?;
        caller.scope(Scope::ApprovalsRequest)?;
        caller.request_scope(&input.machine, project.as_deref())?;
        let request = RequestInput { machine, selection: input.selection, duration: input.duration, reason: input.reason, task: input.task, uses_max: input.uses_max };
        let result = access.request(&caller.subject.id, &caller.actor(), board.as_ref().map(|b| b.hosted()), &binding, &facts.catalog(), &request, &input.request_id).await?;
        Ok(Json(json!({"result":result.result,"replayed":result.replayed,"machine_capabilities":facts})))
        }.await;
        let result = result.and_then(|Json(value)| {
            value.as_object().cloned().ok_or_else(|| {
                BridgeError::new("invalid_response", "access response must be an object")
            })
        });
        self.recorded(&subject, "request_access", "", "", result)
            .await
    }
    #[tool(
        name = "wait_for_access",
        title = "Wait for machine access",
        description = "Wait up to 25 seconds for your request or group. Returns each status and the actual granted terms, which may be narrower. A result is not an execution permit.",
        annotations(
            title = "Wait for machine access",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn wait_for_access(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(input): Parameters<WaitAccess>,
    ) -> Result<Json<serde_json::Map<String, Value>>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result: Result<Json<Value>, BridgeError> = async {
            if input.timeout_s > 25 {
                return Err(BridgeError::new(
                    "invalid",
                    "timeout_s must be from 0 to 25",
                ));
            }
            let mut caller = AccessCaller::new(&context).await?;
            let access = Access::new(caller.accounts.clone());
            let deadline = tokio::time::Instant::now()
                + std::time::Duration::from_secs(u64::from(input.timeout_s));
            loop {
                caller.refresh().await?;
                caller.scope(Scope::ApprovalsRequest)?;
                let value = access
                    .access_lookup(&caller.subject.id, &caller.actor(), &input.request_id)
                    .await?;
                for item in &value.items {
                    caller.history_scope(Some(&self.shown_name(&caller.subject, &item.machine)))?;
                }
                if value.status != "pending" || tokio::time::Instant::now() >= deadline {
                    return Ok(Json(json!(value)));
                }
                tokio::time::sleep_until(
                    deadline
                        .min(tokio::time::Instant::now() + std::time::Duration::from_millis(400)),
                )
                .await;
            }
        }
        .await;
        let result = result.and_then(|Json(value)| {
            value.as_object().cloned().ok_or_else(|| {
                BridgeError::new("invalid_response", "access response must be an object")
            })
        });
        self.recorded(&subject, "wait_for_access", "", "", result)
            .await
    }
    #[tool(
        name = "my_grants",
        title = "My machine grants",
        description = "Your credential-bound principal's live grants, expiry and use counts. Optional machine filter and bounded cursor pagination. Listed grants still require live checks when used.",
        annotations(
            title = "My machine grants",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn my_grants(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(input): Parameters<Grants>,
    ) -> Result<Json<serde_json::Map<String, Value>>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result: Result<Json<Value>, BridgeError> = async {
            let caller = AccessCaller::new(&context).await?;
            caller.scope(Scope::MachinesRead)?;
            caller.history_scope(input.machine.as_deref())?;
            let value = Access::new(caller.accounts.clone())
                .access_grants(
                    &caller.subject.id,
                    &caller.actor(),
                    &AccessPageQuery {
                        machine: input
                            .machine
                            .as_deref()
                            .map(|name| self.stored_name(&caller.subject, name)),
                        principal: Some(caller.actor().id.into()),
                        live: true,
                        after: input.after,
                        limit: input.limit,
                        ..Default::default()
                    },
                )
                .await?;
            Ok(Json(json!(value)))
        }
        .await;
        let result = result.and_then(|Json(value)| {
            value.as_object().cloned().ok_or_else(|| {
                BridgeError::new("invalid_response", "access response must be an object")
            })
        });
        self.recorded(&subject, "my_grants", "", "", result).await
    }
    #[tool(
        name = "capabilities_list",
        title = "List requestable capabilities",
        description = "Requestable catalog capabilities with selector schemas, versioned presets, risk explanations and conditions, allowed durations and default preset. Catalog sandbox levels are not proof that a particular broker supports them.",
        annotations(
            title = "List requestable capabilities",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn capabilities_list(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<serde_json::Map<String, Value>>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result: Result<Json<Value>, BridgeError> = async {
            let caller = AccessCaller::new(&context).await?;
            caller.scope(Scope::CatalogRead)?;
            Ok(Json(
                Access::new(caller.accounts.clone())
                    .request_options(&caller.subject.id, &caller.actor())
                    .await?,
            ))
        }
        .await;
        let result = result.and_then(|Json(value)| {
            value.as_object().cloned().ok_or_else(|| {
                BridgeError::new("invalid_response", "access response must be an object")
            })
        });
        self.recorded(&subject, "capabilities_list", "", "", result)
            .await
    }
    #[tool(
        name = "machines_list",
        title = "List accessible machines",
        description = "List your enrolled machines with connection presence and your live grants. connected means the link is present, not that a command succeeded. Optional machine filter; next_grants pages the grants.",
        annotations(
            title = "List accessible machines",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn machines_list(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(input): Parameters<Grants>,
    ) -> Result<Json<serde_json::Map<String, Value>>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result: Result<Json<Value>, BridgeError> = async {
        let caller = AccessCaller::new(&context).await?;
        caller.scope(Scope::MachinesRead)?;
        caller.history_scope(input.machine.as_deref())?;
        let machines: Vec<Value> = self.links.owned_by(&caller.subject.id).await?.into_iter()
            .filter(|m| input.machine.as_ref().is_none_or(|name| self.shown_name(&caller.subject, &m.name) == *name))
            .map(|m| json!({"name":self.shown_name(&caller.subject, &m.name), "connected":m.revoked_at.is_none() && self.links.connected(&m.name), "revoked":m.revoked_at.is_some(), "last_seen":self.links.last_seen(&m.name)})).collect();
        let grants = Access::new(caller.accounts.clone()).access_grants(&caller.subject.id, &caller.actor(), &AccessPageQuery {
            machine: input.machine.as_deref().map(|name| self.stored_name(&caller.subject,name)), principal: Some(caller.actor().id.into()), live: true,
            after: input.after, limit: input.limit, ..Default::default()
        }).await?;
        Ok(Json(json!({"machines":machines,"my_grants":grants.rows,"next_grants":grants.next})))
        }.await;
        let result = result.and_then(|Json(value)| {
            value.as_object().cloned().ok_or_else(|| {
                BridgeError::new("invalid_response", "access response must be an object")
            })
        });
        self.recorded(&subject, "machines_list", "", "", result)
            .await
    }
}
