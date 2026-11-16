//! A board's settings, and binding a project to an account's own board.

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::super::{BoardOut, Boards, Bound, ProjectIn, SettingsIn, refuse};
use crate::{mcp::Hub, site::Cap};

#[tool_router(router = board_settings_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "board_settings",
        description = "Name a project's repo (an https URL) and its environments (1 to 20 lowercase names) once, on its board: new tickets then carry the repo, and a ticket's environment must be one of them. With neither given, returns the board's settings. For the account that owns the board (and its AI), or a person granted on a shared board; refused as not_owner otherwise, invalid_settings with what to fix in next.",
        title = "A board's repo and environments",
        annotations(
            title = "A board's repo and environments",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_settings(
        &self,
        Parameters(input): Parameters<SettingsIn>,
        context: RequestContext<RoleServer>,
    ) -> BoardOut<crate::agenthub::Settings> {
        let subject = self.board_subject(&context)?;
        // Whose settings these are: the owning account's, and its AI's, on its own board
        // (or its org's); the people the config names on a shared one. Unknown boards are
        // refused by the call itself, as every board tool refuses them.
        let owner = match self.board_access(&subject, &input.project) {
            Ok((_, _, role)) => role.may_configure(),
            Err(_) => true,
        };
        let patch = crate::agenthub::ProjectSettings {
            repo: input.repo,
            environments: input.environments,
        };
        self.board_call(
            "board_settings",
            &input.project,
            None,
            &context,
            move |board, actor| {
                if !owner {
                    return Err(BridgeError::new(
                        "not_owner",
                        "this board's settings are its owner's: the owning account, its AI, or a person granted on it",
                    ));
                }
                board.set_project(actor, &patch)
            },
        )
        .await
    }

    #[tool(
        name = "board_bind",
        description = "Make this account's own board for a project on this hub (or find it: binding again is harmless), so `project` works in every board_* tool from any of the account's machines. A free account holds one project here. Refused as not_offered on a hub that only serves configured boards, name_taken when the name is a shared board you already reach, plan_limit at the limit.",
        title = "Bind a project to this account's board",
        annotations(
            title = "Bind a project to this account's board",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_bind(
        &self,
        Parameters(input): Parameters<ProjectIn>,
        context: RequestContext<RoleServer>,
    ) -> BoardOut<Bound> {
        let subject = self.board_subject(&context)?;
        if let Some((slug, name)) = input.project.split_once('/') {
            let result = self.bind_org(&subject, slug, name).await;
            let outcome = match &result {
                Ok(bound) if bound.created => "created".to_string(),
                Ok(_) => "ok".to_string(),
                Err(error) => error.code.clone(),
            };
            if let Err(error) = self
                .audited(&subject, "board_bind", &input.project, "", &outcome)
                .await
            {
                return Err(refuse(error, None));
            }
            return result.map(Json).map_err(|error| refuse(error, None));
        }
        let shared: Vec<String> = self
            .config
            .boards_for(&subject.id)
            .iter()
            .map(|b| b.project.clone())
            .collect();
        let boards = self.boards.clone();
        let owner = Boards::owner_of(&subject);
        let who: Vec<String> = std::iter::once(subject.id.clone())
            .chain(subject.org.clone())
            .collect();
        let project = input.project.clone();
        // A tenant on a plan binds as many projects as the plan says (`tenancy/gate.rs`).
        let plan = self.plan_cap(&subject, "projects");
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
                let held = self
                    .boards
                    .account_boards(&held_by)
                    .map_err(|error| refuse(error, None))?
                    .len();
                Err(self.plan_limit(&plan, "projects", most, held))
            }
            (result, _) => result,
        };
        let outcome = match &result {
            Ok(bound) if bound.created => "created".to_string(),
            Ok(_) => "ok".to_string(),
            Err(error) => error.code.clone(),
        };
        // A board made is its tenant's board milestone (`analytics::funnel`).
        if let (Ok(bound), Some(analytics), Some(tenant)) =
            (&result, &self.analytics, &subject.tenant)
            && bound.created
        {
            analytics.tenant_event(tenant, crate::ops::analytics::funnel::BOARD);
        }
        if let Err(error) = self
            .audited(&subject, "board_bind", &input.project, "", &outcome)
            .await
        {
            return Err(refuse(error, None));
        }
        result.map(Json).map_err(|error| refuse(error, None))
    }
}
