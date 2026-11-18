//! The broker's history: it holds at most `max_operations` operations, and makes room by
//! retiring the one that finished longest ago. A retired operation's ID is no longer found;
//! its idempotency key stays in the journal for good and is refused, never run again
//! ([DG-RETRY-1](../../../../.ai/docs/retry-lifecycle.md)).

use super::*;

impl Operation {
    /// Settled and no longer owned by a job: the only kind of operation ever retired.
    /// `room_for_one`'s refusal and `retire_oldest_finished`'s choice both ask this one
    /// predicate, so "nothing can be retired" and "retire this one" never disagree.
    fn finished(&self) -> bool {
        !self.active && self.record.view.state.terminal()
    }
}

fn every_operation_runs() -> BridgeError {
    BridgeError::new(
        "operation_limit_reached",
        "Every operation the broker holds is still running; wait for one to finish or cancel one",
    )
}

impl State {
    /// Refuse, changing nothing, when the history is full and none of it has finished.
    pub(super) fn room_for_one(&self) -> BridgeResult<()> {
        if self.operations.len() >= self.limits.max_operations
            && !self.operations.values().any(Operation::finished)
        {
            return Err(every_operation_runs());
        }
        Ok(())
    }

    /// Retire the oldest finished operations until one more fits: several when
    /// `max_operations` was lowered in broker.json across a restart. Called after the
    /// concurrency check, where fewer than `max_concurrent` (no more than `max_operations`)
    /// run, so retiring every finished one always makes room. A journal error is returned
    /// as it is: the retirement it hit rolled back whole.
    pub(super) fn make_room(&mut self) -> BridgeResult<()> {
        while self.operations.len() >= self.limits.max_operations {
            self.retire_oldest_finished()?;
        }
        Ok(())
    }

    /// Retire finished operations down to a lowered `max_operations`, now rather than one
    /// per start. Operations still running stay until they finish.
    pub(super) fn shrink(&mut self) -> BridgeResult<()> {
        while self.operations.len() > self.limits.max_operations
            && self.operations.values().any(Operation::finished)
        {
            self.retire_oldest_finished()?;
        }
        Ok(())
    }

    fn retire_oldest_finished(&mut self) -> BridgeResult<()> {
        let oldest = self
            .operations
            .iter()
            .filter(|(_, operation)| operation.finished())
            .min_by_key(|(_, operation)| {
                (
                    operation.record.view.updated_at_ms,
                    operation.record.view.created_at_ms,
                )
            })
            .map(|(id, _)| id.clone());
        let Some(id) = oldest else {
            return Err(every_operation_runs());
        };
        let record = &self.operations[&id].record;
        self.journal
            .retire(&id, &record.key, &record.request_digest, now_ms())?;
        if let Some(operation) = self.operations.remove(&id) {
            self.keys.remove(&operation.record.key);
        }
        Ok(())
    }
}
