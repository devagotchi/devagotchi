//! The hub's end of the machine link (plan N20). A broker on another machine is reached
//! through one WebSocket its `devagotchi link` dials out to `/link`. The device proves
//! its enrolled Ed25519 key by signing a fresh challenge; after that the hub sends one
//! `call` frame per routed request and waits for the matching `result`.
//!
//! `/link` is reachable without a bearer token, so everything before a valid hello is
//! bounded: concurrent handshakes, the hello's size, and how often refusals are audited.
//! A peer learns only that it was refused; the reason goes to the operator's audit.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock, atomic::AtomicU64},
    time::Duration,
};

use devagotchi_contracts::{BridgeError, HubFrame, WireRequest, WireResponse};
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, mpsc, oneshot};

use crate::{
    config::HubConfig,
    machines::{devices::Devices, update::Update},
    ops::audit::Audit,
};

mod calls;
mod enroll;
mod presence;
mod registry;
mod serve;
#[cfg(test)]
mod tests;
mod updates;

/// Handshakes in flight at once; more are refused with 503 before the upgrade.
const MAX_HANDSHAKES: usize = 32;

/// Calls queued for one device beyond those in flight; more answer `link_busy`.
const CALL_QUEUE: usize = 64;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<WireResponse>>>>;

#[derive(Clone)]
struct Session {
    id: u64,
    calls: mpsc::Sender<(u64, WireRequest)>,
    pending: Pending,
    /// The build the device's hello said it runs; None from a link too old to say.
    build: Option<String>,
    /// Frames the hub sends on its own: `update` (`machine_update.rs`).
    control: mpsc::Sender<HubFrame>,
}

/// Removes a call's waiter if the caller stops waiting for any reason (cancelled, timed
/// out, link gone), so abandoned calls never accumulate.
struct Waiter {
    pending: Pending,
    id: u64,
}

impl Drop for Waiter {
    fn drop(&mut self) {
        self.pending.lock().expect("pending").remove(&self.id);
    }
}

/// Refusals are audited at most once a second; the rest are counted into the next row.
#[derive(Default)]
struct RefusalLog {
    last_second: u64,
    suppressed: u64,
}

pub struct Links {
    config: Arc<HubConfig>,
    devices: Arc<Devices>,
    audit: Arc<Audit>,
    sessions: Mutex<HashMap<String, Session>>,
    handshakes: Arc<Semaphore>,
    refusals: Mutex<RefusalLog>,
    next: AtomicU64,
    /// Tells an account's phones when a machine it added joins (increment 29a).
    push: Option<Arc<crate::notify::push::Push>>,
    /// Whether the account that issued a code may enroll a machine with it now.
    gate: Option<EnrollGate>,
    /// Where an account's first machine is counted (`analytics::funnel`), on a hub that counts.
    analytics: Option<Arc<crate::ops::analytics::Analytics>>,
    /// Woken when a machine connects or drops: what a workflow waits on (`workflows.rs`).
    changes: Option<Arc<tokio::sync::Notify>>,
    /// The build this hub hands out (`downloads.rs`), which machines update to.
    offered: Option<String>,
    /// Each machine's latest update, and who waits on one (`machine_update`).
    updates: Mutex<HashMap<String, Update>>,
    update_changed: tokio::sync::Notify,
    /// Each machine's last [`RECENT_JOBS`] operations routed through its link, newest first,
    /// in memory only (the phone app's machine page, `web/app/machine.rs`).
    jobs: Mutex<HashMap<String, VecDeque<Job>>>,
    /// When each machine's link last went away, since this hub started, and which time
    /// that was (a number no other drop has).
    seen: Mutex<HashMap<String, (u64, u64)>>,
    /// How long a machine stays offline before its account's phones hear it (P1d).
    offline_after: Duration,
    /// Who turned pushes about their machines off (set once, at start).
    preferences: OnceLock<crate::identity::PreferenceStore>,
    /// The pushes about machines this hub made, for the tests to read.
    #[cfg(test)]
    pushed: Mutex<Vec<(Vec<String>, serde_json::Value)>>,
}

