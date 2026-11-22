//! Read-only, admission-bound history. Observation never renews execution authority.
use super::*;

impl Broker {
    pub(super) async fn remote_observe(
        &self,
        observation: GrantObservation,
        request: RequestBody,
    ) -> BridgeResult<RemoteReply> {
        let id = match &request {
            RequestBody::OperationGet(input) => &input.operation_id,
            RequestBody::OperationRead(input) => &input.operation_id,
            RequestBody::OperationWait(input) => &input.operation_id,
            _ => {
                return Err(denied(
                    "not_granted",
                    "observation permits only get, read and wait",
                ));
            }
        }
        .clone();
        let wait_after = match &request {
            RequestBody::OperationWait(input) => {
                if !(1..=MAX_WAIT_MS).contains(&input.timeout_ms) {
                    return Err(invalid("timeout_ms must be from 1 through MAX_WAIT_MS"));
                }
                Some(input.output_after)
            }
            _ => None,
        };
        let remaining = observation.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        self.check_observation(&observation.binding, &id)?;
        let response = match request {
            RequestBody::OperationWait(mut input) => {
                if remaining < 2 {
                    return Err(denied("authority_expired", "observation is expiring"));
                }
                input.timeout_ms = input.timeout_ms.min(remaining / 2);
                self.wait(input).await.map(ResponseBody::Operation)
            }
            request => self.handle(request).await,
        }?;
        observation.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        let sequence = self.check_observation(&observation.binding, &id)?;
        let ready = wait_after.is_none()
            || matches!(&response, ResponseBody::Operation(view) if view.state.terminal())
            || wait_after.flatten().is_some_and(|after| sequence > after);
        Ok(RemoteReply::Observed { response, ready })
    }

    fn check_observation(&self, binding: &GrantBinding, id: &str) -> BridgeResult<u64> {
        binding.validate()?;
        let state = self.shared.lock();
        let operation = state.operations.get(id).ok_or_else(not_found)?;
        if operation.record.grant.as_ref() != Some(binding) {
            return Err(denied(
                "authority_mismatch",
                "operation belongs to another admission",
            ));
        }
        Ok(operation.record.latest_sequence)
    }
}
