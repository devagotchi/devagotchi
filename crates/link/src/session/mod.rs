//! One connection to the hub: prove the device key over the hub's challenge, then serve
//! `call` frames by handing each request to the local broker and sending back its answer.

use std::{sync::Arc, time::Duration};

use devagotchi_contracts::{
    API_VERSION, BridgeError, HUB_ACCOUNT_PREFIX, HubFrame, LINK_HEARTBEAT, LINK_IDLE_TIMEOUT,
    LINK_PROTOCOL_VERSION, LINK_SEND_TIMEOUT, LinkFrame, MAX_LINK_MESSAGE_BYTES,
    REFUSAL_UNAVAILABLE, RequestBody, WireRequest, WireResponse, link_signing_message,
};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    Connector,
    tungstenite::{Message, protocol::WebSocketConfig},
};

use crate::{
    state::{LinkConfig, endpoint},
    update::Updater,
};

mod remote;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Calls handled at once. Beyond this the link answers `link_busy` instead of queueing,
/// so a hub cannot make this machine open unbounded broker connections.
const MAX_IN_FLIGHT: usize = 16;
/// Waits (requests the broker holds until something changes) have their own pool, so a
/// caller waiting on many operations never crowds out ordinary calls.
const MAX_WAITS: usize = 8;

/// How a connection proves both ends are still there (`.ai/patterns/link.md` rule 6): a
/// `heartbeat` frame and a WebSocket Ping every `heartbeat`; give up after `idle` with nothing
/// from the hub (its Pong included), and after `send` on any one write.
#[derive(Debug, Clone, Copy)]
struct Liveness {
    heartbeat: Duration,
    idle: Duration,
    send: Duration,
}

/// The link's own: the contracts' values, which the hub keeps too.
const LIVENESS: Liveness = Liveness {
    heartbeat: LINK_HEARTBEAT,
    idle: LINK_IDLE_TIMEOUT,
    send: LINK_SEND_TIMEOUT,
};

/// Why a connection ended.
#[derive(Debug)]
pub enum Ended {
    /// Network trouble or the hub went away: reconnect after a pause.
    Retry(String),
    /// The hub refused this device (revoked, not enrolled, wrong key): stop and say why.
    /// A hub that is only temporarily unable to check (`unavailable`) is a Retry instead.
    Refused(BridgeError),
    /// `enroll` finished: the key is bound and the connection closed on purpose.
    Enrolled,
}

pub enum Mode<'a> {
    Enroll {
        code: &'a str,
    },
    /// Serve calls, and update this machine when the hub asks (`update.rs`).
    Serve {
        updater: &'a Arc<Updater>,
        state_dir: &'a std::path::Path,
    },
}

fn tls() -> Result<Connector, String> {
    use rustls_platform_verifier::ConfigVerifierExt;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let config = rustls::ClientConfig::with_platform_verifier().map_err(|e| e.to_string())?;
    Ok(Connector::Rustls(Arc::new(config)))
}

pub async fn connect(config: &LinkConfig, key: &SigningKey, mode: Mode<'_>) -> Ended {
    connect_with(config, key, mode, LIVENESS).await
}

