//! Reading a board: prime, what is ready, list, get, the events, waiting for one, and verifying the chain.

use std::time::Duration;

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::super::{
    BoardError, EventsIn, EventsOut, ListIn, PrimeIn, ProjectIn, ReadyIn, TicketIn, TicketsOut,
    WaitIn, refuse,
};
use crate::{
    agenthub::{Anchors, ChainStatus, Event, Filter, Prime, TicketView, face::ME},
    mcp::Hub,
};

#[tool_router(router = board_read_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "board_prime",
        description = "Start here, every session: the tickets you hold (with their fences), what is ready for you to claim, what needs a person, notes addressed to you, and the heartbeat interval. Your identity is the bearer token; `project` is the board.",
        title = "Start a board session",
        annotations(
            title = "Start a board session",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_prime(
        &self,
        Parameters(input): Parameters<PrimeIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Prime>, Json<BoardError>> {
        let limit = input.limit.unwrap_or(10) as usize;
        self.board_call(
            "board_prime",
            &input.project,
            None,
            &context,
            move |board, actor| board.prime(actor, limit),
        )
        .await
    }

    #[tool(
        name = "board_ready",
        description = "Tickets you could claim right now, best first: assigned to you, then priority, then age. Only lane queues with no open blocker and no live lease.",
        title = "Tickets ready to claim",
        annotations(
            title = "Tickets ready to claim",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_ready(
        &self,
        Parameters(input): Parameters<ReadyIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<TicketsOut>, Json<BoardError>> {
        let limit = input.limit.unwrap_or(20).clamp(1, 200) as usize;
        self.board_call(
            "board_ready",
            &input.project,
            None,
            &context,
            move |board, actor| board.ready(actor, limit).map(|tickets| TicketsOut { tickets }),
        )
        .await
    }

    #[tool(
        name = "board_list",
        description = "Tickets on the board, filtered by state, holder, assignee, label, parent, kind, text, machine, repo, environment, agent, session or fingerprint. open_only leaves out done and cancelled.",
        title = "List tickets",
        annotations(title = "List tickets", read_only_hint = true, open_world_hint = false)
    )]
    async fn board_list(
        &self,
        Parameters(input): Parameters<ListIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<TicketsOut>, Json<BoardError>> {
        let filter = Filter {
            state: input.state,
            holder: input.holder,
            assignee: input.assignee,
            label: input.label,
            parent: input.parent,
            kind: input.kind,
            ready: false,
            open_only: input.open_only,
            text: input.text,
            anchors: Anchors {
                machine: input.machine,
                repo: input.repo,
                environment: input.environment,
                agent: input.agent,
                session: input.session,
                fingerprint: input.fingerprint,
            },
            limit: Some(input.limit.unwrap_or(50).clamp(1, 500) as usize),
        };
        self.board_call(
            "board_list",
            &input.project,
            None,
            &context,
            move |board, actor| {
                // `@me` is whoever the caller is on this board (a seat is spelled by the
                // hub, so the command line cannot name itself).
                let me = |who: Option<String>| {
                    who.map(|w| if w == ME { actor.id.to_string() } else { w })
                };
                let filter = Filter {
                    holder: me(filter.holder),
                    assignee: me(filter.assignee),
                    ..filter
                };
                board.list(&filter).map(|tickets| TicketsOut { tickets })
            },
        )
        .await
    }

    #[tool(
        name = "board_get",
        description = "One ticket in full: fields, version, holder and lease, dependencies both ways, children, and every note.",
        title = "Get a ticket",
        annotations(title = "Get a ticket", read_only_hint = true, open_world_hint = false)
    )]
    async fn board_get(
        &self,
        Parameters(input): Parameters<TicketIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<TicketView>, Json<BoardError>> {
        let id = input.id.clone();
        self.board_call(
            "board_get",
            &input.project,
            Some(&input.id),
            &context,
            move |board, _| board.get(&id),
        )
        .await
    }

    #[tool(
        name = "board_events",
        description = "The board's append-only, hash-chained event log after a sequence number: every create, claim, note, handoff and completion, by whom and when.",
        title = "Board event log",
        annotations(
            title = "Board event log",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_events(
        &self,
        Parameters(input): Parameters<EventsIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<EventsOut>, Json<BoardError>> {
        let limit = input.limit.unwrap_or(100).clamp(1, 1000) as usize;
        let after = input.after_seq;
        self.board_call(
            "board_events",
            &input.project,
            None,
            &context,
            move |board, _| board.events(after, limit).map(|events| EventsOut { events }),
        )
        .await
    }

    #[tool(
        name = "board_wait",
        description = "Wait for board events: returns the events newer than after_seq the moment the board records one, or an empty list when timeout_ms passes (call again). Use it instead of calling board_events in a loop.",
        title = "Wait for board events",
        annotations(
            title = "Wait for board events",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_wait(
        &self,
        Parameters(input): Parameters<WaitIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<EventsOut>, Json<BoardError>> {
        let subject = self.board_subject(&context)?;
        let result = self.wait_for_events(&subject, &input, &context).await;
        // One audit line per wait, however long it blocked, refusals included.
        let outcome = match &result {
            Ok(_) => "ok".to_string(),
            Err(error) => error.code.clone(),
        };
        self.record_completion(&subject, "board_wait", &input.project, "", &outcome)
            .await;
        result
            .map(|events| Json(EventsOut { events }))
            .map_err(|error| refuse(error, None))
    }

    #[tool(
        name = "board_verify",
        description = "Recompute the board's event hash chain and report the first broken entry, if any.",
        title = "Verify the board's event chain",
        annotations(
            title = "Verify the board's event chain",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_verify(
        &self,
        Parameters(input): Parameters<ProjectIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ChainStatus>, Json<BoardError>> {
        self.board_call(
            "board_verify",
            &input.project,
            None,
            &context,
            move |board, _| board.verify_chain(),
        )
        .await
    }
}

impl Hub {
    /// The events newer than `after_seq`, as soon as there are any, or none when the wait
    /// runs out.
    async fn wait_for_events(
        &self,
        subject: &crate::identity::Subject,
        input: &WaitIn,
        context: &RequestContext<RoleServer>,
    ) -> devagotchi_contracts::BridgeResult<Vec<Event>> {
        let (board, _) = self.board_for(subject, &input.project)?;
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(input.timeout_ms.unwrap_or(20_000).min(25_000));
        let after = input.after_seq;
        // Subscribed before the first read: a change between the read and the sleep is kept.
        let mut changes = board.changes();
        loop {
            changes.borrow_and_update();
            let board = board.clone();
            let found = tokio::task::spawn_blocking(move || board.events(after, 100))
                .await
                .unwrap_or_else(|join| {
                    Err(BridgeError::new(
                        "board_store",
                        format!("board call did not complete: {join}"),
                    ))
                })?;
            if !found.is_empty() {
                return Ok(found);
            }
            // Sleep until the board commits a change; nothing reads it in between.
            tokio::select! {
                biased;
                _ = context.ct.cancelled() => return Ok(Vec::new()),
                changed = tokio::time::timeout_at(deadline, changes.changed()) => {
                    if !matches!(changed, Ok(Ok(()))) {
                        return Ok(Vec::new());
                    }
                }
            }
        }
    }
}
