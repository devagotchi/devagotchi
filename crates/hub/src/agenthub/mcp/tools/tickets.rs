//! Working a ticket: create, claim under a lease, heartbeat, update, note, hand off, release, complete, depend, and the force release a person may make.

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::super::{
    BoardError, ClaimIn, ClaimOut, CompleteIn, CreateIn, DepIn, DepsOut, ForceIn, HandoffIn,
    HeartbeatIn, NoteIn, ReleaseIn, UpdateIn, given_or, mcp_session,
};
use crate::{
    agenthub::{AnchorPatch, Anchors, DepKind, Lease, NewTicket, NextFilter, Note, Patch, Ticket},
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
    ) -> Result<Json<Ticket>, Json<BoardError>> {
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
        self.board_call(
            "board_create",
            &input.project,
            None,
            &context,
            // No repo given: the store takes the board's (`create_in`), on every face.
            move |board, actor| board.create(actor, new),
        )
        .await
    }

    #[tool(
        name = "board_claim",
        description = "Take a lease on a ticket (by id, or the best ready one for you when id is omitted). Returns the fence every later call on it needs and the heartbeat interval. Re-claiming what you hold returns the current lease. A ticket held by someone else, blocked, or over your WIP limit is refused.",
        title = "Claim a ticket",
        annotations(
            title = "Claim a ticket",
            read_only_hint = false,
            destructive_hint = false,
            // Without an id, each call claims the next ready ticket.
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_claim(
        &self,
        Parameters(input): Parameters<ClaimIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ClaimOut>, Json<BoardError>> {
        let subject = self.subject_id(&context)?;
        let project = input.project.clone();
        let id = input.id.clone();
        let wanted = id.clone();
        let result = self
            .board_call(
                "board_claim",
                &input.project,
                id.as_deref(),
                &context,
                move |board, actor| match &wanted {
                    Some(id) => board.claim(actor, id, input.ttl_ms).map(Some),
                    None => board.claim_next(
                        actor,
                        &NextFilter {
                            label: input.label,
                            states: input.states,
                            parent: input.parent,
                            ttl_ms: input.ttl_ms,
                        },
                    ),
                },
            )
            .await?;
        let Json(lease) = result;
        if let Some(lease) = &lease {
            self.boards
                .remember_fence(&subject, &project, &lease.ticket.id, lease.fence);
        }
        Ok(Json(ClaimOut {
            claimed: lease.is_some(),
            lease,
        }))
    }

    #[tool(
        name = "board_heartbeat",
        description = "Extend your lease on a ticket you hold, at least as often as board_prime's heartbeat_every_ms says; optionally record progress for watchers. A lapsed lease is refused: claim again.",
        title = "Extend a lease",
        annotations(
            title = "Extend a lease",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_heartbeat(
        &self,
        Parameters(input): Parameters<HeartbeatIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Lease>, Json<BoardError>> {
        let subject = self.subject_id(&context)?;
        let fence = self
            .boards
            .fence_for(&subject, &input.project, &input.id, input.fence);
        let id = input.id.clone();
        let result = self
            .board_call(
                "board_heartbeat",
                &input.project,
                Some(&input.id),
                &context,
                move |board, actor| board.heartbeat(actor, &id, fence?, input.progress),
            )
            .await?;
        self.boards
            .remember_fence(&subject, &input.project, &input.id, result.0.fence);
        Ok(result)
    }

    #[tool(
        name = "board_update",
        description = "Edit a ticket's title, description, priority, labels or assignee, or move an unheld ticket along a declared workflow transition (blocked, cancelled, back to ready). version must be current; active states and held tickets change only through claim / complete / handoff / release.",
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
    ) -> Result<Json<Ticket>, Json<BoardError>> {
        let patch = Patch {
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
        self.board_call(
            "board_update",
            &input.project,
            Some(&input.id),
            &context,
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
    ) -> Result<Json<Note>, Json<BoardError>> {
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
            .board_call(
                "board_note",
                &input.project,
                Some(&input.id),
                &context,
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
        if let (Ok(_), Some(to), Ok(subject)) = (&note, &to, self.board_subject(&context))
            && let Ok((board, _, _)) = self.board_access(&subject, &project)
        {
            self.wake_recipient(&project, board, to);
        }
        note
    }

    #[tool(
        name = "board_handoff",
        description = "Stop working a ticket you hold and leave it for someone else, with the note they need; `to` makes them the assignee. The ticket goes back to its queue unfinished.",
        title = "Hand off a ticket",
        annotations(
            title = "Hand off a ticket",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_handoff(
        &self,
        Parameters(input): Parameters<HandoffIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Ticket>, Json<BoardError>> {
        let subject = self.subject_id(&context)?;
        let fence = self
            .boards
            .fence_for(&subject, &input.project, &input.id, input.fence);
        let id = input.id.clone();
        let result = self
            .board_call(
                "board_handoff",
                &input.project,
                Some(&input.id),
                &context,
                move |board, actor| {
                    board.handoff(actor, &id, fence?, &input.note, input.to.as_deref())
                },
            )
            .await?;
        self.boards
            .forget_fence(&subject, &input.project, &input.id);
        Ok(result)
    }

    #[tool(
        name = "board_release",
        description = "Give a ticket you hold back to its queue without finishing it, with a reason. Nothing is recorded as done.",
        title = "Release a ticket",
        annotations(
            title = "Release a ticket",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_release(
        &self,
        Parameters(input): Parameters<ReleaseIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Ticket>, Json<BoardError>> {
        let subject = self.subject_id(&context)?;
        let fence = self
            .boards
            .fence_for(&subject, &input.project, &input.id, input.fence);
        let id = input.id.clone();
        let result = self
            .board_call(
                "board_release",
                &input.project,
                Some(&input.id),
                &context,
                move |board, actor| board.release(actor, &id, fence?, &input.reason),
            )
            .await?;
        self.boards
            .forget_fence(&subject, &input.project, &input.id);
        Ok(result)
    }

    #[tool(
        name = "board_complete",
        description = "Finish a ticket you hold with a summary for the reviewer. It moves to the next lane (review), where someone other than you must take it; a review completed moves it to done.",
        title = "Complete a ticket",
        annotations(
            title = "Complete a ticket",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_complete(
        &self,
        Parameters(input): Parameters<CompleteIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Ticket>, Json<BoardError>> {
        let subject = self.subject_id(&context)?;
        let fence = self
            .boards
            .fence_for(&subject, &input.project, &input.id, input.fence);
        let id = input.id.clone();
        let result = self
            .board_call(
                "board_complete",
                &input.project,
                Some(&input.id),
                &context,
                move |board, actor| board.complete(actor, &id, fence?, &input.summary),
            )
            .await?;
        self.boards
            .forget_fence(&subject, &input.project, &input.id);
        Ok(result)
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
    ) -> Result<Json<DepsOut>, Json<BoardError>> {
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
        self.board_call(
            "board_dep",
            &input.project,
            Some(&input.id),
            &context,
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

    #[tool(
        name = "board_force_release",
        description = "A person's act only: take a ticket away from whoever holds it, with a reason, and put it back in its queue. Agents are refused.",
        title = "Force-release a ticket",
        annotations(
            title = "Force-release a ticket",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn board_force_release(
        &self,
        Parameters(input): Parameters<ForceIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Ticket>, Json<BoardError>> {
        let id = input.id.clone();
        self.board_call(
            "board_force_release",
            &input.project,
            Some(&input.id),
            &context,
            move |board, actor| board.force_release(actor, &id, &input.reason),
        )
        .await
    }
}