async fn connect_with(
    config: &LinkConfig,
    key: &SigningKey,
    mode: Mode<'_>,
    liveness: Liveness,
) -> Ended {
    let url = match endpoint(&config.hub) {
        Ok(url) => url,
        Err(error) => return Ended::Refused(error),
    };
    let connector = match tls() {
        Ok(connector) => connector,
        Err(error) => return Ended::Refused(BridgeError::new("tls", error)),
    };
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(MAX_LINK_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_LINK_MESSAGE_BYTES));
    let connecting = tokio_tungstenite::connect_async_tls_with_config(
        url.as_str(),
        Some(ws_config),
        false,
        Some(connector),
    );
    let socket = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Ok(Ok((socket, _))) => socket,
        Ok(Err(error)) => return Ended::Retry(format!("connect {url}: {error}")),
        Err(_) => return Ended::Retry(format!("connect {url}: timed out")),
    };
    let (mut sink, mut stream) = socket.split();

    // Handshake: challenge in, signed hello out, welcome or refusal back.
    let nonce = match next_frame(&mut stream).await {
        Ok(HubFrame::Challenge { nonce, hub }) if hub == config.hub => nonce,
        Ok(HubFrame::Challenge { hub, .. }) => {
            return Ended::Refused(BridgeError::new(
                "hub_mismatch",
                format!("enrolled for {} but this hub says it is {hub}", config.hub),
            ));
        }
        Ok(
            HubFrame::Welcome { .. }
            | HubFrame::Refused { .. }
            | HubFrame::Call { .. }
            | HubFrame::RemoteCall { .. }
            | HubFrame::Update { .. },
        ) => return Ended::Retry("hub did not open with a challenge".into()),
        Err(why) => return Ended::Retry(why),
    };
    let signature = key.sign(&link_signing_message(&config.hub, &config.device, &nonce));
    let hello = LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: config.device.clone(),
        public_key: hex::encode(key.verifying_key().to_bytes()),
        signature: hex::encode(signature.to_bytes()),
        enroll_code: match mode {
            Mode::Enroll { code } => Some(code.to_string()),
            Mode::Serve { .. } => None,
        },
        build: match &mode {
            Mode::Serve { updater, .. } => Some(updater.build().to_string()),
            Mode::Enroll { .. } => None,
        },
        remote_access: match &mode {
            Mode::Serve { .. } => Some(devagotchi_contracts::REMOTE_ACCESS_VERSION),
            Mode::Enroll { .. } => None,
        },
        kind: match &mode {
            Mode::Serve { updater, .. } => updater.kind().map(str::to_string),
            Mode::Enroll { .. } => None,
        },
    };
    if let Err(why) = send(&mut sink, &hello, liveness.send).await {
        return Ended::Retry(why);
    }
    let generation = match next_frame(&mut stream).await {
        Ok(HubFrame::Welcome { device, generation }) if device == config.device => generation,
        Ok(HubFrame::Welcome { .. }) => {
            return Ended::Refused(BridgeError::new(
                "machine_mismatch",
                "hub welcomed another machine",
            ));
        }
        Ok(HubFrame::Refused { code, message }) => return refusal(code, message),
        Ok(
            HubFrame::Challenge { .. }
            | HubFrame::Call { .. }
            | HubFrame::RemoteCall { .. }
            | HubFrame::Update { .. },
        ) => {
            return Ended::Retry("hub answered the hello with an unexpected frame".into());
        }
        Err(why) => return Ended::Retry(why),
    };
    let grant_origin = remote::origin(config, generation);
    let (updater, state_dir) = match mode {
        Mode::Enroll { .. } => {
            let _ = sink.close().await;
            return Ended::Enrolled;
        }
        Mode::Serve { updater, state_dir } => (updater, state_dir),
    };
    eprintln!(
        "devagotchi link ready: {} linked to {} (broker {})",
        config.device, config.hub, config.broker
    );

    // Serve: each call runs in its own task; answers funnel back through one writer.
    let (answers_tx, mut answers_rx) = mpsc::channel::<LinkFrame>(64);
    let mut heartbeat = tokio::time::interval(liveness.heartbeat);
    heartbeat.tick().await;
    // The hub sends nothing while it has nothing to ask, but answers every Ping: silence
    // this long means it is gone (a NAT that forgot us, a sleeping host), whatever TCP says.
    let idle = tokio::time::sleep(liveness.idle);
    tokio::pin!(idle);
    let broker = std::path::PathBuf::from(&config.broker);
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_IN_FLIGHT));
    let waits = Arc::new(tokio::sync::Semaphore::new(MAX_WAITS));
    // How the update the previous link handed off went, then each change of one under way.
    if let Some(status) = crate::update::outcome(state_dir, updater.build())
        && let Err(why) = send(&mut sink, &status.frame(), liveness.send).await
    {
        return Ended::Retry(why);
    }
    let mut updates = updater.subscribe();
    updates.mark_unchanged();
    loop {
        tokio::select! {
            incoming = stream.next() => {
                if let Some(Ok(_)) = &incoming {
                    idle.as_mut().reset(tokio::time::Instant::now() + liveness.idle);
                }
                match incoming {
                    Some(Ok(Message::Text(text))) => match serde_json::from_str::<HubFrame>(&text) {
                        Ok(HubFrame::Call { id, request }) => {
                            let answers = answers_tx.clone();
                            let held = !request.request.held_for().is_zero();
                            let (pool, limit) = if held { (&waits, MAX_WAITS) } else { (&slots, MAX_IN_FLIGHT) };
                            let Ok(slot) = pool.clone().try_acquire_owned() else {
                                let busy = WireResponse::Error(BridgeError::new(
                                    "link_busy",
                                    format!("this machine is already handling {limit} {}", if held { "waits" } else { "calls" }),
                                ));
                                let _ = answers.try_send(LinkFrame::Result { id, response: busy });
                                continue;
                            };
                            let broker = broker.clone();
                            tokio::spawn(async move {
                                let response = handle(&broker, request).await;
                                let _ = answers.send(LinkFrame::Result { id, response }).await;
                                drop(slot);
                            });
                        }
                        Ok(HubFrame::RemoteCall {id,request})=> {
                            let held=matches!(&request.remote,devagotchi_contracts::RemoteRequest::Execute {request,..} if !request.held_for().is_zero());
                            let pool=if held {&waits} else {&slots};
                            let Ok(slot)=pool.clone().try_acquire_owned() else {
                                let response=devagotchi_contracts::RemoteWireResponse::Error(BridgeError::new("link_busy","machine call capacity is full"));
                                let _=answers_tx.try_send(LinkFrame::RemoteResult {id,response});
                                continue;
                            };
                            let broker=broker.clone();let origin=grant_origin.clone();let answers=answers_tx.clone();
                            tokio::spawn(async move {
                                let response=remote::handle(&broker,origin.as_ref(),request).await;
                                let _=answers.send(LinkFrame::RemoteResult {id,response}).await;
                                drop(slot);
                            });
                        }
                        Ok(HubFrame::Update { version, force }) => {
                            if let Some(status) = updater.request(version, force) {
                                let _ = answers_tx.try_send(status.frame());
                            }
                        }
                        Ok(HubFrame::Refused { code, message }) => return refusal(code, message),
                        Ok(HubFrame::Challenge { .. } | HubFrame::Welcome { .. }) | Err(_) => {
                            return Ended::Retry("hub sent a frame this link does not understand".into());
                        }
                    },
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Ok(Message::Binary(_))) => return Ended::Retry("hub sent a binary frame".into()),
                    Some(Ok(Message::Close(_))) | None => return Ended::Retry("hub closed the link".into()),
                    Some(Err(error)) => return Ended::Retry(format!("link read: {error}")),
                }
            }
            () = &mut idle => {
                return Ended::Retry(format!("hub silent for {}s", liveness.idle.as_secs_f32()));
            }
            Some(answer) = answers_rx.recv() => {
                if let Err(why) = send(&mut sink, &answer, liveness.send).await {
                    return Ended::Retry(why);
                }
            }
            Ok(()) = updates.changed() => {
                let status = updates.borrow_and_update().clone();
                if let Some(status) = status
                    && let Err(why) = send(&mut sink, &status.frame(), liveness.send).await
                {
                    return Ended::Retry(why);
                }
            }
            _ = heartbeat.tick() => {
                let alive = match send(&mut sink, &LinkFrame::Heartbeat, liveness.send).await {
                    Ok(()) => write(&mut sink, Message::Ping(Vec::new().into()), liveness.send).await,
                    Err(why) => Err(why),
                };
                if let Err(why) = alive {
                    return Ended::Retry(why);
                }
            }
        }
    }
}

