//! The machine link: the frames a hub and a machine's `devagotchi link` exchange over one
//! outbound WebSocket. Re-exported flat from the crate root (`devagotchi_contracts::HubFrame`):
//! the wire's public paths are its API (G10's contracts exception).

use crate::{WireRequest, WireResponse};
use serde::{Deserialize, Serialize};

/// The machine link (plan N20): a broker on another machine is reached through one
/// outbound WebSocket that its `devagotchi-link` holds open to the hub. These frames
/// are the whole protocol. A `call` carries exactly one broker request, so the
/// broker's own validation and executable grants still decide what runs.
pub const LINK_PROTOCOL_VERSION: u32 = 1;
/// Largest WebSocket message either side accepts.
pub const MAX_LINK_MESSAGE_BYTES: usize = 1024 * 1024;

/// Hub to device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HubFrame {
    /// The first frame on every connection. The device answers with a signed `hello`.
    Challenge {
        nonce: String,
        hub: String,
    },
    Welcome {
        device: String,
    },
    Refused {
        code: String,
        message: String,
    },
    Call {
        id: u64,
        request: WireRequest,
    },
    /// Update this machine to the hub's build `version`: wait for its running jobs unless
    /// `force`, then download, check and install it. The hub sends it only to a device
    /// whose hello carried `build`; an older device would refuse an unknown frame.
    Update {
        version: String,
        force: bool,
    },
}

/// Device to hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkFrame {
    /// Proves the device key over the challenge. `enroll_code` is present only on the
    /// first connection, when the hub binds this key to the device name.
    Hello {
        version: u32,
        device: String,
        /// Ed25519 public key, 32 bytes, hex.
        public_key: String,
        /// Ed25519 signature over `link_signing_message`, 64 bytes, hex.
        signature: String,
        #[serde(default)]
        enroll_code: Option<String>,
        /// The build this device runs (`devagotchi --version`'s name: `main-<sha7>`, a
        /// release version, or `dev`). An older device leaves it out.
        #[serde(default)]
        build: Option<String>,
        /// `container` for a machine in a container, which updates with its image and never
        /// by itself; left out on any other machine (and by an older device).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    Result {
        id: u64,
        response: WireResponse,
    },
    Heartbeat,
    /// Where an update the hub asked for stands: `waiting_for_jobs`, `downloading`,
    /// `installing`, `done` or `failed`, with `detail` saying why when it failed.
    UpdateStatus {
        state: String,
        #[serde(default)]
        detail: Option<String>,
    },
}

/// The states a [`LinkFrame::UpdateStatus`] reports, in the order an update passes them.
pub const UPDATE_STATES: [&str; 5] = [
    "waiting_for_jobs",
    "downloading",
    "installing",
    "done",
    "failed",
];

/// The exact bytes a device signs: protocol, the hub's public URL, the device name and
/// the hub's nonce, so a signature cannot be replayed to another hub, as another
/// device, or on another connection.
pub fn link_signing_message(hub: &str, device: &str, nonce: &str) -> Vec<u8> {
    format!("devagotchi-link/{LINK_PROTOCOL_VERSION}\n{hub}\n{device}\n{nonce}").into_bytes()
}

#[cfg(test)]
mod tests;
