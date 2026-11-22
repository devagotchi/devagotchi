//! Preserve grant authority over IPC and bind it to the authenticated hub connection.
use crate::state::LinkConfig;
use devagotchi_contracts::*;
use sha2::{Digest, Sha256};

pub(super) fn origin(config: &LinkConfig, generation: Option<String>) -> Option<GrantOrigin> {
    generation.map(|generation| GrantOrigin {
        hub: hex::encode(Sha256::digest(config.hub.as_bytes())),
        machine: config.device.clone(),
        generation,
    })
}
pub(super) async fn handle(
    broker: &std::path::Path,
    origin: Option<&GrantOrigin>,
    wire: RemoteWireRequest,
) -> RemoteWireResponse {
    let result = async {
        check(origin, &wire)?;
        devagotchi_ipc::remote_request(broker, wire).await
    }
    .await;
    match result {
        Ok(reply) => RemoteWireResponse::Ok(reply),
        Err(error) => RemoteWireResponse::Error(error),
    }
}
fn check(origin: Option<&GrantOrigin>, wire: &RemoteWireRequest) -> BridgeResult<()> {
    let origin = origin.ok_or_else(|| {
        BridgeError::new(
            "remote_access_unavailable",
            "hub did not bind this link's enrollment",
        )
    })?;
    origin.validate()?;
    if wire.api_version != API_VERSION {
        return Err(BridgeError::new(
            "api_version",
            "unsupported granted broker protocol",
        ));
    }
    let supplied = match &wire.remote {
        RemoteRequest::Prepare { origin, .. } => Some(origin),
        RemoteRequest::Status { binding } => Some(&binding.origin),
        RemoteRequest::Observe { observation, .. } => Some(&observation.binding.origin),
        RemoteRequest::Control { control, .. } => Some(&control.binding.origin),
        RemoteRequest::Execute { authority, .. } | RemoteRequest::Refresh { authority } => {
            Some(&authority.binding.origin)
        }
        RemoteRequest::Interrupt { notice } => Some(&notice.origin),
        RemoteRequest::Capabilities {} => None,
    };
    if supplied.is_some_and(|supplied| supplied != origin) {
        return Err(BridgeError::new(
            "authority_mismatch",
            "grant origin differs from this authenticated hub and machine enrollment",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_link_refuses_cross_hub_machine_generation_and_unnegotiated_authority() {
        let origin = GrantOrigin {
            hub: "a".repeat(64),
            machine: "machine".into(),
            generation: "generation".into(),
        };
        for changed in 0..4 {
            let mut supplied = origin.clone();
            match changed {
                0 => supplied.hub = "b".repeat(64),
                1 => supplied.machine = "other".into(),
                2 => supplied.generation = "new".into(),
                _ => {}
            }
            let wire = RemoteWireRequest {
                api_version: API_VERSION,
                remote: RemoteRequest::Interrupt {
                    notice: GrantInterruption {
                        origin: supplied.clone(),
                        grant_id: "grant".into(),
                        minimum_version: 2,
                    },
                },
            };
            assert_eq!(check(Some(&origin), &wire).is_ok(), changed == 3);
            assert_eq!(
                check(None, &wire).unwrap_err().code,
                "remote_access_unavailable"
            );
            let wire = RemoteWireRequest {
                api_version: API_VERSION,
                remote: RemoteRequest::Observe {
                    observation: GrantObservation {
                        binding: GrantBinding {
                            origin: supplied,
                            account: "owner".into(),
                            principal: "owner".into(),
                            admission_id: "admission".into(),
                            grant_id: "grant".into(),
                            grant_version: 1,
                            operation_key: "key".into(),
                            input_sha256: "a".repeat(64),
                            effective: GrantItem {
                                capability: GrantCapability::Exec,
                                sandbox_level: GrantLevel::Unsandboxed,
                                selector: Default::default(),
                            },
                            lease_sha256: None,
                        },
                        issued_at: 1,
                        expires_at: 100,
                    },
                    client: "client".into(),
                    request: RequestBody::OperationGet(GetInput {
                        operation_id: "operation".into(),
                        caller: None,
                    }),
                },
            };
            assert_eq!(check(Some(&origin), &wire).is_ok(), changed == 3);
            assert_eq!(
                check(None, &wire).unwrap_err().code,
                "remote_access_unavailable"
            );
            let RemoteRequest::Observe {
                observation,
                client,
                ..
            } = wire.remote
            else {
                unreachable!()
            };
            let wire = RemoteWireRequest {
                api_version: API_VERSION,
                remote: RemoteRequest::Control {
                    control: GrantControl {
                        binding: observation.binding,
                        action: ProcessControl::Cancel,
                        issued_at: observation.issued_at,
                        expires_at: observation.expires_at,
                    },
                    client,
                    request: RequestBody::OperationCancel(GetInput {
                        operation_id: "operation".into(),
                        caller: None,
                    }),
                },
            };
            assert_eq!(check(Some(&origin), &wire).is_ok(), changed == 3);
            assert_eq!(
                check(None, &wire).unwrap_err().code,
                "remote_access_unavailable"
            );
        }
    }
}
