//! Waking an AI that has no MCP session open (a custom agent: Hermes, something built on
//! OpenRouter or the Grok API). A recipient with a webhook gets its new notes POSTed, complete
//! and signed, so the transfer is done and the agent can act on what it now holds. At most one
//! POST per recipient per `WINDOW`: a burst of notes becomes one call.
//!
//! The hub calls the URL itself, so on a hub with many tenants a webhook must be https to a
//! public address: resolved once, checked, and the connection pinned to that address, with
//! redirects off. On one owner's hub the owner's own network is allowed.

use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use devagotchi_contracts::{BridgeError, BridgeResult};
use hmac::{Hmac, KeyInit, Mac};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::{Actor, Board, inbox::InboxNote, invalid, store_error};

pub const SCHEMA: &str = "devagotchi.notes/v1";
pub(super) const KEY: &str = "wakes";
/// A recipient is called at most once in this long.
pub const WINDOW: Duration = Duration::from_secs(60);
const MAX_WAKES: usize = 20;
/// The gaps before each retry of a POST that was refused.
const RETRIES: [Duration; 2] = [Duration::from_secs(5), Duration::from_secs(30)];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wake {
    pub url: String,
    pub secret: String,
}

pub type Wakes = BTreeMap<String, Wake>;

/// When a recipient may be called: now, after a wait, or not at all because a call is
/// already waiting and will carry this note too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    Now,
    After(Duration),
    Coalesced,
}

/// One call per recipient per window; everything arriving meanwhile rides one later call.
#[derive(Default)]
pub struct Limiter {
    windows: Mutex<HashMap<String, (Instant, bool)>>,
}

impl Limiter {
    pub fn admit(&self, key: &str, now: Instant, window: Duration) -> Admit {
        let mut windows = self.windows.lock().expect("wake windows");
        match windows.get_mut(key) {
            Some((until, queued)) if now < *until => {
                if *queued {
                    Admit::Coalesced
                } else {
                    *queued = true;
                    Admit::After(*until - now)
                }
            }
            _ => {
                windows.insert(key.to_string(), (now + window, false));
                Admit::Now
            }
        }
    }

    /// The waiting call goes now: the next window starts here.
    pub fn fired(&self, key: &str, now: Instant, window: Duration) {
        let mut windows = self.windows.lock().expect("wake windows");
        windows.insert(key.to_string(), (now + window, false));
    }
}

/// Whether an address is one a tenant's webhook may reach.
pub(crate) fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

/// The address to call for `url`, or why not.
pub(crate) async fn target(
    url: &str,
    allow_private: bool,
) -> BridgeResult<(reqwest::Url, String, SocketAddr)> {
    let refuse = |why: String| invalid(format!("webhook {url:?}: {why}"));
    let parsed = reqwest::Url::parse(url).map_err(|error| refuse(error.to_string()))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(refuse("a URL with credentials in it".into()));
    }
    match parsed.scheme() {
        "https" => {}
        "http" if allow_private => {}
        _ => return Err(refuse("must be https".into())),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| refuse("has no host".into()))?
        .trim_matches(['[', ']'])
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| refuse("has no port".into()))?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|error| refuse(format!("does not resolve: {error}")))?
        .collect();
    if addresses.is_empty() {
        return Err(refuse("does not resolve".into()));
    }
    if !allow_private && addresses.iter().any(|a| !is_public(a.ip())) {
        return Err(refuse("resolves to a private address".into()));
    }
    Ok((parsed, host, addresses[0]))
}

/// POST `notes` to `wake`, signed, pinned to the address that was checked.
pub(crate) async fn send(
    wake: &Wake,
    project: &str,
    recipient: &str,
    notes: &[InboxNote],
    allow_private: bool,
) -> BridgeResult<()> {
    let (url, host, address) = target(&wake.url, allow_private).await?;
    let failed = |why: String| BridgeError::new("wake_failed", format!("{recipient}: {why}"));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .resolve(&host, address)
        .build()
        .map_err(|error| failed(error.to_string()))?;
    let body = serde_json::json!({
        "schema": SCHEMA, "project": project, "recipient": recipient, "notes": notes,
    })
    .to_string();
    let mut mac = Hmac::<Sha256>::new_from_slice(wake.secret.as_bytes()).expect("HMAC key");
    mac.update(body.as_bytes());
    let answer = client
        .post(url)
        .header("content-type", "application/json")
        .header("devagotchi-notes-schema", SCHEMA)
        .header(
            "devagotchi-signature",
            format!("sha256={}", hex::encode(mac.finalize().into_bytes())),
        )
        .body(body)
        .send()
        .await
        .map_err(|error| failed(error.without_url().to_string()))?;
    if answer.status().is_success() {
        Ok(())
    } else {
        Err(failed(format!("answered {}", answer.status())))
    }
}