/// A refusal from the hub: permanent unless the hub says it only could not check now.
fn refusal(code: String, message: String) -> Ended {
    if code == REFUSAL_UNAVAILABLE {
        Ended::Retry(format!("hub temporarily unavailable: {message}"))
    } else {
        Ended::Refused(BridgeError::new(&code, message))
    }
}

/// Hand one request to the local broker. The broker's own validation and executable
/// grants decide; the link adds nothing and removes nothing. It only refuses a request
/// that claims a caller no hub can be (see `from_a_hub`).
async fn handle(broker: &std::path::Path, request: WireRequest) -> WireResponse {
    if let Err(error) = from_a_hub(&request.request) {
        return WireResponse::Error(error);
    }
    if request.api_version != API_VERSION {
        return WireResponse::Error(BridgeError::new(
            "api_version",
            format!(
                "broker speaks api {API_VERSION}, hub sent {}",
                request.api_version
            ),
        ));
    }
    match devagotchi_ipc::request(broker, request.request).await {
        Ok(body) => WireResponse::Ok(body),
        Err(error) => WireResponse::Error(error),
    }
}

/// A hub speaks only for the accounts it signs in (`hub:<subject>`): never for this
/// machine's own user, who owns everything its broker runs. A request naming any other
/// caller came from a hub that is wrong or not ours, and reaches nothing.
fn from_a_hub(request: &RequestBody) -> Result<(), BridgeError> {
    match request.caller() {
        Some(caller) if !caller.account.starts_with(HUB_ACCOUNT_PREFIX) => Err(BridgeError::new(
            "not_your_operation",
            "This machine takes requests from a hub only for the hub's own accounts",
        )),
        _ => Ok(()),
    }
}

async fn next_frame<S>(stream: &mut S) -> Result<HubFrame, String>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match tokio::time::timeout(HANDSHAKE_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                return serde_json::from_str(&text).map_err(|e| format!("bad frame from hub: {e}"));
            }
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)))) => continue,
            Ok(Some(Ok(_))) => return Err("unexpected message from hub".into()),
            Ok(Some(Err(error))) => return Err(format!("link read: {error}")),
            Ok(None) => return Err("hub closed the link during the handshake".into()),
            Err(_) => return Err("hub did not answer in time".into()),
        }
    }
}

async fn send<S>(sink: &mut S, frame: &LinkFrame, within: Duration) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let text = serde_json::to_string(frame).map_err(|e| e.to_string())?;
    write(sink, Message::text(text), within).await
}

/// One write, given up after `within`: a hub that stopped reading cannot hold the link.
async fn write<S>(sink: &mut S, message: Message, within: Duration) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    match tokio::time::timeout(within, sink.send(message)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("link write: {error}")),
        Err(_) => Err(format!(
            "link write: the hub took more than {}s",
            within.as_secs_f32()
        )),
    }
}

#[cfg(test)]
mod tests;
