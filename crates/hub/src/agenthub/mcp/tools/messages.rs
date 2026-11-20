//! Reaching others on a board: the inbox, waking an agent, and asking a person for an approval and waiting on it.

use std::time::Duration;

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use super::super::{
    ApprovalOut, ApprovalRequestIn, ApprovalWaitIn, BoardError, InboxIn, InboxOut, WakeIn, WakeOut,
    refuse,
};
use crate::mcp::Hub;

#[tool_router(router = board_message_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "board_inbox",
        description = "The notes addressed to you that you have not marked read, complete. New ones also arrive by themselves on your next tool call. Pass ack_through (a note's seq) to mark everything through it read and stop the reminders.",
        title = "Read notes left for you",
        annotations(
            title = "Read notes left for you",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn board_inbox(
        &self,
        Parameters(input): Parameters<InboxIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<InboxOut>, Json<BoardError>> {
        let names = self.client_names(&context);
        self.board_call(
            "board_inbox",
            &input.project,
            None,
            &context,
            move |board, actor| {
                let mut who = names;
                who.push(actor.id.to_string());
                board
                    .inbox(actor, &who, input.ack_through)
                    .map(|notes| InboxOut { notes })
            },
        )
        .await
    }

    #[tool(
        name = "board_wake",
        description = "Give an AI that has no open session (a custom agent) a webhook: its new notes are POSTed to the URL, complete and signed, at most once a minute. Returns the signing secret once. Omit url to remove it.",
        title = "Set a webhook for notes",
        annotations(
            title = "Set a webhook for notes",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn board_wake(
        &self,
        Parameters(input): Parameters<WakeIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<WakeOut>, Json<BoardError>> {
        let subject = self.board_subject(&context)?;
        let owner = match self.board_access(&subject, &input.project) {
            Ok((_, _, role)) => role.may_configure(),
            Err(_) => true,
        };
        // The URL is checked (resolved, public) before the call and refused inside it, so a
        // refused URL is audited like any other refusal (G7).
        let checked = match &input.url {
            Some(url) => crate::agenthub::wake::target(url, !self.config.multi_tenant())
                .await
                .map(drop),
            None => Ok(()),
        };
        self.board_call(
            "board_wake",
            &input.project,
            None,
            &context,
            move |board, actor| {
                if !owner {
                    return Err(BridgeError::new(
                        "not_owner",
                        "this board's webhooks are its owner's",
                    ));
                }
                checked?;
                let secret = board.set_wake(actor, &input.recipient, input.url.as_deref())?;
                Ok(WakeOut {
                    recipient: input.recipient.clone(),
                    secret,
                })
            },
        )
        .await
    }

    #[tool(
        name = "approval_request",
        description = "Ask the user to approve one step before you take it: what will run or change, why, how to undo it, and the kind of step (kind: restart, deploy, edit-file …) with its target when that matters. When the user approved exactly that step ahead of time (the same kind, target and what) it comes back approved at once with `standing`: take the step, and only that step. Anything else waits for them. Otherwise it reaches them where they approve (their phone) and returns at once with the approval's id, status pending and phones (how many phones were notified; if 0, give the user the url yourself); then call approval_wait with that id and act only when it says approved. A denial means do not take the step.",
        title = "Ask the user to approve a step",
        annotations(
            title = "Ask the user to approve a step",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn approval_request(
        &self,
        Parameters(input): Parameters<ApprovalRequestIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ApprovalOut>, Json<BoardError>> {
        let project = input.project.clone();
        let Json((approval, standing)) = self
            .board_call(
                "approval_request",
                &input.project,
                None,
                &context,
                move |board, actor| {
                    board.ask(
                        actor,
                        &crate::agenthub::approval::Ask {
                            title: &input.title,
                            what: &input.what,
                            why: input.why.as_deref().unwrap_or_default(),
                            undo: input.undo.as_deref().unwrap_or_default(),
                            kind: input.kind.as_deref().filter(|k| !k.trim().is_empty()),
                            target: input.target.as_deref().filter(|t| !t.trim().is_empty()),
                        },
                    )
                },
            )
            .await?;
        // Approved ahead of time: nobody is asked, so nobody is notified.
        if standing.is_some() {
            let mut out = self.approval_out(&project, approval);
            out.phones = Some(0);
            out.standing = standing;
            return Ok(Json(out));
        }
        // On the account's own board the push goes to that account's phones; never to
        // the people of a configured board that shares no more than the name.
        let subject = self.board_subject(&context)?;
        let people = match self.board_access(&subject, &project) {
            Ok((_, kind, _)) => self.approvers(&subject, &project, kind),
            Err(_) => Vec::new(),
        };
        let phones = self.announce_approval_to(people, &project, &approval).await;
        let mut out = self.approval_out(&project, approval);
        out.phones = Some(phones);
        Ok(Json(out))
    }

    #[tool(
        name = "approval_wait",
        description = "Wait for the user's decision on an approval you requested: returns as soon as they approve or deny, or with status pending when timeout_ms passes (call again). status approved: take the step. denied: do not; read note for why.",
        title = "Wait for the user's decision",
        annotations(
            title = "Wait for the user's decision",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn approval_wait(
        &self,
        Parameters(input): Parameters<ApprovalWaitIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ApprovalOut>, Json<BoardError>> {
        let subject = self.board_subject(&context)?;
        let result = self.wait_for_decision(&subject, &input, &context).await;
        // One audit line per wait, however long it blocked: the outcome, with the status
        // the wait ended on beside the id.
        let (target, outcome) = match &result {
            Ok(approval) => (format!("{} {}", input.id, approval.status), "ok".to_string()),
            Err(error) => (input.id.clone(), error.code.clone()),
        };
        self.record_completion(&subject, "approval_wait", &input.project, &target, &outcome)
            .await;
        result
            .map(|approval| Json(self.approval_out(&input.project, approval)))
            .map_err(|error| refuse(error, Some(&input.id)))
    }
}

impl Hub {
    /// The approval as it is once it is decided, or when the wait runs out.
    async fn wait_for_decision(
        &self,
        subject: &crate::identity::Subject,
        input: &ApprovalWaitIn,
        context: &RequestContext<RoleServer>,
    ) -> devagotchi_contracts::BridgeResult<crate::agenthub::approval::ApprovalStatus> {
        let (board, _) = self.board_for(subject, &input.project)?;
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(input.timeout_ms.unwrap_or(25_000).min(50_000));
        let mut changes = board.changes();
        loop {
            changes.borrow_and_update();
            let (board, id) = (board.clone(), input.id.clone());
            let status = tokio::task::spawn_blocking(move || board.approval_status(&id))
                .await
                .unwrap_or_else(|join| {
                    Err(BridgeError::new(
                        "board_store",
                        format!("board call did not complete: {join}"),
                    ))
                })?;
            if status.status != "pending" || tokio::time::Instant::now() >= deadline {
                return Ok(status);
            }
            // The decision is a board write: sleep until the board commits a change.
            tokio::select! {
                biased;
                _ = context.ct.cancelled() => return Ok(status),
                changed = tokio::time::timeout_at(deadline, changes.changed()) => {
                    if !matches!(changed, Ok(Ok(()))) {
                        return Ok(status);
                    }
                }
            }
        }
    }
}
