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
mod prepared;
mod selection;
use prepared::PreparedInput;
mod files;
pub(crate) use selection::StartSelection;

#[derive(Debug, Clone)]
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
/// Owned, non-serializable broker preparation. Private fields prevent adapters from
/// replacing canonical input, selected authority or request identity after validation.
#[derive(Debug, Clone)]
pub(crate) struct PreparedCall {
    account: String,
    principal: String,
    grant_id: String,
    client: String,
    target: ExecutionTarget,
    origin: GrantOrigin,
    levels: Vec<GrantLevel>,
    input: PreparedInput,
    action: GrantAction,
    operation_key: String,
    input_sha256: String,
    call: GrantCall,
}
impl PreparedCall {
    pub(crate) fn grant_id(&self) -> &str {
        &self.grant_id
    }
    pub(crate) fn project(&self) -> Option<&str> {
        self.target.project.as_deref()
    }
}
impl Admitted {
    pub(super) fn binding(&self, origin: GrantOrigin) -> BridgeResult<GrantBinding> {
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
        let binding = GrantBinding {
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
        };
        binding.validate()?;
        Ok(binding)
    }
    pub(crate) fn authority(&self, origin: GrantOrigin) -> BridgeResult<GrantAuthority> {
        let authority = GrantAuthority {
            binding: self.binding(origin)?,
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
    /// Preparation has no effect and consumes no use. Adapters must refresh current
    /// credentials and scopes after this wait, before calling execute_prepared.
    #[cfg(test)]
    pub(crate) async fn prepare_start(
        &self,
        execution: &LinkExecution<'_>,
        input: StartInput,
    ) -> BridgeResult<PreparedCall> {
        self.prepare_call(execution, PreparedInput::Start(input))
            .await
    }
    async fn prepare_call(
        &self,
        execution: &LinkExecution<'_>,
        input: PreparedInput,
    ) -> BridgeResult<PreparedCall> {
        let target = self
            .execution_target(execution.account, execution.actor, execution.grant_id)
            .await?;
        let origin = execution.links.grant_origin(&target.machine);
        let actor = GrantActor {
            account: execution.account.into(),
            principal: execution.actor.id.into(),
            client: execution.client.into(),
        };
        actor.validate()?;
        let levels = match execution
            .links
            .call_remote(&target.machine.name, input.capabilities())
            .await?
        {
            RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                sandbox_levels,
                ..
            } if matches!(&input, PreparedInput::Start(_)) => sandbox_levels,
            RemoteReply::FileCapabilities {
                version: REMOTE_FILE_ACCESS_VERSION,
                sandbox_levels,
            } if matches!(&input, PreparedInput::Files(_)) => sandbox_levels,
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
                input.prepare(origin.clone(), actor, target.level),
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
        let expected = input.action();
        if action != expected || operation_key != input.key() || call.level != target.level {
            return Err(BridgeError::new(
                "invalid_response",
                "broker preparation changed the requested operation identity",
            ));
        }
        Ok(PreparedCall {
            account: execution.account.into(),
            principal: execution.actor.id.into(),
            grant_id: execution.grant_id.into(),
            client: execution.client.into(),
            target,
            origin,
            levels,
            input,
            action,
            operation_key,
            input_sha256,
            call,
        })
    }
    fn check_prepared_identity(
        &self,
        execution: &LinkExecution<'_>,
        prepared: &PreparedCall,
    ) -> BridgeResult<()> {
        if prepared.account != execution.account
            || prepared.principal != execution.actor.id
            || prepared.grant_id != execution.grant_id
            || prepared.client != execution.client
            || prepared.origin != execution.links.grant_origin(&prepared.target.machine)
        {
            return Err(BridgeError::new(
                "authority_mismatch",
                "preparation belongs to another request",
            ));
        }
        Ok(())
    }
    /// Current domain authority is checked after adapter credential refresh.
    async fn admit_prepared(
        &self,
        execution: &LinkExecution<'_>,
        prepared: &PreparedCall,
    ) -> BridgeResult<Admitted> {
        self.check_prepared_identity(execution, prepared)?;
        self.admit(
            execution.account,
            execution.actor,
            execution.board,
            &AdmissionMachine {
                binding: &prepared.target.machine,
                sandbox_levels: &prepared.levels,
            },
            &AdmissionInput {
                action: prepared.action,
                grant_id: execution.grant_id,
                operation_key: &prepared.operation_key,
                input_sha256: &prepared.input_sha256,
                call: &prepared.call,
            },
        )
        .await
    }

    pub(crate) async fn execute_prepared(
        &self,
        execution: &LinkExecution<'_>,
        prepared: PreparedCall,
    ) -> BridgeResult<(Admitted, StartOutput)> {
        let PreparedInput::Start(input) = &prepared.input else {
            return Err(BridgeError::new(
                "authority_mismatch",
                "process execution needs a process preparation",
            ));
        };
        let admitted = self.admit_prepared(execution, &prepared).await?;
        let authority = admitted.authority(prepared.origin.clone())?;
        match execution
            .links
            .call_remote(
                &prepared.target.machine.name,
                RemoteRequest::Execute {
                    authority,
                    client: execution.client.into(),
                    request: RequestBody::OperationStart(input.clone()),
                },
            )
            .await?
        {
            RemoteReply::Executed(ResponseBody::Start(output)) => {
                self.bind_operation(
                    execution.account,
                    execution.actor,
                    &prepared.target.machine,
                    &admitted.admission_id,
                    &output.operation.operation_id,
                )
                .await?;
                Ok((admitted, output))
            }
            _ => Err(BridgeError::new(
                "invalid_response",
                "broker did not return an operation start",
            )),
        }
    }
}
