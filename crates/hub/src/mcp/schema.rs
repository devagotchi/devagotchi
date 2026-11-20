//! What the hub's own tools take and return, as their JSON schemas describe them: a machine, the routed operation inputs, adding and updating a machine, and `whoami`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One machine as a person sees it on their page and in `/api/v1/me`, and as an AI sees it
/// through the `machines` tool. The hub is its one owner.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Machine {
    /// The name to pass as `machine` to the machine tools.
    pub name: String,
    /// `local`: a broker on the hub's own host. `linked`: a machine that dials in.
    pub kind: &'static str,
    /// `online` (a linked machine's link is connected, a local broker's endpoint answers; that
    /// a linked machine's broker answers too, `machine_inspect` asks), `offline` (off, asleep,
    /// or its link is not running), `not_enrolled` (a linked machine that never enrolled) or
    /// `revoked`.
    pub status: &'static str,
    /// When the machine enrolled, in seconds since the Unix epoch (linked machines).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrolled_at: Option<u64>,
    /// The account added this machine itself ("Add a machine"); only it reaches the
    /// machine, and it can revoke it. Left out when false: `skip_serializing_if` keeps it out
    /// of the output schema's `required` (a client that checks refuses `machines`
    /// otherwise), and `default` lets an older answer without it deserialize.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub owned: bool,
    /// The build the machine runs, as its link said when it connected (online linked
    /// machines whose link says it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// `updating` while an update to the hub's build runs on it, or `failed: <reason>, back
    /// on <build>` after one failed. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Machines {
    /// Every machine this identity can work on, each with its status.
    pub machines: Vec<Machine>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineAdd {
    /// Short name for the machine, `[a-z0-9][a-z0-9-]{0,62}`.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineUpdate {
    /// The machine's name, as `machines` lists it.
    pub machine: String,
    /// Update now even while the machine runs jobs; they end when its broker restarts.
    #[serde(default)]
    pub force: bool,
    /// How long to wait for the machine to come back, in seconds (default 50, at most 300).
    #[serde(default)]
    pub wait_seconds: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MachineUpdated {
    pub machine: String,
    /// `done`, `failed`, or where it still is when the wait ended: `requested`,
    /// `waiting_for_jobs`, `downloading` or `installing`.
    pub state: String,
    /// Why it failed, or `already on <build>` when there was nothing to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The build the machine runs now, while it is online.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MachineAdded {
    pub name: String,
    pub code: String,
    pub expires_at: u64,
    pub linux: String,
    pub windows: String,
    /// Compose file URL. The code is the same single-use join token.
    pub docker: String,
    /// Host installer for Proxmox or Incus (not inside a guest).
    pub host: String,
    /// Tarball plus `devagotchi link enroll`, code on stdin.
    pub manual: String,
}

/// An organisation the caller belongs to, as `whoami` lists it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OrgRole {
    /// What boards of it are named by: `<org>/<project>`.
    pub org: String,
    pub name: String,
    /// owner, admin, member, agent or viewer: what the caller may do on its projects.
    pub role: crate::tenancy::orgs::Role,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WhoAmI {
    /// The account ID (`sub`) the identity provider proved, never a caller-supplied name.
    /// Grants are written against this.
    pub subject: String,
    pub via: String,
    /// The login name the identity provider reported, to tell your accounts apart.
    /// Grants never use it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The org (tenant) the identity provider proved this account belongs to. Projects
    /// bound by any member belong to it and every member works them (B13).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// Unix seconds until an agent-opened account must be claimed. Absent once claimed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provisional_until: Option<u64>,
    /// The agent seat on this token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Code the person uses to claim this account. Shown while it is provisional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_code: Option<String>,
    /// Time left is under a seventh of the live provisional ttl (at least an hour).
    /// Left out when false: `skip_serializing_if` keeps it out of the schema's `required`,
    /// since clients check `structuredContent` against the output schema (the MCP
    /// TypeScript SDK refuses the call otherwise); `default` lets an older answer
    /// deserialize.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub provisional_warning: bool,
    /// `{public_url}/claim?code=…` while the account is provisional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_url: Option<String>,
    /// The tenant this account belongs to on a hub with its own accounts (`t-<uuid>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// On a hub with its own accounts: `admitted`, or `waiting` while the tenant is on the
    /// early-access list. While it waits, every other tool answers with `access_note`
    /// instead of acting: tell the person, once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
    /// When the tenant joined the early-access list, in Unix seconds, while it waits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<u64>,
    /// What the hub says to a waiting tenant: when it joined, that an email comes when it
    /// is in, and its account page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_note: Option<String>,
    /// The plan the tenant is on (a plan id from the site), on a hosted hub.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Where its person secures this account, while nobody has: until then it is bound to
    /// the browser that made it and the AI apps it approved, nothing can recover it, and it
    /// ends at `provisional_until` if unused. Tell the person, and give them this link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure_url: Option<String>,
    /// Whether this is the hub's configured owner, who reaches every broker and board.
    pub owner: bool,
    /// Whether this subject acts as a person on boards (force-release, moving tickets by
    /// hand): the owner, or a grant marked human. Agents are never people.
    pub human: bool,
    /// Brokers this subject holds a grant for, by name.
    pub brokers: Vec<String>,
    /// Project boards this subject may work on, by project name: its org's (or, with no
    /// org, the account's own; `board_bind`) first, then the shared ones it holds a grant
    /// for, then its organisations' projects as `<org>/<project>`.
    pub boards: Vec<String>,
    /// The organisations of this hub the subject belongs to, with its role in each.
    /// Left out when empty: `skip_serializing_if` keeps it out of the schema's `required`,
    /// and `default` lets an older answer deserialize.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orgs: Vec<OrgRole>,
}
