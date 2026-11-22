//! Working a ticket: create, claim under a lease, heartbeat, update, note, hand off, release, complete, depend, and the force release a person may make.

use super::super::mutations::{MutationInput, MutationOut};
use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::super::{BoardError, CreateIn, DepIn, DepsOut, NoteIn, UpdateIn, given_or, mcp_session};
use crate::{
    agenthub::{AnchorPatch, Anchors, DepKind, NewTicket, Note, Patch, Ticket},
    mcp::Hub,
};

#[tool_router(router = board_ticket_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "board_create",
        description = "Create a ticket. Work you discover while holding another ticket goes on the board with discovered_from set, never silently done. parent makes a child with a `parent.n` id. origin names the document item it is imported from: the same create again returns the ticket as it is, changed inputs are source_conflict.",
        title = "Create a ticket",
        annotations(
            title = "Create a ticket",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_create(
        &self,
        Parameters(input): Parameters<CreateIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MutationOut<Ticket>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let new = NewTicket {
            title: input.title,
            description: input.description,
            kind: input.kind,
            priority: input.priority,
            labels: input.labels,
            parent: input.parent,
            depends_on: input.depends_on,
            assignee: input.assignee,
            discovered_from: input.discovered_from,
            anchors: Anchors {
                // What the caller left empty, the hub fills from what it knows: the seat
                // of the token, this MCP session, the one machine this session routed to.
                machine: given_or(input.machine, || {
                    self.routed.lock().expect("routed lock").only()
                }),
                repo: input.repo,
                environment: input.environment,
                // The token's seat; a token without one works as the seat the board uses
                // for it (`actor_for`): the X-Devagotchi-Seat header, or "ai" for an AI
                // the hub connected itself.
                agent: given_or(input.agent, || {
                    let subject = self.subject(&context).ok()?;
                    subject
                        .agent
                        .or_else(|| self.seat(&context).ok().flatten())
                        .or_else(|| matches!(subject.via, "hub" | "key").then(|| "ai".to_string()))
                }),
                session: given_or(input.session, || mcp_session(&context)),
                fingerprint: input.fingerprint,
            },
            source: input.origin,
            ..NewTicket::default()
        };
        self.board_mutation(
            "board_create",
            "create_task",
            &input.project,
            None,
            &context,
            request,
            // No repo given: the store takes the board's (`create_in`), on every face.
            move |board, actor| board.create(actor, new),
        )
        .await
    }

    #[tool(
        name = "board_update",
        description = "Edit ticket metadata or progress at its current version. A live holder must supply its claim fence (or use this hub's remembered fence); another agent cannot edit it. Move an unheld ticket along a declared workflow transition. Held tickets change state only through lease operations.",
        title = "Update a ticket",
        annotations(
            title = "Update a ticket",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_update(
        &self,
        Parameters(input): Parameters<UpdateIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MutationOut<Ticket>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let subject = self.subject_id(&context)?;
        let fence = input.fence.or_else(|| {
            self.boards
                .fence_for(&subject, &input.project, &input.id, None)
                .ok()
        });
        let patch = Patch {
            fence,
            progress: input.progress,
            title: input.title,
            description: input.description,
            priority: input.priority,
            labels: input.labels,
            assignee: if input.clear_assignee {
                Some(None)
            } else {
                input.assignee.map(Some)
            },
            state: input.state,
            anchors: AnchorPatch {
                machine: input.machine.map(Some),
                repo: input.repo.map(Some),
                environment: input.environment.map(Some),
                agent: input.agent.map(Some),
                session: input.session.map(Some),
                fingerprint: input.fingerprint.map(Some),
            },
            ..Patch::default()
        };
        let id = input.id.clone();
        let version = input.version;
        self.board_mutation(
            "board_update",
            "update_task",
            &input.project,
            Some(&input.id),
            &context,
            request,
            move |board, actor| board.update(actor, &id, version, patch),
        )
        .await
    }

    #[tool(
        name = "board_note",
        description = "Append a note to a ticket: what you learned, a decision, a question, a review. Notes are append-only and travel with the ticket; `to` addresses one to a subject, who sees it in board_prime.",
        title = "Add a note to a ticket",
        annotations(
            title = "Add a note to a ticket",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_note(
        &self,
        Parameters(input): Parameters<NoteIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MutationOut<Note>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        // A fence is only needed while the caller holds the ticket; use the cached one
        // when the caller gave none and the hub knows one, and let the store decide.
        let subject = self.subject_id(&context)?;
        let fence = input.fence.or_else(|| {
            self.boards
                .fence_for(&subject, &input.project, &input.id, None)
                .ok()
        });
        let id = input.id.clone();
        let (to, project) = (input.to.clone(), input.project.clone());
        let note = self
            .board_mutation(
                "board_note",
                "add_note",
                &input.project,
                Some(&input.id),
                &context,
                request,
                move |board, actor| {
                    board.add_note(
                        actor,
                        &id,
                        input.kind.as_deref().unwrap_or("note"),
                        &input.body,
                        input.to.as_deref(),
                        fence,
                    )
                },
            )
            .await;
        if let (Ok(Json(result)), Some(to), Ok(subject)) =
            (&note, &to, self.board_subject(&context))
            && !result.replayed
            && let Ok((board, _, _)) = self.board_access(&subject, &project)
        {
            self.wake_recipient(&project, board, to);
        }
        note
    }

    #[tool(
        name = "board_dep",
        description = "Record (or remove) a dependency: `id` depends on `depends_on`. Only `blocks` gates readiness; parent, related, discovered-from, duplicates and supersedes are memory. Cycles are refused.",
        title = "Set a dependency",
        annotations(
            title = "Set a dependency",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_dep(
        &self,
        Parameters(input): Parameters<DepIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MutationOut<DepsOut>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        // An unknown kind is refused inside the call, so it is audited (G7).
        let kind = match input.kind.as_deref() {
            None => Ok(DepKind::Blocks),
            Some(name) => DepKind::parse(name).ok_or_else(|| {
                BridgeError::new(
                    "invalid",
                    format!("kind {name:?} is not blocks, parent, related, discovered-from, duplicates or supersedes"),
                )
            }),
        };
        let id = input.id.clone();
        self.board_mutation(
            "board_dep",
            if input.remove {
                "remove_dep"
            } else {
                "add_dep"
            },
            &input.project,
            Some(&input.id),
            &context,
            request,
            move |board, actor| {
                let kind = kind?;
                if input.remove {
                    board.remove_dep(actor, &id, &input.depends_on, kind)
                } else {
                    board.add_dep(actor, &id, &input.depends_on, kind)
                }
                .map(|deps| DepsOut { deps })
            },
        )
        .await
    }
}
