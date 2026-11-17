//! The MCP server itself: each call checked against an API key's limits and early access, run, and given the caller's inbox banner; the tool list without the offers' tools while they do not show; the server's name and instructions from the site.

use rmcp::{Json, RoleServer, ServerHandler, service::RequestContext, tool_handler};

use crate::mcp::hub::Hub;

#[tool_handler(
    router = Self::router()
)]
impl ServerHandler for Hub {
    /// Every tool call, unless the caller's tenant waits to be let in (early access): then
    /// every tool but `whoami` answers with the site's message instead of acting
    /// (`tenancy/gate.rs`), and the call is audited as `early_access`.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        // The offers' tools exist only while the site's offers show (`crate::offers`).
        if !self.offered() && crate::offers::TOOLS.contains(&request.name.as_ref()) {
            return Err(rmcp::ErrorData::invalid_params("tool not found", None));
        }
        let subject = self.subject(&context).ok();
        // An API key may do only what it was made to do.
        let limits = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| {
                parts
                    .extensions
                    .get::<crate::identity::accounts::api_keys::KeyLimits>()
            })
            .cloned();
        if let (Some(limits), Some(subject)) = (&limits, &subject)
            && let Some(why) = limits.check(&request.name, request.arguments.as_ref())
        {
            self.record(subject, &request.name, "", "", "key_denied")
                .await
                .map_err(|Json(error)| rmcp::ErrorData::internal_error(error.message, None))?;
            return Ok(
                rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(why)])
                    .into(),
            );
        }
        let seat = self.request_seat(&context);
        let delivers = request.name != "board_inbox";
        if request.name != "whoami"
            && let Some(subject) = &subject
            && let Some(answer) = self.waiting_result(subject)
        {
            self.record(subject, &request.name, "", "", "early_access")
                .await
                .map_err(|Json(error)| rmcp::ErrorData::internal_error(error.message, None))?;
            return Ok(answer.into());
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let answer = Self::router().call(call).await;
        // Notes left for this caller ride the answer to whatever it just called.
        let mut answer = answer;
        if delivers
            && let Ok(rmcp::model::CallToolResponse::Complete(result)) = &mut answer
            && let Some(subject) = &subject
            && let Some(text) = self.inbox_banner(subject, seat.as_deref()).await
        {
            result.content.push(rmcp::model::ContentBlock::text(text));
        }
        // A successful call from an AI client of a known family: what a workflow's
        // `ai_client` step waits on (`workflows.rs`).
        if let Ok(rmcp::model::CallToolResponse::Complete(result)) = &answer
            && result.is_error != Some(true)
            && let Some(subject) = &subject
            && let (Some(tenant), Some(client)) = (&subject.tenant, &subject.client)
            && let Some(workflows) = &self.workflows
        {
            workflows.client_called(tenant, client).await;
        }
        answer
    }

    /// Every tool, but the offers' while the site's offers do not show.
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        let supports_cache_hints = context
            .protocol_version()
            .is_some_and(|version| version >= rmcp::model::ProtocolVersion::V_2026_07_28);
        let offered = self.offered();
        let mut listed = rmcp::model::ListToolsResult::with_all_items(
            Self::router()
                .list_all()
                .into_iter()
                .filter(|tool| offered || !crate::offers::TOOLS.contains(&tool.name.as_ref()))
                .collect(),
        );
        listed.ttl_ms = supports_cache_hints.then_some(0);
        listed.cache_scope = supports_cache_hints.then_some(rmcp::model::CacheScope::Public);
        Ok(listed)
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        if !self.offered() && crate::offers::TOOLS.contains(&name) {
            return None;
        }
        Self::router().get(name).cloned()
    }

    /// The MCP server's name and instructions are the site's (`site.json` `mcp`), so every
    /// AI client meets the hub under the name the site gives it.
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let site = self.config.site();
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(site.mcp_info())
        .with_instructions(
            match site.offers_instructions().filter(|_| self.offered()) {
                Some(offers) => format!(
                    "{} {offers}",
                    site.mcp_instructions(&self.config.public_url)
                ),
                None => site.mcp_instructions(&self.config.public_url),
            },
        )
    }
}
