//! Who is calling and what they reach: `whoami`, `machines`, and adding or renaming a machine.

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, service::RequestContext, tool,
    tool_router,
};

use crate::mcp::{
    hub::Hub,
        schema::{OrgRole, WhoAmI},
};

#[tool_router(router = machines_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "whoami",
        description = "The subject this hub authenticated for the current request and the brokers it may route to. Identity comes from the bearer token, never from arguments.",
        title = "Who am I",
        annotations(title = "Who am I", read_only_hint = true, open_world_hint = false)
    )]
    async fn whoami(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<WhoAmI>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let access_note = subject.waiting().then(|| self.waiting_text(&subject));
        let plan = self.held_plan(&subject).map(|plan| plan.id);
        let result = async { Ok(WhoAmI {
            access: subject.access.as_ref().map(|access| {
                if access.waiting {
                    crate::tenancy::access::WAITING
                } else {
                    crate::tenancy::access::ADMITTED
                }
                .to_string()
            }),
            waiting_since: subject
                .access
                .as_ref()
                .filter(|access| access.waiting)
                .and_then(|access| access.waiting_since),
            access_note,
            plan,
            brokers: self.machine_names(&subject).await,
            boards: {
                let hub = self.clone();
                let who = subject.clone();
                tokio::task::spawn_blocking(move || hub.board_names(&who))
                    .await
                    .unwrap_or_else(|join| {
                        Err(BridgeError::new(
                            "board_store",
                            format!("board call did not complete: {join}"),
                        ))
                    })
                    ?
            },
            orgs: match self.boards.orgs() {
                Some(orgs) => orgs
                    .of_subject(&subject.id)
                    .await
                    ?
                    .into_iter()
                    .map(|(org, role)| OrgRole {
                        org: org.slug,
                        name: org.name,
                        role,
                    })
                    .collect(),
                None => Vec::new(),
            },
            owner: self.config.is_owner(&subject.id),
            // An AI connected through the hub's own tokens never has a person's standing.
            human: subject.via != "hub" && self.config.is_human(&subject.id),
            subject: subject.id.clone(),
            via: subject.via.to_string(),
            name: subject.name.clone(),
            org: subject.org.clone(),
            provisional_until: subject.provisional_until,
            agent: subject.agent.clone(),
            claim_code: subject.claim_code.clone(),
            provisional_warning: subject.provisional_warning,
            secure_url: (subject.via == "hub" || subject.via == "passkey")
                .then_some(subject.provisional_until)
                .flatten()
                .map(|_| {
                    format!(
                        "{}/account#secure",
                        self.config.public_url.trim_end_matches('/')
                    )
                }),
            tenant: subject.tenant.clone(),
            claim_url: subject.claim_code.as_ref().map(|code| {
                format!(
                    "{}/claim?code={code}",
                    self.config.public_url.trim_end_matches('/')
                )
            }),
        }) }.await;
        self.recorded(&subject, "whoami", "", "", result).await
    }

}
