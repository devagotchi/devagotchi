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
mod selection;
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
pub(crate) struct PreparedStart {
    account: String,
    principal: String,
    grant_id: String,
    client: String,
    target: ExecutionTarget,
    origin: GrantOrigin,
    levels: Vec<GrantLevel>,
    input: StartInput,
    action: GrantAction,
    operation_key: String,
    input_sha256: String,
    call: GrantCall,
}
impl PreparedStart {
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
    pub(crate) async fn prepare_start(
        &self,
        execution: &LinkExecution<'_>,
        input: StartInput,
    ) -> BridgeResult<PreparedStart> {
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
        Ok(PreparedStart {
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
    /// Only the original request can consume this preparation. Current domain authority
    /// is checked in the admission transaction, after adapter credential refresh.
    pub(crate) async fn execute_prepared(
        &self,
        execution: &LinkExecution<'_>,
        prepared: PreparedStart,
    ) -> BridgeResult<(Admitted, StartOutput)> {
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
        let PreparedStart {
            target,
            origin,
            levels,
            input,
            action,
            operation_key,
            input_sha256,
            call,
            ..
        } = prepared;
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
            RemoteReply::Executed(ResponseBody::Start(output)) => {
                self.bind_operation(
                    execution.account,
                    execution.actor,
                    &target.machine,
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
