//! Claim and lease mutations; replay flags never refresh the client's fence memory.
use super::super::mutations::{MutationInput, MutationOut};
use super::super::{
    BoardError, ClaimIn, ClaimOut, CompleteIn, ForceIn, HandoffIn, HeartbeatIn, ReleaseIn,
};
use crate::{
    agenthub::{Lease, NextFilter, Ticket},
    mcp::Hub,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

#[tool_router(router = board_lease_router, vis = "pub(super)")]
impl Hub {
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
    ) -> Result<Json<MutationOut<ClaimOut>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let subject = self.subject_id(&context)?;
        let project = input.project.clone();
        let id = input.id.clone();
        let wanted = id.clone();
        let result = self
            .board_mutation(
                "board_claim",
                if id.is_some() { "claim" } else { "claim_next" },
                &input.project,
                id.as_deref(),
                &context,
                request,
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
        let Json(result) = result;
        if !result.replayed
            && let Some(lease) = &result.result
        {
            self.boards
                .remember_fence(&subject, &project, &lease.ticket.id, lease.fence);
        }
        Ok(Json(result.map(|lease| ClaimOut {
            claimed: lease.is_some(),
            lease,
        })))
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
    ) -> Result<Json<MutationOut<Lease>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let subject = self.subject_id(&context)?;
        let fence =
            request.fence(
                self.boards
                    .fence_for(&subject, &input.project, &input.id, input.fence),
            );
        let id = input.id.clone();
        let result = self
            .board_mutation(
                "board_heartbeat",
                "heartbeat",
                &input.project,
                Some(&input.id),
                &context,
                request,
                move |board, actor| board.heartbeat(actor, &id, fence?, input.progress),
            )
            .await?;
        if !result.0.replayed {
            self.boards
                .remember_fence(&subject, &input.project, &input.id, result.0.result.fence);
        }
        Ok(result)
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
    ) -> Result<Json<MutationOut<Ticket>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let subject = self.subject_id(&context)?;
        let fence =
            request.fence(
                self.boards
                    .fence_for(&subject, &input.project, &input.id, input.fence),
            );
        let id = input.id.clone();
        let result = self
            .board_mutation(
                "board_handoff",
                "handoff",
                &input.project,
                Some(&input.id),
                &context,
                request,
                move |board, actor| {
                    board.handoff(actor, &id, fence?, &input.note, input.to.as_deref())
                },
            )
            .await?;
        if !result.0.replayed {
            self.boards
                .forget_fence(&subject, &input.project, &input.id);
        }
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
    ) -> Result<Json<MutationOut<Ticket>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let subject = self.subject_id(&context)?;
        let fence =
            request.fence(
                self.boards
                    .fence_for(&subject, &input.project, &input.id, input.fence),
            );
        let id = input.id.clone();
        let result = self
            .board_mutation(
                "board_release",
                "release",
                &input.project,
                Some(&input.id),
                &context,
                request,
                move |board, actor| board.release(actor, &id, fence?, &input.reason),
            )
            .await?;
        if !result.0.replayed {
            self.boards
                .forget_fence(&subject, &input.project, &input.id);
        }
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
    ) -> Result<Json<MutationOut<Ticket>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let subject = self.subject_id(&context)?;
        let fence =
            request.fence(
                self.boards
                    .fence_for(&subject, &input.project, &input.id, input.fence),
            );
        let id = input.id.clone();
        let result = self
            .board_mutation(
                "board_complete",
                "complete",
                &input.project,
                Some(&input.id),
                &context,
                request,
                move |board, actor| board.complete(actor, &id, fence?, &input.summary),
            )
            .await?;
        if !result.0.replayed {
            self.boards
                .forget_fence(&subject, &input.project, &input.id);
        }
        Ok(result)
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
    ) -> Result<Json<MutationOut<Ticket>>, Json<BoardError>> {
        let request = MutationInput::of(&input)?;
        let id = input.id.clone();
        self.board_mutation(
            "board_force_release",
            "force_release",
            &input.project,
            Some(&input.id),
            &context,
            request,
            move |board, actor| board.force_release(actor, &id, &input.reason),
        )
        .await
    }
}
