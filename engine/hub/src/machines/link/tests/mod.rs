//! The hub's end of the link, one subject per file. The shared fixtures are here:
//! [`test_links`] (taken as `or_skip!(test_links())`), [`hello`], and [`connect`] over
//! `Session::fake` (`machines/link/mod.rs`).

use devagotchi_contracts::{LINK_PROTOCOL_VERSION, LinkFrame, UPDATE_FAILED, link_signing_message};
use ed25519_dalek::{Signer, SigningKey};

use super::*;
use crate::machines::update;

mod admit;
mod calls;
mod container;
mod presence;
mod remote;
mod socket;
mod updates;
mod wire;

#[cfg(unix)]
const TOKENS: &str = "/tmp/tokens.json";
#[cfg(windows)]
const TOKENS: &str = r"C:\tmp\tokens.json";
const HUB: &str = "http://127.0.0.1:8787";

/// Links over a registry and an audit chain on new databases of the test Postgres, for a hub
/// that serves the link broker `gus` to `jesse`; `None` without a test Postgres.
async fn test_links() -> Option<Links> {
    let config = HubConfig::parse(
        &serde_json::json!({
            "listen": "127.0.0.1:8787",
            "public_url": HUB,
            "auth": { "mode": "dev", "tokens_file": TOKENS },
            "brokers": [ { "name": "gus", "link": true } ],
            "grants": [ { "subject": "jesse", "brokers": ["gus"] } ]
        })
        .to_string(),
    )
    .unwrap();
    let devices = crate::machines::devices::test_registry().await?;
    let audit = crate::ops::audit::test_audit().await?;
    Some(Links::new(Arc::new(config), devices, audit))
}

/// A hello from `device` signed with `key` over `hub` and `nonce`, as a link sends it.
fn hello(
    key: &SigningKey,
    hub: &str,
    device: &str,
    nonce: &str,
    code: Option<&str>,
) -> Option<LinkFrame> {
    Some(LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: device.into(),
        public_key: hex::encode(key.verifying_key().to_bytes()),
        signature: hex::encode(
            key.sign(&link_signing_message(hub, device, nonce))
                .to_bytes(),
        ),
        enroll_code: code.map(str::to_string),
        build: None,
        kind: None,
        remote_access: None,
    })
}

/// A live session for `device` as `Session::fake(id, build, container)` makes it; the
/// receivers get every call routed to it and every frame the hub sends it on its own.
fn connect(
    links: &Links,
    device: &str,
    id: u64,
    build: Option<&str>,
    container: bool,
) -> (Pending, FakeReceivers) {
    let (session, receivers) = Session::fake(id, build, container);
    let pending = session.pending.clone();
    links
        .sessions
        .lock()
        .unwrap()
        .insert(device.to_string(), session);
    (pending, receivers)
}