/// Everything waiting for `recipient` goes to its webhook. What is taken is never sent again;
/// what is refused stays, for the next note or the recipient's next tool call.
pub(crate) async fn flush(
    board: std::sync::Arc<Board>,
    project: String,
    recipient: String,
    allow_private: bool,
) {
    let run = |board: std::sync::Arc<Board>, recipient: String| async move {
        tokio::task::spawn_blocking(move || -> BridgeResult<_> {
            let wake = board.wakes()?.remove(&recipient);
            let Some(wake) = wake else {
                return Ok(None);
            };
            let who = [recipient.clone()];
            let delivery = board.inbox_deliver(&Actor::agent("wake"), &who, &recipient, false)?;
            Ok(Some((wake, delivery)))
        })
        .await
        .map_err(|error| BridgeError::new("wake_failed", error.to_string()))?
    };
    // A burst larger than one delivery goes in a few calls; a bound, since each is a POST.
    for _ in 0..4 {
        let (wake, delivery) = match run(board.clone(), recipient.clone()).await {
            Ok(Some(found)) => found,
            Ok(None) => return,
            Err(error) => {
                eprintln!("devagotchi hub: wake {recipient}: {error}");
                return;
            }
        };
        if delivery.notes.is_empty() {
            return;
        }
        let mut sent = send(&wake, &project, &recipient, &delivery.notes, allow_private).await;
        for gap in RETRIES {
            if sent.is_ok() {
                break;
            }
            tokio::time::sleep(gap).await;
            sent = send(&wake, &project, &recipient, &delivery.notes, allow_private).await;
        }
        if let Err(error) = sent {
            eprintln!("devagotchi hub: wake {recipient}: {error}");
            return;
        }
        let (taken, more) = (delivery.notes, delivery.more);
        let marked = board.clone();
        let done = tokio::task::spawn_blocking(move || {
            marked.inbox_mark_delivered(&Actor::agent("wake"), &taken)
        })
        .await;
        if !matches!(done, Ok(Ok(()))) {
            eprintln!("devagotchi hub: wake {recipient}: could not record the delivery");
            return;
        }
        if more == 0 {
            return;
        }
    }
}

impl Board {
    pub fn wakes(&self) -> BridgeResult<Wakes> {
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(hosted.wakes(), false);
        }
        let connection = self.local()?;
        let raw: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(store_error)?;
        Ok(raw
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default())
    }

    /// Point `recipient` at `url`, or take its webhook away. A new secret comes back once.
    pub fn set_wake(
        &self,
        actor: &Actor<'_>,
        recipient: &str,
        url: Option<&str>,
    ) -> BridgeResult<Option<String>> {
        let mut wakes = self.wakes()?;
        let secret = url.map(|_| hex::encode(crate::link::random::<32>()));
        match (url, &secret) {
            (Some(url), Some(secret)) => {
                if !wakes.contains_key(recipient) && wakes.len() >= MAX_WAKES {
                    return Err(invalid(format!(
                        "a board holds at most {MAX_WAKES} webhooks"
                    )));
                }
                wakes.insert(
                    recipient.to_string(),
                    Wake {
                        url: url.to_string(),
                        secret: secret.clone(),
                    },
                );
            }
            _ => {
                wakes.remove(recipient);
            }
        }
        let detail = serde_json::json!({ "recipient": recipient, "url": url });
        if let Some(hosted) = &self.hosted {
            self.hosted_call(hosted.save_wakes(actor, &wakes, detail), true)?;
            return Ok(secret);
        }
        self.write(actor, |w| {
            w.tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![KEY, serde_json::to_string(&wakes).expect("wakes json")],
            )
            .map_err(store_error)?;
            w.event("notes.wake_set", None, None, detail)?;
            Ok(())
        })?;
        Ok(secret)
    }
}
