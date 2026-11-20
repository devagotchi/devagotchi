//! A board's settings, and binding a project to an account's own board.

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::super::{BoardError, Bound, ProjectIn, SettingsIn, refuse};
use crate::mcp::Hub;

#[tool_router(router = board_settings_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "board_settings",
        description = "Name a project's repo (an https URL) and its environments (1 to 20 lowercase names) once, on its board: new tickets then carry the repo, and a ticket's environment must be one of them. With neither given, returns the board's settings. For the account that owns the board (and its AI), or a person granted on a shared board; refused as not_owner otherwise, invalid_settings with what to fix in next.",
        title = "A board's repo and environments",
        annotations(
            title = "A board's repo and environments",
            read_only_hint = false,
            // A new list of environments replaces the old one.
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_settings(
        &self,
        Parameters(input): Parameters<SettingsIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<crate::agenthub::Settings>, Json<BoardError>> {
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
    ) -> Result<Json<Bound>, Json<BoardError>> {
        let subject = self.board_subject(&context)?;
        self.bind_project(&subject, &input.project)
            .await
            .map(Json)
            .map_err(|error| refuse(error, None))
    }
}
