//! Project binding shared by the browser and MCP, including plan limits and audit.
use super::{Boards, Bound};
use crate::{identity::Subject, mcp::Hub, site::Cap};
use devagotchi_contracts::{BridgeError, BridgeResult};

impl Hub {
    pub(crate) async fn bind_project(
        &self,
        subject: &Subject,
        project: &str,
    ) -> BridgeResult<Bound> {
        let result = self.bind_project_inner(subject, project).await;
        let (target, outcome) = match &result {
            Ok(bound) if bound.created => ("created", "ok".to_string()),
            Ok(_) => ("", "ok".to_string()),
            Err(error) => ("", error.code.clone()),
        };
        self.record_completion(subject, "board_bind", project, target, &outcome)
            .await;
        result
    }

    async fn bind_project_inner(&self, subject: &Subject, project: &str) -> BridgeResult<Bound> {
        if let Some(error) = self.waiting_refusal(subject) {
            return Err(error);
        }
        if let Some((slug, name)) = project.split_once('/') {
            let result = self.bind_org(subject, slug, name).await;
            return result;
        }
        let shared: Vec<String> = self
            .config
            .boards_for(&subject.id)
            .iter()
            .map(|b| b.project.clone())
            .collect();
        let boards = self.boards.clone();
        let owner = Boards::owner_of(subject);
        let who: Vec<String> = std::iter::once(subject.id.clone())
            .chain(subject.org.clone())
            .collect();
        let project = project.to_string();
        // A tenant on a plan binds as many projects as the plan says (`tenancy/gate.rs`).
        let plan = self.plan_cap(subject, "projects");
        let planned = match &plan {
            Some((_, Cap::AtMost(most))) => Some(Some(*most as usize)),
            Some((_, Cap::Unlimited)) => Some(None),
            Some((_, Cap::Unset)) | None => None,
        };
        let held_by = owner.clone();
        let result = tokio::task::spawn_blocking(move || {
            let who: Vec<&str> = who.iter().map(String::as_str).collect();
            boards.bind_held(&owner, &who, &project, &shared, planned)
        })
        .await
        .unwrap_or_else(|join| {
            Err(BridgeError::new(
                "board_store",
                format!("board call did not complete: {join}"),
            ))
        });
        let result = match (result, plan) {
            (Err(error), Some((plan, Cap::AtMost(most)))) if error.code == "plan_limit" => {
                // A store that cannot count is the refusal, audited like the rest.
                self.boards
                    .account_boards(&held_by)
                    .and_then(|held| Err(self.plan_limit(&plan, "projects", most, held.len())))
            }
            (result, _) => result,
        };
        // A board made is its tenant's board milestone (`analytics::funnel`).
        if let (Ok(bound), Some(analytics), Some(tenant)) =
            (&result, &self.analytics, &subject.tenant)
            && bound.created
        {
            analytics.tenant_event(tenant, crate::ops::analytics::funnel::BOARD);
        }
        result
    }
}
