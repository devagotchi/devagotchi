//! The account this machine signs in with: `devagotchi login`, `logout`, `whoami` (D22).
//!
//! Sign-in is OAuth device authorization (RFC 8628): the terminal shows a short code and a
//! URL, the person opens it anywhere — a phone will do — and the tokens arrive here. No
//! browser on this machine, no local port, no secret in the binary: the CLI is a public
//! client. It signs in at the hub `--hub` names (or `DEVAGOTCHI_HUB`, or the hub a previous
//! login saved). Nothing here picks a hub for you. It finds the endpoints in the hub's
//! RFC 8414 metadata: a hub with its own accounts
//! (`auth.mode hub`) shows the code on its `/device` page, approved by a person signed in
//! with their passkey; a hub that opens accounts for agents (`auth.mode devagotchi`) gives
//! one at once, claimed later. `--issuer` signs in at Nomen directly. The tokens rest in one
//! owner-only file under the config directory (`account.json`, 0600, written whole and
//! renamed into place); nothing else reads it but this binary. A hub checks the access
//! token the usual way, so a stolen file is a stolen token and nothing more, and `logout`
//! revokes it where it was issued.
//!
//! `login --local` records the other choice D22 allows: this machine runs without an
//! account. Until one of the two is on file, `devagotchi start` says so once per run.
//!
//! `client` is one MCP session with a hub as the account; `front` carries that into an
//! agent's stdio session (`devagotchi mcp --hub`); the board's remote face calls through it.
//!
//! Offline grace (D22, PROPOSED): the access token is refreshed when within five minutes
//! of expiry; when Nomen cannot be reached, the account on file stays "signed in (offline)"
//! for seven days past expiry, then "expired", and nothing here deletes it — only `logout`
//! does. What a consumer does in each state is the consumer's call (board sync, packet 15).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

mod cli;
pub mod client;
mod file;
pub mod front;
mod hub;
mod issuer;
#[cfg(test)]
mod tests;
mod wire;

pub use cli::cli;
pub use file::{current, read, remove, signed_in, write};
pub use issuer::Issuer;

/// The public client id `devagotchi login --issuer` sends unless `--client-id` names another.
/// It is not the hub's built-in client (`devagotchi-cli`): a saved login is refreshed on the
/// path that matches its client.
pub const DEFAULT_CLIENT_ID: &str = "devagotchi";

pub const FILE_NAME: &str = "account.json";

/// Refresh when the access token is this close to expiry.
const REFRESH_AHEAD: Duration = Duration::from_secs(5 * 60);

/// Past expiry, the account on file still counts as signed in (offline) for this long.
pub const OFFLINE_GRACE: Duration = Duration::from_secs(7 * 24 * 3600);

/// A device code the person never confirms is given up on after this long at most.
const MAX_WAIT: Duration = Duration::from_secs(15 * 60);

/// What is on file: an account, or the choice to run without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stored {
    Local { chosen_at: u64 },
    Account(Account),
}

/// A signed-in account. The tokens are never printed; `Debug` shows the rest.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub issuer: String,
    pub client_id: String,
    /// The Nomen account ID (`sub`), which never changes; the hub grants by it.
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The hub this login was for, when sign-in went through a hub. `devagotchi mcp --hub`
    /// uses it when no hub is named again. An issuer login leaves it empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub: Option<String>,
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
    pub signed_in_at: u64,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("subject", &self.subject)
            .field("username", &self.username)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// The account's standing at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The access token is good for a while.
    Fresh,
    /// Within `REFRESH_AHEAD` of expiry, or past it: refresh before use.
    Refresh,
    /// Past expiry and no refresh succeeded; still the account on file, for now.
    Offline { grace_ends_at: u64 },
    /// Past the grace: sign in again.
    Expired,
}

impl Account {
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// An account as a test would have it: a token that is good until `expires_at`.
    #[cfg(test)]
    pub(crate) fn for_tests(issuer: &str, subject: &str, token: &str, expires_at: u64) -> Self {
        Self {
            issuer: issuer.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            subject: subject.into(),
            username: None,
            email: None,
            hub: None,
            access_token: token.into(),
            refresh_token: None,
            expires_at,
            signed_in_at: unix_now(),
        }
    }

    pub fn standing(&self, now: u64) -> Standing {
        if now + REFRESH_AHEAD.as_secs() < self.expires_at {
            Standing::Fresh
        } else if now < self.expires_at {
            Standing::Refresh
        } else if now < self.expires_at + OFFLINE_GRACE.as_secs() {
            Standing::Offline {
                grace_ends_at: self.expires_at + OFFLINE_GRACE.as_secs(),
            }
        } else {
            Standing::Expired
        }
    }

    /// `username` when Nomen gave one, else the account ID.
    pub fn display_name(&self) -> &str {
        self.username.as_deref().unwrap_or(&self.subject)
    }
}

/// Why a sign-in or refresh did not produce an account.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The person declined, the code expired, or Nomen refused the client or the grant.
    Refused(String),
    /// Nomen could not be reached or did not answer as an issuer does.
    Unavailable(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Refused(why) => write!(f, "refused: {why}"),
            Refusal::Unavailable(why) => write!(f, "{why}"),
        }
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
