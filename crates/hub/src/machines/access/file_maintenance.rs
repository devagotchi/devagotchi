//! File lifecycle evidence: reads expire without autonomous renewal; mutation receipts
//! may be finished, explicitly uncertain, or permanently retired.
use super::*;
use super::{admission_completion::Completion, continuation::AdmissionSnapshot};
use devagotchi_contracts::{FileExecutionState, GrantCapability};

impl Access {
    pub(super) async fn maintain_file_receipt(
        &self,
        tenant: &str,
        id: &str,
        snapshot: &AdmissionSnapshot,
        operation_id: String,
        state: FileExecutionState,
    ) -> BridgeResult<()> {
        if !matches!(
            snapshot.binding.effective.capability,
            GrantCapability::FileRead | GrantCapability::FileWrite
        ) || operation_id.is_empty()
            || operation_id.len() > 128
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || snapshot
                .operation_id
                .as_ref()
                .is_some_and(|old| old != &operation_id)
        {
            return Err(BridgeError::new(
                "invalid_response",
                "file lifecycle evidence identifies another admission",
            ));
        }
        let completion = match state {
            FileExecutionState::Active { .. } => return Ok(()),
            FileExecutionState::Finished { error_code } => {
                if error_code.as_ref().is_some_and(|code| {
                    code.is_empty()
                        || code.len() > 64
                        || !code
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                }) {
                    return Err(BridgeError::new(
                        "invalid_response",
                        "file completion needs a bounded diagnostic code",
                    ));
                }
                Completion::Finished {
                    operation_id,
                    exit_code: None,
                    error_code,
                }
            }
            FileExecutionState::Uncertain {} => Completion::Finished {
                operation_id,
                exit_code: None,
                error_code: Some("file_outcome_uncertain".into()),
            },
            FileExecutionState::Retired {} => Completion::Retired { operation_id },
        };
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            Self::complete_admission_in(
                c,
                tenant,
                &snapshot.binding.account,
                &snapshot.binding.principal,
                &snapshot.machine,
                id,
                &completion,
            )
            .await?;
            Ok(())
        })
    }
}
