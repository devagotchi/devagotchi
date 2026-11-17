//! The board's MCP face: `board_*` tools on the hub's `/mcp`, one per protocol verb,
//! for agents on any machine. The command line (`cli.rs`) and these tools open the
//! same store, so an agent on the hub's host and one across the world see one board.
//!
//! Identity is the hub's: the actor is the authenticated subject, a person only when
//! the config says so (`grants[].human`, or the owner). A fence may be omitted on a
//! held ticket; the hub then supplies the one it handed that subject at claim or
//! heartbeat, the way the command line's claim cache does. The store still checks it,
//! so a lost lease is refused either way. Every call lands on the audit chain under
//! the project's name.
//!
//! Two kinds of board answer to a project name. A configured board (`boards` in the hub
//! config) is a directory on this host that grants open to several accounts. An account
//! board (D23, packet 15) is the hosted tier's: `board_bind` makes one for the calling
//! account under `account_boards.dir`, reached by that account alone, counted against the
//! free limit. On either, a caller may name its seat (`X-Devagotchi-Seat: claude`): the
//! account's own agent on that machine. On the account's own board the seat is the
//! actor outright; on a shared board it is `<account>/<seat>`, so no account can pose
//! as another's seat. A seat is never a person.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{Arc, Mutex},
};

use rmcp::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Board;
use crate::tenancy::orgs;

/// The header a caller names its seat in: the account's own agent on that machine.
pub const SEAT_HEADER: &str = "x-devagotchi-seat";

/// The owner every configured board has in the store (`hosted::registry`, `crate::orgs`).
pub(crate) const SHARED_OWNER: &str = "@shared";

/// The boards a hub serves, opened on first use and kept open; plus the last fence
/// handed to each subject per ticket, so a caller may omit it.
pub struct Boards {
    /// The hub's boards, in its Postgres (`registry.rs`): account, organisation and
    /// configured ones alike.
    hosted: super::registry::Registry,
    /// Organisations, their members and who works which project (`crate::orgs`): in the
    /// hub's store, beside the hosted boards.
    orgs: Option<orgs::Orgs>,
    /// What people told the hub they want, kept beside the organisations.
    preferences: Option<crate::identity::PreferenceStore>,
    /// The export of these boards' events to ClickHouse (`crate::warehouse`), when the hub
    /// has one: from the config at start, or from the settings page at any time.
    warehouse: std::sync::RwLock<Option<Arc<crate::ops::warehouse::Warehouse>>>,
    /// The hub's signal sinks (`crate::signals`), set once at start.
    sinks: std::sync::OnceLock<Vec<Arc<crate::notify::signals::Sink>>>,
    /// The boards the hub's config names (`boards`), shared by grant.
    configured: BTreeSet<String>,
    accounts: Option<Accounts>,
    /// The site a new board's getting-started ticket comes from, and the hub's address
    /// in its steps.
    site: Arc<crate::site::SiteCell>,
    public_url: String,
    open: Mutex<BTreeMap<String, Arc<Board>>>,
    fences: Mutex<HashMap<(String, String, String), i64>>,
    /// Told of every change any open board commits; the lease reaper waits on it.
    activity: Arc<tokio::sync::Notify>,
    /// When each caller's inbox was last looked at.
    inbox_checks: Mutex<HashMap<String, std::time::Instant>>,
    wake_limits: Arc<super::wake::Limiter>,
}

/// Who may hold how many account boards.
struct Accounts {
    free_limit: usize,
    unlimited: HashSet<String>,
    /// The hub's owner, who has no limit; in hub mode it can be learned after start.
    owner: std::sync::Arc<std::sync::OnceLock<String>>,
}

/// What `board_bind` says.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Bound {
    pub project: String,
    /// False when the board already existed: binding is idempotent.
    pub created: bool,
    /// Every board this account may work on now, account boards first.
    pub boards: Vec<String>,
    /// How many account boards this account may hold here; absent for no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Which kind of board answered a project name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The calling account's own board.
    Account,
    /// A board the caller's org owns: every member of the org works it (B13).
    Org,
    /// A board from the hub config, shared by grant.
    Shared,
    /// A project of an organisation of this hub (`crate::orgs`), named `<slug>/<project>`:
    /// what the caller may do there is its role, from the store.
    Workspace,
}

/// The board tools that change nothing: a viewer may call these and no other.
const READ_ONLY_TOOLS: &[&str] = &[
    "board_attach",
    "board_issue_context",
    "board_prime",
    "board_ready",
    "board_list",
    "board_get",
    "board_events",
    "board_wait",
    "board_inbox",
    "board_verify",
    "approval_wait",
];

mod access;
mod approvals;
mod boards;
mod calls;
mod notices;
mod refusals;
mod schema;
mod tools;

pub use refusals::BoardError;
use refusals::{given_or, mcp_session, refuse};
pub use schema::*;

/// What every board tool returns: its result, or a refusal that says what to call next.
type BoardOut<T> = Result<Json<T>, Json<BoardError>>;

#[cfg(test)]
mod tests;