/// A machine offline this long is pushed to its account's phones, once per time it went.
const OFFLINE_PUSH_AFTER: Duration = Duration::from_secs(600);

/// How many routed operations the hub keeps per machine.
pub const RECENT_JOBS: usize = 20;

/// One operation the hub routed to a machine, as its page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub operation_id: String,
    /// When it started, in Unix seconds.
    pub at: u64,
    /// The command line's first 80 characters (or the executable and its arguments).
    pub command: String,
    /// The operation's state as the broker last answered it (`running`, `succeeded`, …).
    pub state: String,
    pub exit_code: Option<i32>,
}

/// What asking a machine to update came to.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateAsked {
    /// The machine already runs the build this hub hands out.
    AlreadyOn(String),
    /// `update` went to the machine.
    Sent,
}

/// Whether an account may enroll a machine now: false while its tenant waits to be let in
/// (`tenancy/gate.rs`), or when that cannot be told.
pub type EnrollGate =
    Arc<dyn Fn(String) -> futures_util::future::BoxFuture<'static, bool> + Send + Sync>;

/// A device the hello proved, and the account it just joined for, if it enrolled now
/// with an account's code.
#[derive(Debug)]
struct Admitted {
    device: String,
    key: String,
    joined_for: Option<String>,
    build: Option<String>,
}

fn offline(device: &str) -> BridgeError {
    BridgeError::new(
        "broker_offline",
        format!(
            "{device} is enrolled but its link is not connected; the machine is off, asleep or its `devagotchi link` is not running"
        ),
    )
}

/// A short, stable fingerprint of a device key for the audit log.
fn fingerprint(public_key: &str) -> String {
    hex::encode(&Sha256::digest(public_key.as_bytes())[..8])
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("operating system random source");
    bytes
}

impl Links {
    pub fn new(config: Arc<HubConfig>, devices: Arc<Devices>, audit: Arc<Audit>) -> Self {
        Self {
            config,
            devices,
            audit,
            sessions: Mutex::new(HashMap::new()),
            handshakes: Arc::new(Semaphore::new(MAX_HANDSHAKES)),
            refusals: Mutex::new(RefusalLog::default()),
            next: AtomicU64::new(1),
            push: None,
            gate: None,
            analytics: None,
            changes: None,
            offered: None,
            updates: Mutex::new(HashMap::new()),
            update_changed: tokio::sync::Notify::new(),
            jobs: Mutex::new(HashMap::new()),
            seen: Mutex::new(HashMap::new()),
            offline_after: OFFLINE_PUSH_AFTER,
            preferences: OnceLock::new(),
            #[cfg(test)]
            pushed: Mutex::new(Vec::new()),
        }
    }

    /// Ask `preferences` before a machine pushes to a person.
    pub fn set_preferences(&self, preferences: crate::identity::PreferenceStore) {
        let _ = self.preferences.set(preferences);
    }

    /// Machines update to `version`, the build this hub hands out, when there is one.
    pub fn offering(mut self, version: Option<String>) -> Self {
        self.offered = version;
        self
    }

    /// Wake `changes`' waiters whenever a machine connects or drops.
    pub fn signalling(mut self, changes: Arc<tokio::sync::Notify>) -> Self {
        self.changes = Some(changes);
        self
    }

    fn changed(&self) {
        if let Some(changes) = &self.changes {
            changes.notify_waiters();
        }
    }

    /// Count each account's first machine in `analytics`.
    pub fn counting(mut self, analytics: Option<Arc<crate::ops::analytics::Analytics>>) -> Self {
        self.analytics = analytics;
        self
    }

    /// Ask `gate` before a machine enrolls with an account's code.
    pub fn gated(mut self, gate: EnrollGate) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Push "your machine joined" to the owner's phones when a machine an account added
    /// enrolls.
    pub fn notifying(mut self, push: Option<Arc<crate::notify::push::Push>>) -> Self {
        self.push = push;
        self
    }
}
