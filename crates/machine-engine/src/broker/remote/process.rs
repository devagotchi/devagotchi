//! Admission-bound observation and cancellation. Interactive effects have their own adapter.
use super::*;

impl Broker {
    /// Private lifecycle observation by the authenticated hub, including after a
    /// grant ends. Exact stored identity is required; no command/output is disclosed
    /// and no grant floor or live authority clock is changed by observation.
    pub(super) fn admission_status(&self, binding: &GrantBinding) -> BridgeResult<RemoteReply> {
        binding.validate()?;
        let state = self.shared.lock();
        let id = state
            .keys
            .get(&binding.operation_key)
            .ok_or_else(not_found)?;
        let operation = state.operations.get(id).ok_or_else(not_found)?;
        if operation.record.grant.as_ref() != Some(binding) {
            return Err(denied(
                "authority_mismatch",
                "operation belongs to another admission",
            ));
        }
        Ok(RemoteReply::Status {
            operation_id: id.clone(),
            state: operation.record.view.state.clone(),
            exit_code: operation.record.view.exit_code,
        })
    }

    pub(super) async fn remote_process(
        &self,
        authority: GrantAuthority,
        request: RequestBody,
    ) -> BridgeResult<RemoteReply> {
        let id = match &request {
            RequestBody::OperationGet(input) | RequestBody::OperationCancel(input) => {
                &input.operation_id
            }
            RequestBody::OperationRead(input) => &input.operation_id,
            RequestBody::OperationWait(input) => &input.operation_id,
            _ => {
                return Err(denied(
                    "remote_operation_unavailable",
                    "this remote operation has no enforcing broker adapter yet",
                ));
            }
        };
        let remaining = authority.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        {
            let mut state = self.shared.lock();
            state.journal.check_grant(&authority.binding)?;
            let operation = state.operations.get_mut(id).ok_or_else(not_found)?;
            if operation.record.grant.as_ref() != Some(&authority.binding) {
                return Err(denied(
                    "authority_mismatch",
                    "operation does not belong to this admission and principal",
                ));
            }
            if operation.active {
                if operation
                    .control
                    .as_ref()
                    .is_none_or(|c| c.borrow().is_some())
                {
                    return Err(denied(
                        "authority_expired",
                        "interrupted work cannot be resumed",
                    ));
                }
                if operation
                    .authority
                    .as_ref()
                    .is_none_or(|clock| Instant::now() >= *clock.deadline.borrow())
                {
                    return Err(denied("authority_expired", "admission authority ended"));
                }
            }
        }
        // A long poll must not hold a remote observation open beyond its authority.
        let response = match request {
            RequestBody::OperationWait(mut input) => {
                input.timeout_ms = input.timeout_ms.min(remaining);
                self.wait(input).await.map(ResponseBody::Operation)
            }
            request => self.handle(request).await,
        }?;
        // Recheck the durable floor after any await before disclosing the response.
        authority.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        self.shared.lock().journal.check_grant(&authority.binding)?;
        Ok(RemoteReply::Executed(response))
    }
}
