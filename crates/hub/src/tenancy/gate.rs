//! Early access and plans where a caller acts (`tenancy/access/` keeps them): a tenant
//! waiting to be let in is answered, not served, and a tenant's plan caps what it holds.
//!
//! - Waiting: every MCP tool but `whoami` answers with the site's one message (when it
//!   joined the list, that an email comes when it is in, its account page) instead of
//!   acting, and adding a machine, enrolling one or deciding on a board is refused the same.
//! - Plans hold tenants on a hosted hub (`tenancy.mode multi`), everyone but its owner. A
//!   limit a plan sets replaces the config's (`account_boards.free_limit`, the machine
//!   quota); a limit it leaves out stays the config's; an account nobody secured is also
//!   held to `tenancy.unsecured`. A hosted tenant is one account (made on first connect or
//!   first sign-in; sign-ins link to it), so its count is that account's.
//! - Reaching a limit is refused with `plan_limit`: the plan, the limit, the count and where
//!   the plans are, in the site's words.

use devagotchi_contracts::BridgeError;
use rmcp::model::CallToolResult;

use crate::{
    identity::Subject,
    mcp::Hub,
    site::{Cap, Plan},
    tenancy::access,
};

impl Hub {
    /// The account page this hub's messages link to.
    fn account_url(&self) -> String {
        format!("{}/account", self.config.public_url.trim_end_matches('/'))
    }

    /// What a waiting caller is told: the site's message, with when it joined the list.
    pub(crate) fn waiting_text(&self, subject: &Subject) -> String {
        let since = subject
            .access
            .as_ref()
            .and_then(|access| access.waiting_since)
            .map(crate::web::utc)
            .unwrap_or_default();
        self.config
            .site()
            .waiting_message(&since, &self.account_url())
    }

    /// The refusal for a caller whose tenant waits to be let in, if it does.
    pub(crate) fn waiting_refusal(&self, subject: &Subject) -> Option<BridgeError> {
        subject
            .waiting()
            .then(|| BridgeError::new("early_access", self.waiting_text(subject)))
    }

    /// What a tool call answers while the caller's tenant waits: a refusal like any other
    /// tool's, `{code: "early_access", message}`, marked an error result because nothing ran.
    /// A client checks structured content against a tool's output schema only on a result
    /// that is not an error (MCP 2025-11-25, tool results), so the refusal's own shape is
    /// read as a refusal; `message` is a plain sentence for the person.
    pub(crate) fn waiting_result(&self, subject: &Subject) -> Option<CallToolResult> {
        self.waiting_refusal(subject).map(|refused| {
            CallToolResult::structured_error(
                serde_json::json!({"code": refused.code, "message": refused.message}),
            )
        })
    }

    /// The plan `subject`'s tenant is held to: on a hosted hub, for anyone but its owner,
    /// with a tenant the hub keeps.
    pub(crate) fn held_plan(&self, subject: &Subject) -> Option<Plan> {
        if !self.config.multi_tenant() || self.config.is_owner(&subject.id) {
            return None;
        }
        let tenant = subject.tenant.as_deref()?;
        let kept = subject.access.as_ref()?;
        access::plan_for(&self.config.site(), tenant, kept)
    }

    /// What `subject`'s plan says about limit `key`, with the plan; `None` when no plan
    /// holds it.
    pub(crate) fn plan_cap(&self, subject: &Subject, key: &str) -> Option<(Plan, Cap)> {
        let plan = self.held_plan(subject)?;
        let tenant = subject.tenant.as_deref().unwrap_or("");
        let cap = match self.config.extensions.plan.limit(tenant, key) {
            Some(limit) => match limit {
                crate::extensions::Limit::Unset => Cap::Unset,
                crate::extensions::Limit::Unlimited => Cap::Unlimited,
                crate::extensions::Limit::AtMost(n) => Cap::AtMost(n),
            },
            None => plan.cap(key),
        };
        Some((plan, cap))
    }

    /// The refusal for reaching limit `key` of `plan` (at most `most`, `count` held).
    pub(crate) fn plan_limit(
        &self,
        plan: &Plan,
        key: &str,
        most: u32,
        count: usize,
    ) -> BridgeError {
        BridgeError::new(
            "plan_limit",
            self.config
                .site()
                .limit_message(plan, key, most, count, &self.config.public_url),
        )
    }
}
