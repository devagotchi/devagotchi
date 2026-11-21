//! Compose durable hub admission with the authenticated broker's canonical preparation.
use super::admissions::{AdmissionInput, AdmissionMachine, Admitted};
use super::*;
use crate::machines::{
    devices::{Devices, MachineBinding},
    link::Links,
};
use devagotchi_agenthub::{Actor, hosted::HostedBoard};
use devagotchi_contracts::*;
use sha2::{Digest, Sha256};

pub(crate) struct ExecutionTarget {
    pub machine: MachineBinding,
    pub level: GrantLevel,
    pub project: Option<String>,
}
pub(crate) struct LinkExecution<'a> {
    pub account: &'a str,
    pub actor: &'a Actor<'a>,
    pub board: Option<&'a HostedBoard>,
    pub links: &'a Links,
    pub grant_id: &'a str,
    pub client: &'a str,
}
impl Admitted {
    pub(super) fn authority(&self, origin: GrantOrigin) -> BridgeResult<GrantAuthority> {
        if origin.machine != self.machine || origin.generation != self.generation {
            return Err(BridgeError::new(
                "authority_mismatch",
                "admission belongs to another machine enrollment",
            ));
        }
        // Heartbeat expiry and mutable task labels must never change admission identity.
        let lease_sha256 = self
            .task
            .as_ref()
            .map(|task| {
                serde_json::to_vec(&(
                    &task.tenant,
                    &task.board,
                    &task.project,
                    &task.ticket,
                    &task.principal,
                    task.fence,
                    &task.lease_id,
                ))
                .map(|bytes| hex::encode(Sha256::digest(bytes)))
            })
            .transpose()
            .map_err(|_| invalid("task identity cannot be encoded"))?;
        let authority = GrantAuthority {
            binding: GrantBinding {
                origin,
                account: self.account.clone(),
                principal: self.principal.clone(),
                admission_id: self.admission_id.clone(),
                grant_id: self.grant_id.clone(),
                grant_version: self.grant_version,
                operation_key: self.operation_key.clone(),
                input_sha256: self.input_sha256.clone(),
                effective: self.effective.clone(),
                lease_sha256,
            },
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        };
        authority.remaining_ms(self.issued_at)?;
        Ok(authority)
    }
}
impl Access {
    /// Metadata used by trusted adapters to resolve the original board before admission.
    pub(crate) async fn execution_target(
        &self,
        account: &str,
        actor: &Actor<'_>,
        grant_id: &str,
    ) -> BridgeResult<ExecutionTarget> {
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            let grant = grant_record::GrantRecord::load(c, &found.tenant, grant_id).await?;
            if grant.account != account || grant.principal != actor.id {
                return Err(refused("not_granted", "grant belongs to another principal"));
            }
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(account), &grant.machine)
                .await?;
            Ok(ExecutionTarget {
                machine: grant.machine,
                level: grant.terms.item.sandbox_level,
                project: grant.task.map(|t| t.project),
            })
        })
    }
    /// No external I/O occurs inside a SQL transaction; the broker rechecks the issued
    /// authority immediately before durable admission and native process creation.
    pub(crate) async fn execute_start(
        &self,
        execution: &LinkExecution<'_>,
        input: StartInput,
    ) -> BridgeResult<(Admitted, StartOutput)> {
        let target = self
            .execution_target(execution.account, execution.actor, execution.grant_id)
            .await?;
        if target.project.is_some() != execution.board.is_some() {
            return Err(BridgeError::new(
                "not_granted",
                "execution needs the grant's original task board",
            ));
        }
        let origin = execution.links.grant_origin(&target.machine);
        let actor = GrantActor {
            account: execution.account.into(),
            principal: execution.actor.id.into(),
            client: execution.client.into(),
        };
        actor.validate()?;
        let levels = match execution
            .links
            .call_remote(&target.machine.name, RemoteRequest::Capabilities {})
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
        if !levels.contains(&target.level) {
            return Err(BridgeError::new(
                "sandbox_unavailable",
                "broker has not proved the grant's confinement level",
            ));
        }
        let prepared = execution
            .links
            .call_remote(
                &target.machine.name,
                RemoteRequest::Prepare {
                    origin: origin.clone(),
                    actor,
                    level: target.level,
                    request: RequestBody::OperationStart(input.clone()),
                },
            )
            .await?;
        let RemoteReply::Prepared {
            action,
            operation_key,
            input_sha256,
            call,
        } = prepared
        else {
            return Err(BridgeError::new(
                "invalid_response",
                "broker did not prepare this operation",
            ));
        };
        let expected = if input.interactive.is_some() {
            GrantAction::ProcessStart
        } else {
            GrantAction::Exec
        };
        if action != expected
            || operation_key != input.idempotency_key
            || call.level != target.level
        {
            return Err(BridgeError::new(
                "invalid_response",
                "broker preparation changed the requested operation identity",
            ));
        }
        let admitted = self
            .admit(
                execution.account,
                execution.actor,
                execution.board,
                &AdmissionMachine {
                    binding: &target.machine,
                    sandbox_levels: &levels,
                },
                &AdmissionInput {
                    action,
                    grant_id: execution.grant_id,
                    operation_key: &operation_key,
                    input_sha256: &input_sha256,
                    call: &call,
                },
            )
            .await?;
        let authority = admitted.authority(origin)?;
        match execution
            .links
            .call_remote(
                &target.machine.name,
                RemoteRequest::Execute {
                    authority,
                    client: execution.client.into(),
                    request: RequestBody::OperationStart(input),
                },
            )
            .await?
        {
            RemoteReply::Executed(ResponseBody::Start(output)) => Ok((admitted, output)),
            _ => Err(BridgeError::new(
                "invalid_response",
                "broker did not return an operation start",
            )),
        }
    }
}
