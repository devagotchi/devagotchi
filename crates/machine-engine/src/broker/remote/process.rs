//! Admission-bound observation and cancellation. Interactive effects have their own adapter.
use super::*;

impl Broker {
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
                operation
                    .authority
                    .as_mut()
                    .ok_or_else(|| denied("authority_expired", "admission authority ended"))?
                    .refresh(&authority)?;
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
