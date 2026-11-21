//! Bounded granted calls, pinned to the authenticated connection's enrollment.
use super::*;
use crate::machines::devices::MachineBinding;
use devagotchi_contracts::{
    API_VERSION, BridgeResult, GrantOrigin, REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest,
    RemoteWireRequest, RemoteWireResponse,
};
use std::sync::atomic::Ordering;

pub(super) type RemotePending = Arc<Mutex<HashMap<u64, oneshot::Sender<RemoteWireResponse>>>>;
#[derive(Clone)]
pub(super) struct RemoteSession {
    pub version: Option<u32>,
    pub binding: MachineBinding,
    pub key: String,
    pub calls: mpsc::Sender<(u64, RemoteWireRequest)>,
    pub pending: RemotePending,
}
impl RemoteSession {
    #[cfg(test)]
    pub fn unavailable() -> Self {
        Self {
            version: None,
            binding: MachineBinding {
                name: String::new(),
                generation: String::new(),
                owner: None,
            },
            key: String::new(),
            calls: mpsc::channel(CALL_QUEUE).0,
            pending: RemotePending::default(),
        }
    }
}
struct RemoteWaiter {
    pending: RemotePending,
    id: u64,
}
impl Drop for RemoteWaiter {
    fn drop(&mut self) {
        self.pending
            .lock()
            .expect("remote pending")
            .remove(&self.id);
    }
}
impl Links {
    #[cfg(test)]
    pub(crate) fn connect_remote_for_test(
        &self,
        binding: MachineBinding,
        key: &str,
    ) -> mpsc::Receiver<(u64, RemoteWireRequest)> {
        let (mut session, _) = Session::fake(1, None, false);
        let (calls, received) = mpsc::channel(CALL_QUEUE);
        session.remote = RemoteSession {
            version: Some(REMOTE_ACCESS_VERSION),
            binding: binding.clone(),
            key: key.into(),
            calls,
            pending: RemotePending::default(),
        };
        self.sessions
            .lock()
            .expect("links")
            .insert(binding.name, session);
        received
    }
    #[cfg(test)]
    pub(crate) fn answer_remote_for_test(
        &self,
        device: &str,
        id: u64,
        response: RemoteWireResponse,
    ) {
        let session = self
            .sessions
            .lock()
            .expect("links")
            .get(device)
            .cloned()
            .expect("test session");
        let waiter = session
            .remote
            .pending
            .lock()
            .expect("remote pending")
            .remove(&id)
            .expect("test waiter");
        let _ = waiter.send(response);
    }
    pub(crate) fn grant_origin(&self, binding: &MachineBinding) -> GrantOrigin {
        GrantOrigin {
            hub: hex::encode(Sha256::digest(self.config.public_url.as_bytes())),
            machine: binding.name.clone(),
            generation: binding.generation.clone(),
        }
    }
    pub(crate) async fn call_remote(
        &self,
        device: &str,
        remote: RemoteRequest,
    ) -> BridgeResult<RemoteReply> {
        let session = self
            .sessions
            .lock()
            .expect("links")
            .get(device)
            .cloned()
            .ok_or_else(|| offline(device))?;
        if session.remote.version != Some(REMOTE_ACCESS_VERSION) {
            return Err(BridgeError::new(
                "remote_access_unavailable",
                "machine link does not support granted execution",
            ));
        }
        let binding = self
            .devices
            .link_binding(device, &session.remote.key)
            .await?;
        if binding != session.remote.binding {
            return Err(BridgeError::new(
                "machine_reenrolled",
                "link belongs to an older enrollment",
            ));
        }
        let supplied = match &remote {
            RemoteRequest::Prepare { origin, .. } => Some(origin),
            RemoteRequest::Status { binding } => Some(&binding.origin),
            RemoteRequest::Execute { authority, .. } | RemoteRequest::Refresh { authority } => {
                Some(&authority.binding.origin)
            }
            RemoteRequest::Interrupt { notice } => Some(&notice.origin),
            RemoteRequest::Capabilities {} => None,
        };
        if supplied.is_some_and(|origin| origin != &self.grant_origin(&binding)) {
            return Err(BridgeError::new(
                "authority_mismatch",
                "remote call belongs to another hub or machine enrollment",
            ));
        }
        let _capacity = session
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| BridgeError::new("link_busy", "machine call capacity is full"))?;
        let held = match &remote {
            RemoteRequest::Execute { request, .. } => request.held_for(),
            _ => Duration::ZERO,
        };
        let started = match &remote {
            RemoteRequest::Execute { request, .. } => calls::command_of(request),
            _ => None,
        };
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        session
            .remote
            .pending
            .lock()
            .expect("remote pending")
            .insert(id, tx);
        let _waiter = RemoteWaiter {
            pending: session.remote.pending.clone(),
            id,
        };
        match session.remote.calls.try_send((
            id,
            RemoteWireRequest {
                api_version: API_VERSION,
                remote,
            },
        )) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                return Err(BridgeError::new(
                    "link_busy",
                    "machine granted call queue is full",
                ));
            }
            Err(mpsc::error::TrySendError::Closed(_)) => return Err(offline(device)),
        }
        let response = match tokio::time::timeout(calls::CALL_TIMEOUT + held, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return Err(offline(device)),
            Err(_) => {
                return Err(BridgeError::new(
                    "link_timeout",
                    "machine did not answer; admitted work remains bounded by its broker authority",
                ));
            }
        };
        // Revocation/re-enrollment while waiting must not disclose the old response.
        if self
            .devices
            .link_binding(device, &session.remote.key)
            .await?
            != binding
        {
            return Err(BridgeError::new(
                "machine_reenrolled",
                "machine enrollment changed during call",
            ));
        }
        match response {
            RemoteWireResponse::Ok(reply) => {
                if let RemoteReply::Executed(body) = &reply {
                    self.jobs_saw(device, started, body);
                }
                Ok(reply)
            }
            RemoteWireResponse::Error(error) => Err(error),
        }
    }
}
