//! The machine link: the frames a hub and a machine's `devagotchi link` exchange over one
//! outbound WebSocket. Re-exported flat from the crate root (`devagotchi_contracts::HubFrame`):
//! the wire's public paths are its API (G10's contracts exception).

use std::time::Duration;

use crate::{WireRequest, WireResponse};
use serde::{Deserialize, Serialize};

/// The machine link (plan N20): a broker on another machine is reached through one
/// outbound WebSocket that its `devagotchi link run` holds open to the hub. These frames
/// are the whole protocol. A `call` carries exactly one broker request, so the
/// broker's own validation and executable grants still decide what runs.
///
/// Never bump it to ship a feature: a hub that refuses a machine's version strands it until
/// someone reinstalls it by hand (`.ai/patterns/link.md` rule 2).
pub const LINK_PROTOCOL_VERSION: u32 = 1;
/// Largest WebSocket message either side accepts.
pub const MAX_LINK_MESSAGE_BYTES: usize = 1024 * 1024;

/// How often the link says it is alive (a `heartbeat` frame and a WebSocket Ping).
pub const LINK_HEARTBEAT: Duration = Duration::from_secs(20);
/// Silence after which either side gives the connection up: the hub marks the machine
/// offline, the link reconnects. Three heartbeats fit inside it.
pub const LINK_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const _: () = assert!(LINK_HEARTBEAT.as_secs() * 3 <= LINK_IDLE_TIMEOUT.as_secs());
/// Longest either side waits for one frame to be written before it gives the connection up.
pub const LINK_SEND_TIMEOUT: Duration = Duration::from_secs(10);
const _: () = assert!(LINK_SEND_TIMEOUT.as_secs() < LINK_HEARTBEAT.as_secs());

/// The `Hello.kind` of a machine in a container, and the value of
/// `DEVAGOTCHI_MACHINE_KIND` that makes a link say it. Such a machine updates with its image,
/// never by itself, so the hub never sends it `update`.
pub const MACHINE_KIND_CONTAINER: &str = "container";
/// The handshake refusal code that means "try again later": the link reconnects. Every other
/// refusal is permanent and the link exits 2, so supervisors stop.
pub const REFUSAL_UNAVAILABLE: &str = "unavailable";

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
        /// release version, or `dev`). Left out on enroll and by an older device; links
        /// released before it was skipped when None send `"build":null`, which still parses.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build: Option<String>,
        /// [`MACHINE_KIND_CONTAINER`] for a machine in a container, which updates with its
        /// image and never by itself; left out on any other machine (and by an older device).
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

/// An update waits for the broker's running jobs to end (not when forced).
pub const UPDATE_WAITING: &str = "waiting_for_jobs";
/// The installer and this machine's archive are being fetched from the hub and checked.
pub const UPDATE_DOWNLOADING: &str = "downloading";
/// Handed to the platform's updater, which outlives the link it replaces.
pub const UPDATE_INSTALLING: &str = "installing";
/// The machine runs the build it was asked to (or already did).
pub const UPDATE_DONE: &str = "done";
/// It did not happen; `detail` says why.
pub const UPDATE_FAILED: &str = "failed";
/// The states a [`LinkFrame::UpdateStatus`] reports, in the order an update passes them.
pub const UPDATE_STATES: [&str; 5] = [
    UPDATE_WAITING,
    UPDATE_DOWNLOADING,
    UPDATE_INSTALLING,
    UPDATE_DONE,
    UPDATE_FAILED,
];
/// Whether an update in `state` has ended: nothing more will be reported about it.
pub fn update_finished(state: &str) -> bool {
    state == UPDATE_DONE || state == UPDATE_FAILED
}

/// A build name the hub may hand out and a machine may install (`main-<sha7>`, `ci-<sha7>`,
/// `X.Y.Z`, `dev`): it names directories and goes on an installer's command line, so it is
/// plain ASCII, starts with a letter or digit, and holds at most 64 of `[A-Za-z0-9._-]`.
pub fn valid_build_name(name: &str) -> bool {
    name.len() <= 64
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// The exact bytes a device signs: protocol, the hub's public URL, the device name and
/// the hub's nonce, so a signature cannot be replayed to another hub, as another
/// device, or on another connection.
pub fn link_signing_message(hub: &str, device: &str, nonce: &str) -> Vec<u8> {
    format!("devagotchi-link/{LINK_PROTOCOL_VERSION}\n{hub}\n{device}\n{nonce}").into_bytes()
}

#[cfg(test)]
mod tests;
