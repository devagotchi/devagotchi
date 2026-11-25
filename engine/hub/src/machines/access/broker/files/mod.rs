//! File pages reuse shared selection and admission, then complete the single intent.
//! Owns: no tables; admissions and completion use the existing access owners.
//! Serves: prepared file pages and one intent completion.
//! Settings: existing grant limits and authority TTL.
//! Depends on: shared preparation, authenticated links and broker receipts.
use super::*;
use crate::machines::access::admission_completion::Completion;
#[cfg(test)]
mod tests;

impl Access {
    /// Recheck live authority after the final reply without reopening the finished read.
    pub(crate) async fn check_finished_files(
        &self,
        execution: &LinkExecution<'_>,
        prepared: &PreparedCall,
        admitted: &Admitted,
    ) -> BridgeResult<()> {
        self.check_prepared_identity(execution, prepared)?;
        transaction!(&self.accounts.store, |c| {
            let current = self
                .current_in(
                    c,
                    execution.account,
                    execution.actor,
                    execution.board,
                    &prepared.target.machine,
                    &super::super::current::CurrentInput {
                        grant_id: execution.grant_id,
                        action: prepared.action,
                    },
                )
                .await?;
            if current.grant.version != admitted.grant_version {
                return Err(refused(
                    "authority_mismatch",
                    "file grant changed before final disclosure",
                ));
            }
            current.grant.terms.item.selector.permits(
                current.grant.terms.item.capability,
                current.grant.terms.item.sandbox_level,
                &prepared.call,
            )?;
            Ok(())
        })
    }
    pub(crate) async fn check_prepared_files(
        &self,
        execution: &LinkExecution<'_>,
        prepared: &PreparedCall,
    ) -> BridgeResult<()> {
        self.admit_prepared(execution, prepared).await.map(|_| ())
    }
    pub(crate) async fn execute_prepared_files(
        &self,
        execution: &LinkExecution<'_>,
        prepared: &PreparedCall,
        step: FileStep,
    ) -> BridgeResult<(Admitted, FileOutcome)> {
        let PreparedInput::Files(invocation) = &prepared.input else {
            return Err(BridgeError::new(
                "authority_mismatch",
                "file execution requires a file preparation",
            ));
        };
        invocation.request_for(&step)?;
        let admitted = self.admit_prepared(execution, prepared).await?;
        let authority = admitted.authority(prepared.origin.clone())?;
        let RemoteReply::FileExecuted {
            operation_id,
            outcome,
            ..
        } = execution
            .links
            .call_remote(
                &prepared.target.machine.name,
                RemoteRequest::ExecuteFiles {
                    authority,
                    client: execution.client.into(),
                    invocation: invocation.clone(),
                    step,
                },
            )
            .await?
        else {
            return Err(BridgeError::new(
                "invalid_response",
                "broker did not return a file result",
            ));
        };
        self.bind_operation(
            execution.account,
            execution.actor,
            &prepared.target.machine,
            &admitted.admission_id,
            &operation_id,
        )
        .await?;
        if invocation.action() == GrantAction::FileWrite {
            self.complete_admission(
                execution.account,
                execution.actor,
                &prepared.target.machine,
                &admitted.admission_id,
                &Completion::Finished {
                    operation_id,
                    exit_code: None,
                    error_code: outcome.error_code().map(str::to_owned),
                },
            )
            .await?;
        }
        Ok((admitted, outcome))
    }

    pub(crate) async fn finish_prepared_files(
        &self,
        execution: &LinkExecution<'_>,
        prepared: &PreparedCall,
        admitted: &Admitted,
    ) -> BridgeResult<()> {
        if !matches!(&prepared.input, PreparedInput::Files(_))
            || prepared.operation_key != admitted.operation_key
        {
            return Err(BridgeError::new(
                "authority_mismatch",
                "file completion belongs to another intent",
            ));
        }
        let RemoteReply::FileFinished {
            operation_id,
            error_code,
        } = execution
            .links
            .call_remote(
                &prepared.target.machine.name,
                RemoteRequest::FinishFiles {
                    binding: admitted.binding(prepared.origin.clone())?,
                },
            )
            .await?
        else {
            return Err(BridgeError::new(
                "invalid_response",
                "broker did not confirm file completion",
            ));
        };
        self.complete_admission(
            execution.account,
            execution.actor,
            &prepared.target.machine,
            &admitted.admission_id,
            &Completion::Finished {
                operation_id,
                exit_code: None,
                error_code,
            },
        )
        .await?;
        Ok(())
    }
}
