use super::*;

pub(in crate::broker) struct AuthorityClock {
    issued_at: i64,
    expires_at: i64,
    pub(in crate::broker) deadline: watch::Sender<Instant>,
}
impl AuthorityClock {
    pub(in crate::broker) fn new(authority: &GrantAuthority) -> BridgeResult<Self> {
        let remaining = authority.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        Ok(Self {
            issued_at: authority.issued_at,
            expires_at: authority.expires_at,
            deadline: watch::channel(Instant::now() + Duration::from_millis(remaining)).0,
        })
    }
    pub(in crate::broker) fn refresh(&mut self, authority: &GrantAuthority) -> BridgeResult<()> {
        let remaining = authority.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        if Instant::now() >= *self.deadline.borrow() {
            return Err(denied(
                "authority_expired",
                "expired admission authority cannot be resumed",
            ));
        }
        if authority.issued_at < self.issued_at
            || authority.issued_at == self.issued_at && authority.expires_at != self.expires_at
        {
            return Err(denied(
                "authority_stale",
                "authority refresh is stale or changes an existing issue",
            ));
        }
        if authority.issued_at > self.issued_at {
            self.issued_at = authority.issued_at;
            self.expires_at = authority.expires_at;
            self.deadline
                .send_replace(Instant::now() + Duration::from_millis(remaining));
        }
        Ok(())
    }
}
pub(in crate::broker) fn watch_expiry(
    shared: &Arc<Shared>,
    id: String,
    mut clock: watch::Receiver<Instant>,
) {
    let shared = Arc::downgrade(shared);
    tokio::spawn(async move {
        loop {
            let deadline = *clock.borrow_and_update();
            tokio::select! {
                result=clock.changed()=>if result.is_err() {return;},
                _=tokio::time::sleep_until(deadline)=> {
                    let Some(shared)=shared.upgrade() else {return;};
                    let mut state=shared.lock();
                    let Some(operation)=state.operations.get_mut(&id) else {return;};
                    if !operation.active {return;}
                    if operation.authority.as_ref().is_some_and(|a|Instant::now()<*a.deadline.borrow()) {continue;}
                    operation.authority=None;
                    if let Some(control)=&operation.control {let _=control.send(Some(StopReason::GrantExpired));}
                    return;
                }
            }
        }
    });
}
impl Shared {
    /// Serialize process creation with interruption, after any asynchronous terminal setup.
    pub(crate) fn while_launch_authorized<T>(
        &self,
        id: &str,
        deadline: Instant,
        launch: impl FnOnce() -> T,
    ) -> Option<T> {
        let state = self.lock();
        let operation = state.operations.get(id)?;
        if !operation.active
            || Instant::now() >= deadline
            || operation
                .control
                .as_ref()
                .is_none_or(|c| c.borrow().is_some())
            || operation.record.grant.is_some()
                && operation
                    .authority
                    .as_ref()
                    .is_none_or(|a| Instant::now() >= *a.deadline.borrow())
        {
            return None;
        }
        Some(launch())
    }
    pub(crate) fn grant_expired(&self, id: &str) -> bool {
        let state = self.lock();
        state.operations.get(id).is_some_and(|operation| {
            operation.record.grant.is_some()
                && operation
                    .authority
                    .as_ref()
                    .is_none_or(|clock| Instant::now() >= *clock.deadline.borrow())
        })
    }
}
impl Broker {
    pub(in crate::broker) fn refresh_authority(
        &self,
        authority: GrantAuthority,
    ) -> BridgeResult<RemoteReply> {
        authority.remaining_ms(
            now_ms()
                .try_into()
                .map_err(|_| invalid("broker clock overflow"))?,
        )?;
        let mut state = self.shared.lock();
        state.journal.check_grant(&authority.binding)?;
        let id = state
            .keys
            .get(&authority.binding.operation_key)
            .cloned()
            .ok_or_else(not_found)?;
        let operation = state.operations.get_mut(&id).ok_or_else(not_found)?;
        if operation.record.grant.as_ref() != Some(&authority.binding) {
            return Err(denied(
                "authority_mismatch",
                "refresh does not match the original admission",
            ));
        }
        if !operation.active
            || operation
                .control
                .as_ref()
                .is_none_or(|c| c.borrow().is_some())
        {
            return Err(denied(
                "authority_expired",
                "settled or interrupted operations cannot be resumed",
            ));
        }
        operation
            .authority
            .as_mut()
            .ok_or_else(|| denied("authority_expired", "admission authority ended"))?
            .refresh(&authority)?;
        Ok(RemoteReply::Refreshed { operation_id: id })
    }
    pub(in crate::broker) fn interrupt_authority(
        &self,
        notice: GrantInterruption,
    ) -> BridgeResult<RemoteReply> {
        notice.validate()?;
        let mut state = self.shared.lock();
        if let Err(error) = state.journal.interrupt_grant(&notice) {
            state.fault(error.clone());
            return Err(error);
        }
        let mut stopped = Vec::new();
        for operation in state.operations.values_mut() {
            if operation.active
                && operation.record.grant.as_ref().is_some_and(|g| {
                    g.origin == notice.origin
                        && g.grant_id == notice.grant_id
                        && g.grant_version < notice.minimum_version
                })
            {
                operation.authority = None;
                operation.record.view.cancellation_requested = true;
                operation.record.view.updated_at_ms = now_ms();
                stopped.push((operation.record.view.clone(), operation.control.clone()));
            }
        }
        for (view, _) in &stopped {
            if let Err(error) = state.journal.update(view) {
                state.fault(error.clone());
                return Err(error);
            }
        }
        let count = stopped.len() as u32;
        drop(state);
        for (_, control) in stopped {
            if let Some(control) = control {
                let _ = control.send(Some(StopReason::GrantInterrupted));
            }
        }
        self.shared.changed();
        Ok(RemoteReply::Interrupted { count })
    }
}
