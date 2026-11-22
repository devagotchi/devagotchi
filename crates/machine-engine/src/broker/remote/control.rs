//! Process effects retain an action-bound deadline through native input delivery.
use super::*;

#[derive(Clone)]
pub(crate) struct ControlPermit {
    shared: std::sync::Weak<Shared>,
    binding: GrantBinding,
    operation: String,
    pub(crate) deadline: Instant,
}
impl ControlPermit {
    pub(crate) fn check(&self) -> BridgeResult<()> {
        let shared = self
            .shared
            .upgrade()
            .ok_or_else(|| denied("authority_expired", "broker ended"))?;
        self.check_locked(&shared.lock())
    }
    pub(in crate::broker) fn check_locked(&self, state: &State) -> BridgeResult<()> {
        if Instant::now() >= self.deadline {
            return Err(Self::expired());
        }
        let op = state
            .operations
            .get(&self.operation)
            .ok_or_else(not_found)?;
        if op.record.grant.as_ref() != Some(&self.binding) {
            return Err(denied(
                "authority_mismatch",
                "control belongs to another admission",
            ));
        }
        state.journal.check_grant(&self.binding)?;
        if !op.active
            || op.control.as_ref().is_none_or(|c| c.borrow().is_some())
            || op
                .authority
                .as_ref()
                .is_none_or(|a| Instant::now() >= *a.deadline.borrow())
        {
            return Err(Self::expired());
        }
        Ok(())
    }
    pub(crate) fn expired() -> BridgeError {
        denied(
            "authority_expired",
            "process input permission ended; earlier bytes may already have been delivered, so do not blindly retry",
        )
    }
}
impl Broker {
    pub(super) async fn remote_control(
        &self,
        control: GrantControl,
        request: RequestBody,
    ) -> BridgeResult<RemoteReply> {
        let (action, id) = match &request {
            RequestBody::OperationInput(input) => (ProcessControl::Input, &input.operation_id),
            RequestBody::OperationResize(input) => (ProcessControl::Resize, &input.operation_id),
            RequestBody::OperationCancel(input) => (ProcessControl::Cancel, &input.operation_id),
            _ => {
                return Err(denied(
                    "not_granted",
                    "control permits only input, resize or cancellation",
                ));
            }
        };
        if control.action != action {
            return Err(denied(
                "authority_mismatch",
                "control action differs from requested effect",
            ));
        }
        if action != ProcessControl::Cancel
            && control.binding.effective.capability != GrantCapability::Process
        {
            return Err(denied(
                "not_granted",
                "input needs the original interactive process grant",
            ));
        }
        let remaining = control.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        let permit = ControlPermit {
            shared: Arc::downgrade(&self.shared),
            binding: control.binding,
            operation: id.clone(),
            deadline: Instant::now() + Duration::from_millis(remaining),
        };
        permit.check()?;
        let response = match request {
            RequestBody::OperationInput(input) => self
                .input_bound(input, Some(permit))
                .await
                .map(ResponseBody::InputSent),
            RequestBody::OperationResize(input) => self
                .resize_bound(input, Some(permit))
                .await
                .map(ResponseBody::Operation),
            RequestBody::OperationCancel(input) => self
                .cancel_bound(&input.operation_id, input.caller.as_ref(), Some(&permit))
                .map(ResponseBody::Operation),
            _ => unreachable!("validated control action"),
        }?;
        Ok(RemoteReply::Controlled(response))
    }
}
