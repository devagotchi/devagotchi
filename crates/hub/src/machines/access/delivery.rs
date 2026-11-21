//! Durable interruption delivery; transport failure never acknowledges the outbox.
use super::*;
use crate::machines::{devices::MachineBinding, link::Links};
use devagotchi_contracts::{GrantInterruption, RemoteReply, RemoteRequest};
use futures_util::{StreamExt, stream};

impl Access {
    pub(crate) async fn deliver_interruptions(
        &self,
        links: &Links,
        tenant: &str,
        limit: u32,
    ) -> BridgeResult<u32> {
        let notices = self.take_notifications(tenant, limit).await?;
        let results = stream::iter(notices)
            .map(|notice| async move {
                let origin = links.grant_origin(&MachineBinding {
                    name: notice.machine.clone(),
                    generation: notice.generation.clone(),
                    owner: None,
                });
                let result = links
                    .call_remote(
                        &notice.machine,
                        RemoteRequest::Interrupt {
                            notice: GrantInterruption {
                                origin,
                                grant_id: notice.grant_id.clone(),
                                minimum_version: notice.version,
                            },
                        },
                    )
                    .await;
                let failure = match &result {
                    Ok(RemoteReply::Interrupted { .. }) => None,
                    Ok(_) => Some("invalid_response"),
                    // Persist only a bounded code, never the transport's diagnostic message.
                    Err(error) => Some(match error.code.as_str() {
                        "broker_offline" => "broker_offline",
                        "link_busy" => "link_busy",
                        "link_timeout" => "link_timeout",
                        "machine_reenrolled" | "authority_mismatch" => "machine_reenrolled",
                        "device_revoked" => "device_revoked",
                        "remote_access_unavailable" => "remote_access_unavailable",
                        _ => "delivery_failed",
                    }),
                };
                let recorded = self.finish_notification(tenant, &notice, failure).await?;
                Ok::<_, BridgeError>(u32::from(recorded && failure.is_none()))
            })
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;
        results
            .into_iter()
            .try_fold(0, |total, result| result.map(|count| total + count))
    }
}
