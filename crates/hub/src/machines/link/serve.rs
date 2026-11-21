//! One machine's link from hello to close: the bounded handshake, the session loop with its idle, revocation and send timeouts, forgetting the session, and auditing refusals.

use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use devagotchi_contracts::{
    BridgeError, HubFrame, LINK_IDLE_TIMEOUT, LINK_SEND_TIMEOUT, LinkFrame, REFUSAL_UNAVAILABLE,
    WireRequest, WireResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{OwnedSemaphorePermit, mpsc};

use crate::{machines::now_secs, ops::audit::AuditEntry};

use super::{
    Admitted, CALL_QUEUE, Links, Pending, RemotePending, RemoteSession, Session, fingerprint,
    random,
};

pub(super) const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// A hello is a few hundred bytes; anything larger is refused before parsing.
pub(super) const MAX_HELLO_BYTES: usize = 4096;

/// A link that sends nothing for this long (no frame, Ping or Pong) is considered gone; the
/// link beats every `LINK_HEARTBEAT`, and gives up a silent hub after the same window.
pub(super) const IDLE_TIMEOUT: Duration = LINK_IDLE_TIMEOUT;

/// No single write to a device may hold the session loop longer than this.
pub(super) const SEND_TIMEOUT: Duration = LINK_SEND_TIMEOUT;

pub(super) const REVOCATION_CHECK: Duration = Duration::from_secs(2);

/// What a refused peer is told. Details stay in the audit so the wire does not reveal
/// which device names exist or what state they are in.
pub(super) fn wire_refusal(error: &BridgeError) -> HubFrame {
    let (code, message) = match error.code.as_str() {
        "protocol_version" => ("protocol_version", error.message.as_str()),
        "device_store" | "audit_write" => (
            REFUSAL_UNAVAILABLE,
            "the hub could not check this device right now; retry",
        ),
        _ => (
            "refused",
            "this hub refused the link; its operator can see why in the hub's audit log",
        ),
    };
    HubFrame::Refused {
        code: code.into(),
        message: message.into(),
    }
}

pub(super) fn revoked_frame(device: &str) -> HubFrame {
    HubFrame::Refused {
        code: "device_revoked".into(),
        message: format!("{device} was revoked on this hub"),
    }
}

/// One frame to the device, bounded in time so a peer that stops reading cannot hold
/// the session loop (and with it the revocation and idle checks).
pub(super) async fn send<S>(sink: &mut S, frame: &HubFrame) -> Result<(), ()>
where
    S: futures_util::Sink<Message> + Unpin,
{
    let text = serde_json::to_string(frame).map_err(|_| ())?;
    match tokio::time::timeout(
        SEND_TIMEOUT,
        sink.send(Message::Text(Utf8Bytes::from(text))),
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

impl Links {
    /// A slot for one unauthenticated handshake, or `None` when all are taken.
    pub fn handshake_permit(&self) -> Option<OwnedSemaphorePermit> {
        self.handshakes.clone().try_acquire_owned().ok()
    }

    /// Serve one device connection until it closes, idles out or is revoked.
    pub async fn serve(
        self: Arc<Self>,
        socket: WebSocket,
        peer: SocketAddr,
        permit: OwnedSemaphorePermit,
    ) {
        let (mut sink, mut stream) = socket.split();
        let nonce = hex::encode(random::<32>());
        let challenge = HubFrame::Challenge {
            nonce: nonce.clone(),
            hub: self.config.public_url.clone(),
        };
        if send(&mut sink, &challenge).await.is_err() {
            return;
        }
        let hello = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) if text.len() <= MAX_HELLO_BYTES => {
                serde_json::from_str::<LinkFrame>(&text).ok()
            }
            _ => None,
        };
        let claimed = match &hello {
            Some(LinkFrame::Hello { device, .. }) => Some(device.clone()),
            _ => None,
        };
        if let Some(LinkFrame::Hello {
            enroll_code: Some(code),
            ..
        }) = &hello
            && let Err(error) = self.may_enroll(code).await
        {
            self.refused(claimed.as_deref(), peer, &error).await;
            let _ = send(&mut sink, &wire_refusal(&error)).await;
            let _ = tokio::time::timeout(SEND_TIMEOUT, sink.close()).await;
            return;
        }
        let Admitted {
            device,
            key,
            joined_for,
            build,
            container,
            remote_access,
        } = match self.admit(hello, &nonce).await {
            Ok(admitted) => admitted,
            Err(error) => {
                self.refused(claimed.as_deref(), peer, &error).await;
                let _ = send(&mut sink, &wire_refusal(&error)).await;
                let _ = tokio::time::timeout(SEND_TIMEOUT, sink.close()).await;
                return;
            }
        };
        let binding = match self.devices.link_binding(&device, &key).await {
            Ok(binding) => binding,
            Err(error) => {
                let _ = send(&mut sink, &wire_refusal(&error)).await;
                return;
            }
        };
        drop(permit);

        let (calls_tx, mut calls_rx) = mpsc::channel::<(u64, WireRequest)>(CALL_QUEUE);
        let (control_tx, mut control_rx) = mpsc::channel::<HubFrame>(4);
        let pending: Pending = Arc::default();
        let remote_pending: RemotePending = Arc::default();
        let (remote_tx, mut remote_rx) = mpsc::channel(CALL_QUEUE);
        let remote_session = RemoteSession {
            version: remote_access,
            binding: binding.clone(),
            key: key.clone(),
            calls: remote_tx,
            pending: remote_pending.clone(),
        };
        let session_id = self.next.fetch_add(1, Ordering::Relaxed);
        let previous = self.sessions.lock().expect("links").insert(
            device.clone(),
            Session {
                id: session_id,
                calls: calls_tx,
                pending: pending.clone(),
                build: build.clone(),
                container,
                control: control_tx,
                remote: remote_session,
                capacity: Arc::new(tokio::sync::Semaphore::new(CALL_QUEUE)),
            },
        );
        drop(previous); // A reconnect replaces the old session; its sender drops and it ends.
        self.changed();
        let subject = format!("device:{device}");
        let via = format!("link {peer}");
        self.event(
            &subject,
            &via,
            "link.connect",
            &device,
            &fingerprint(&key),
            "ok",
        )
        .await;
        if let (Some(owner), Some(push)) = (joined_for, self.push.clone()) {
            let print = fingerprint(&key);
            let name = device.clone();
            let product = self.config.site().name().to_string();
            tokio::spawn(async move {
                push.notify(
                    &[owner],
                    &serde_json::json!({
                        "title": format!("{name} joined {product}"),
                        "body": format!("Key {print}. Not you? Open Machines and revoke it."),
                        "url": "/account",
                        "tag": format!("devagotchi-machine-{name}"),
                    }),
                )
                .await;
            });
        }
        if send(
            &mut sink,
            &HubFrame::Welcome {
                device: device.clone(),
                generation: (remote_access == Some(devagotchi_contracts::REMOTE_ACCESS_VERSION))
                    .then(|| binding.generation.clone()),
            },
        )
        .await
        .is_err()
        {
            remote_pending.lock().expect("remote pending").clear();
            let _ = self.forget(&device, session_id, &pending);
            return;
        }
        // An update that brought this build is done; then any machine due one is asked.
        self.connected_on(&device, build.as_deref()).await;
        self.auto_update_now().await;

        let mut revocation = tokio::time::interval(REVOCATION_CHECK);
        let mut idle = Box::pin(tokio::time::sleep(IDLE_TIMEOUT));
        let reason = loop {
            tokio::select! {
                outgoing=remote_rx.recv()=>match outgoing {
                    Some((id,request))=> {
                        match self.devices.link_binding(&device,&key).await {
                            Ok(current) if current==binding=> {
                                if send(&mut sink,&HubFrame::RemoteCall {id,request}).await.is_err() {break "send_failed";}
                            }
                            _=> {let _=send(&mut sink,&revoked_frame(&device)).await;break "revoked";}
                        }
                    }
                    None=>break "replaced",
                },
                outgoing = calls_rx.recv() => match outgoing {
                    Some((id, request)) => {
                        // Re-check at the moment of forwarding: a call queued just before a
                        // revoke must not reach the device. Fail closed if the store errs.
                        match self.devices.is_revoked(&device).await {
                            Ok(false) => {
                                if send(&mut sink, &HubFrame::Call { id, request }).await.is_err() {
                                    break "send_failed";
                                }
                            }
                            Ok(true) => {
                                let _ = send(&mut sink, &revoked_frame(&device)).await;
                                break "revoked";
                            }
                            Err(error) => {
                                if let Some(waiter) = pending.lock().expect("pending").remove(&id) {
                                    let _ = waiter.send(WireResponse::Error(error));
                                }
                            }
                        }
                    }
                    None => break "replaced",
                },
                Some(frame) = control_rx.recv() => {
                    if send(&mut sink, &frame).await.is_err() {
                        break "send_failed";
                    }
                },
                incoming = stream.next() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        idle.as_mut().reset(tokio::time::Instant::now() + IDLE_TIMEOUT);
                        match serde_json::from_str::<LinkFrame>(&text) {
                            Ok(LinkFrame::RemoteResult {id,response})=> {
                                if remote_access!=Some(devagotchi_contracts::REMOTE_ACCESS_VERSION) {break "protocol_error";}
                                if let Some(waiter)=remote_pending.lock().expect("remote pending").remove(&id) {let _=waiter.send(response);}
                            }
                            Ok(LinkFrame::Result { id, response }) => {
                                if let Some(waiter) = pending.lock().expect("pending").remove(&id) {
                                    let _ = waiter.send(response);
                                }
                            }
                            Ok(LinkFrame::Heartbeat) => {}
                            Ok(LinkFrame::UpdateStatus { state, detail }) => {
                                self.update_reported(&device, &state, detail).await;
                            }
                            Ok(LinkFrame::Hello { .. }) | Err(_) => break "protocol_error",
                        }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
                        idle.as_mut().reset(tokio::time::Instant::now() + IDLE_TIMEOUT);
                    }
                    Some(Ok(Message::Binary(_))) => break "protocol_error",
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break "closed",
                },
                _ = &mut idle => break "idle_timeout",
                _ = revocation.tick() => {
                    // Only a definite revocation ends the link; a store hiccup does not.
                    if let Ok(true) = self.devices.is_revoked(&device).await {
                        let _ = send(&mut sink, &revoked_frame(&device)).await;
                        break "revoked";
                    }
                }
            }
        };
        remote_pending.lock().expect("remote pending").clear();
        if let Some(episode) = self.forget(&device, session_id, &pending) {
            self.clone().watch_offline(device.clone(), episode);
        }
        let _ = tokio::time::timeout(SEND_TIMEOUT, sink.close()).await;
        self.event(
            &subject,
            &via,
            "link.disconnect",
            &device,
            &fingerprint(&key),
            reason,
        )
        .await;
    }

    /// The session is gone; which drop this was, when it was the device's live session.
    pub(super) fn forget(&self, device: &str, session_id: u64, pending: &Pending) -> Option<u64> {
        let mut sessions = self.sessions.lock().expect("links");
        let mut episode = None;
        if sessions.get(device).is_some_and(|s| s.id == session_id) {
            sessions.remove(device);
            let this = self.next.fetch_add(1, Ordering::Relaxed);
            self.seen
                .lock()
                .expect("seen")
                .insert(device.to_string(), (now_secs(), this));
            episode = Some(this);
        }
        drop(sessions);
        self.changed();
        // Dropping the waiters answers every in-flight call with broker_offline.
        pending.lock().expect("pending").clear();
        episode
    }

    pub(super) async fn event(
        &self,
        subject: &str,
        via: &str,
        tool: &str,
        broker: &str,
        key: &str,
        outcome: &str,
    ) {
        self.audit
            .record_after(&AuditEntry {
                subject,
                via,
                tool,
                broker,
                operation_id: key,
                outcome,
            })
            .await;
    }

    /// Audit a refused handshake, at most once a second. The claimed name is recorded as
    /// unverified; the peer address is the TCP peer (the edge, when behind Caddy).
    pub(super) async fn refused(
        &self,
        claimed: Option<&str>,
        peer: SocketAddr,
        error: &BridgeError,
    ) {
        let now = now_secs();
        let suppressed = {
            let mut log = self.refusals.lock().expect("refusals");
            if log.last_second == now {
                log.suppressed += 1;
                return;
            }
            log.last_second = now;
            std::mem::take(&mut log.suppressed)
        };
        let claimed: String = claimed
            .unwrap_or("-")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(63)
            .collect();
        let subject = format!("unverified:{claimed}");
        let via = format!("link {peer}");
        let unitemized = match suppressed {
            0 => String::new(),
            n => format!("{n} more refusals not itemized"),
        };
        eprintln!(
            "devagotchi hub: link refused from {peer} claiming {claimed:?}: {}",
            error
        );
        self.event(
            &subject,
            &via,
            "link.refused",
            &claimed,
            &unitemized,
            &error.code,
        )
        .await;
    }
}
