//! Existing input, terminal sizing and cancellation owners for local and granted calls.
use super::*;

impl Broker {
    /// Type into an interactive operation: text, then named keys, delivered in order once
    /// the caller holds the keyboard (see `claim_writer`).
    pub(super) async fn input(&self, input: SendInput) -> BridgeResult<InputSent> {
        self.input_bound(input, None).await
    }
    pub(super) async fn input_bound(
        &self,
        input: SendInput,
        permit: Option<ControlPermit>,
    ) -> BridgeResult<InputSent> {
        let id = validate_id(&input.operation_id)?;
        let text = input.text.unwrap_or_default();
        if text.len() > MAX_INPUT_TEXT_BYTES || input.keys.len() > MAX_INPUT_KEYS {
            return Err(invalid(format!(
                "text is at most {MAX_INPUT_TEXT_BYTES} bytes and keys at most {MAX_INPUT_KEYS}"
            )));
        }
        if text.is_empty() && input.keys.is_empty() {
            return Err(invalid("Give text, keys, or both"));
        }
        let caller = input.caller.ok_or_else(no_caller)?;
        let (sender, steps, cursor) = {
            let mut state = self.shared.lock();
            if let Some(permit) = &permit {
                permit.check_locked(&state)?;
            }
            let operation = reach_mut(&mut state.operations, &id, Some(&caller))?;
            let mode = claim_writer(operation, &caller, input.take_over)?;
            let steps = input_steps(mode, &text, &input.keys)?;
            let sender = operation.input.clone().ok_or_else(input_closed)?;
            (sender, steps, operation.record.latest_sequence)
        };
        self.shared.output_changed();
        let closes = steps.contains(&InputStep::Close);
        deliver(&sender, steps, permit).await?;
        if closes {
            self.shared.input_closed(&id);
        }
        Ok(InputSent {
            operation: self.get(&id, Some(&caller))?,
            cursor,
        })
    }

    /// Resize an interactive operation's terminal; its program gets SIGWINCH.
    pub(super) async fn resize(&self, input: ResizeInput) -> BridgeResult<OperationView> {
        self.resize_bound(input, None).await
    }
    pub(super) async fn resize_bound(
        &self,
        input: ResizeInput,
        permit: Option<ControlPermit>,
    ) -> BridgeResult<OperationView> {
        let id = validate_id(&input.operation_id)?;
        check_size(input.cols, input.rows)?;
        let caller = input.caller.ok_or_else(no_caller)?;
        let sender = {
            let mut state = self.shared.lock();
            if let Some(permit) = &permit {
                permit.check_locked(&state)?;
            }
            let operation = reach_mut(&mut state.operations, &id, Some(&caller))?;
            if claim_writer(operation, &caller, input.take_over)? != InputMode::Terminal {
                return Err(BridgeError::new(
                    "no_terminal",
                    "This operation reads a pipe, not a terminal; there is nothing to resize",
                ));
            }
            operation.input.clone().ok_or_else(input_closed)?
        };
        deliver(
            &sender,
            vec![InputStep::Resize {
                cols: input.cols,
                rows: input.rows,
            }],
            permit,
        )
        .await?;
        let mut state = self.shared.lock();
        let operation = reach_mut(&mut state.operations, &id, Some(&caller))?;
        if let Some(interactive) = &mut operation.record.view.interactive {
            interactive.cols = input.cols;
            interactive.rows = input.rows;
        }
        operation.record.view.updated_at_ms = now_ms();
        let view = operation.record.view.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error.clone());
            return Err(error);
        }
        drop(state);
        self.shared.output_changed();
        Ok(view)
    }

    pub(super) fn cancel(&self, id: &str, caller: Option<&Caller>) -> BridgeResult<OperationView> {
        self.cancel_bound(id, caller, None)
    }
    pub(super) fn cancel_bound(
        &self,
        id: &str,
        caller: Option<&Caller>,
        permit: Option<&ControlPermit>,
    ) -> BridgeResult<OperationView> {
        let id = validate_id(id)?;
        let mut state = self.shared.lock();
        if let Some(permit) = permit {
            permit.check_locked(&state)?;
        }
        let operation = reach_mut(&mut state.operations, &id, caller)?;
        if !operation.active {
            return Ok(operation.record.view.clone());
        }
        operation.record.view.cancellation_requested = true;
        operation.record.view.updated_at_ms = now_ms();
        let view = operation.record.view.clone();
        let control = operation.control.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error.clone());
            return Err(error);
        }
        drop(state);
        self.shared.changed();
        if let Some(control) = control {
            let _ = control.send(Some(StopReason::Cancelled));
        }
        Ok(view)
    }
}
