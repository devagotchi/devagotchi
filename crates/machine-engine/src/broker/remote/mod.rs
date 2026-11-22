//! Granted remote execution at the broker boundary.
//! Owns: live authority clocks; grant identities/floors use the existing execution journal.
//! Serves: authenticated remote IPC prepare/execute/refresh/interruption.
//! Settings: bounded authority TTL and the broker's existing execution limits.
//! Depends on: shared grant contracts, canonical launch resolution and native execution.
use super::*;
mod clock;
mod observation;
mod process;
pub(super) use clock::AuthorityClock;
pub(super) use clock::watch_expiry;

fn denied(code: &str, message: &str) -> BridgeError {
    BridgeError::new(code, message)
}
fn caller(origin: &GrantOrigin, actor: &GrantActor) -> BridgeResult<Caller> {
    origin.validate()?;
    actor.validate()?;
    Ok(Caller::hub(
        &format!("{}:{}", origin.hub, actor.account),
        &actor.client,
    ))
}
fn supported(level: GrantLevel) -> BridgeResult<()> {
    if level != GrantLevel::Unsandboxed {
        return Err(denied(
            "sandbox_unavailable",
            "this broker has not proved the requested native confinement level",
        ));
    }
    Ok(())
}
pub(super) fn start_call(spec: &ExecutionSpec, level: GrantLevel) -> BridgeResult<GrantCall> {
    let mut call = GrantCall::new(
        if spec.interactive.is_some() {
            GrantCapability::Process
        } else {
            GrantCapability::Exec
        },
        level,
    );
    call.argv = std::iter::once(path_string(&spec.executable)?)
        .chain(spec.args.iter().cloned())
        .collect();
    call.paths = vec![path_string(&spec.cwd)?];
    call.runtime_s = Some(
        spec.deadline_ms
            .div_ceil(1000)
            .try_into()
            .map_err(|_| invalid("runtime exceeds grant vocabulary"))?,
    );
    Ok(call)
}
pub(super) fn check_start(
    input: &StartInput,
    spec: &ExecutionSpec,
    digest: &str,
    authority: &GrantAuthority,
) -> BridgeResult<()> {
    authority.remaining_ms(
        now_ms()
            .try_into()
            .map_err(|_| invalid("broker clock overflow"))?,
    )?;
    supported(authority.binding.effective.sandbox_level)?;
    if input.idempotency_key != authority.binding.operation_key
        || digest != authority.binding.input_sha256
    {
        return Err(denied(
            "authority_mismatch",
            "broker canonical operation differs from the approved admission",
        ));
    }
    let call = start_call(spec, authority.binding.effective.sandbox_level)?;
    authority.binding.effective.selector.permits(
        authority.binding.effective.capability,
        authority.binding.effective.sandbox_level,
        &call,
    )
}
impl Broker {
    /// Only an authenticated transport may construct this envelope. It cannot be a tool schema.
    pub async fn handle_remote(&self, wire: RemoteWireRequest) -> BridgeResult<RemoteReply> {
        if wire.api_version != API_VERSION {
            return Err(denied("api_version", "unsupported broker API version"));
        }
        match wire.remote {
            RemoteRequest::Capabilities {} => {
                let caps = self.capabilities()?;
                Ok(RemoteReply::Capabilities {
                    version: REMOTE_ACCESS_VERSION,
                    os: caps.os,
                    sandbox_levels: vec![GrantLevel::Unsandboxed],
                })
            }
            RemoteRequest::Prepare {
                origin,
                actor,
                level,
                mut request,
            } => {
                request.set_caller(&caller(&origin, &actor)?);
                supported(level)?;
                let RequestBody::OperationStart(input) = request else {
                    return Err(denied(
                        "remote_operation_unavailable",
                        "this remote operation has no enforcing broker adapter yet",
                    ));
                };
                let (spec, input_sha256) = self.normalize(&input)?;
                let call = start_call(&spec, level)?;
                Ok(RemoteReply::Prepared {
                    action: if spec.interactive.is_some() {
                        GrantAction::ProcessStart
                    } else {
                        GrantAction::Exec
                    },
                    operation_key: input.idempotency_key,
                    input_sha256,
                    call,
                })
            }
            RemoteRequest::Execute {
                authority,
                client,
                mut request,
            } => {
                let actor = GrantActor {
                    account: authority.binding.account.clone(),
                    principal: authority.binding.principal.clone(),
                    client,
                };
                request.set_caller(&caller(&authority.binding.origin, &actor)?);
                match request {
                    RequestBody::OperationStart(input) => self
                        .start_bound(input, Some(authority))
                        .map(|out| RemoteReply::Executed(ResponseBody::Start(out))),
                    request => self.remote_process(authority, request).await,
                }
            }
            RemoteRequest::Refresh { authority } => self.refresh_authority(authority),
            RemoteRequest::Status { binding } => self.admission_status(&binding),
            RemoteRequest::Observe {
                observation,
                client,
                mut request,
            } => {
                let actor = GrantActor {
                    account: observation.binding.account.clone(),
                    principal: observation.binding.principal.clone(),
                    client,
                };
                request.set_caller(&caller(&observation.binding.origin, &actor)?);
                self.remote_observe(observation, request).await
            }
            RemoteRequest::Interrupt { notice } => self.interrupt_authority(notice),
        }
    }
}
