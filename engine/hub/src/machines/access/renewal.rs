//! Deliver a renewed authority only to the same enrolled broker and admitted operation.
use super::admissions::{AdmissionMachine, Admitted};
use super::*;
use crate::machines::{devices::MachineBinding, link::Links};
use devagotchi_agenthub::{Actor, hosted::HostedBoard};
use devagotchi_contracts::{REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest};

pub(crate) struct LinkContinuation<'a> {
    pub account: &'a str,
    pub actor: &'a Actor<'a>,
    pub board: Option<&'a HostedBoard>,
    pub machine: &'a MachineBinding,
    pub links: &'a Links,
}
impl Access {
    pub(crate) async fn refresh_execution(
        &self,
        execution: &LinkContinuation<'_>,
        admission_id: &str,
    ) -> BridgeResult<Admitted> {
        let levels = match execution
            .links
            .call_remote(&execution.machine.name, RemoteRequest::Capabilities {})
            .await?
        {
            RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                sandbox_levels,
                ..
            } => sandbox_levels,
            _ => {
                return Err(BridgeError::new(
                    "invalid_response",
                    "broker did not prove its grant capabilities",
                ));
            }
        };
        // Broker waits precede the transaction: revocation or a changed task/profile
        // while waiting is checked before any fresh authority leaves the hub.
        let continuation = self
            .renew_admission(
                execution.account,
                execution.actor,
                execution.board,
                &AdmissionMachine {
                    binding: execution.machine,
                    sandbox_levels: &levels,
                },
                admission_id,
            )
            .await?;
        let authority = continuation
            .admitted
            .authority(execution.links.grant_origin(execution.machine))?;
        match execution
            .links
            .call_remote(
                &execution.machine.name,
                RemoteRequest::Refresh { authority },
            )
            .await?
        {
            RemoteReply::Refreshed { operation_id }
                if operation_id == continuation.operation_id =>
            {
                Ok(continuation.admitted)
            }
            _ => Err(BridgeError::new(
                "invalid_response",
                "broker did not acknowledge this admission's operation",
            )),
        }
    }
}
