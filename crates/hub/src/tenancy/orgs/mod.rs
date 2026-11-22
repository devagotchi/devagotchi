//! Organisations, their members and teams, and who works which project (increment 114).
//!
//! An organisation is a tenant of its own (`tenancy_tenants.kind = 'organisation'`, migration
//! 0017). Its projects are hosted boards owned by `org-<tenant id>` (`owner_of`), named
//! `<slug>/<project>` wherever a caller names a board. Every row about an organisation is
//! read and written in a transaction scoped to its tenant, so row-level security holds on
//! Postgres; the three questions that cross tenants (which organisations a subject belongs
//! to, which projects it was added to, which tenant an invite belongs to) are answered by
//! `SECURITY DEFINER` functions that return those ids and nothing else.
//!
//! Roles, highest first: owner, admin, member, agent, viewer (`Role`). An organisation's
//! role applies to each of its projects whose visibility is `org`; on a `members` project
//! only admins and owners reach it through the organisation. A project's own members (a
//! subject, or a team of the organisation) hold the role their row names there; the higher
//! of the two counts (`project_role`). What a role may do is said once, on `Role`.
//!
//! Who may change what: an admin manages members, teams, projects and invites, never above
//! their own role; only an owner makes or unmakes an owner, and the last owner stays. Every
//! change lands on the tenant's own hash chain (`tenancy_events`).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{identity::accounts::tenants::valid_tenant_id, store::Store};

pub(crate) mod api;
mod checks;
mod invites;
mod members;
mod org;
mod project_roles;
pub(crate) use project_roles::effective as effective_role_in;
mod projects;
mod roles;
mod teams;
#[cfg(test)]
mod tests;

/// How long an invite link works.
pub const INVITE_TTL_SECS: i64 = 7 * 24 * 3600;

/// What a subject is on an organisation or a project. Ordered: a higher role may do all a
/// lower one may.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Reads the boards: lists, tickets, events. Changes nothing.
    Viewer,
    /// Works tickets as an agent: claims, notes, completes, hands off. Never a person: no
    /// force-release, no moving by hand, no approvals.
    Agent,
    /// A person on the board: an agent's work, and force-release, moving by hand, approvals.
    Member,
    /// A member who also configures projects and manages members, teams and invites.
    Admin,
    /// An admin who also makes and unmakes owners. An organisation keeps at least one.
    Owner,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Owner,
        Role::Admin,
        Role::Member,
        Role::Agent,
        Role::Viewer,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Agent => "agent",
            Role::Viewer => "viewer",
        }
    }

    /// Changes tickets (claims, notes, completes, creates).
    pub fn may_work(self) -> bool {
        self >= Role::Agent
    }

    /// Acts as a person: force-release, moving by hand, deciding approvals.
    pub fn is_person(self) -> bool {
        self >= Role::Member
    }

    /// Configures a project (settings, workflow, visibility) and manages who works it.
    pub fn may_configure(self) -> bool {
        self >= Role::Admin
    }
}

/// An organisation, as a caller sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Org {
    /// Its tenant (`t-<uuid>`): where its rows live. Never a name.
    pub id: String,
    /// What a caller names it by: `[a-z0-9][a-z0-9-]{0,62}`, unique on this hub.
    pub slug: String,
    pub name: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// One member of an organisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Member {
    pub subject: String,
    pub role: Role,
    pub added_by: String,
    pub added_at: i64,
}

/// A team of an organisation and who is on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Team {
    pub team: String,
    pub name: String,
    pub members: Vec<String>,
}

/// Who a project row names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Principal {
    Subject,
    Team,
}

impl Principal {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "subject" => Some(Self::Subject),
            "team" => Some(Self::Team),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Team => "team",
        }
    }
}

/// One row of a project's own members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMember {
    pub kind: Principal,
    pub principal: String,
    pub role: Role,
}

/// Who reaches a project through its organisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Every member of the organisation, at their organisation role.
    Org,
    /// Only its own members, and the organisation's admins and owners.
    Members,
}

impl Visibility {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "org" => Some(Self::Org),
            "members" => Some(Self::Members),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Members => "members",
        }
    }
}

/// A project of an organisation, for its listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    pub project: String,
    pub visibility: Visibility,
    pub members: Vec<ProjectMember>,
}

/// An invite that has not been used, revoked or let run out. Its token is never kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Invite {
    /// The first 12 hex digits of the token's SHA-256: what revoking names it by.
    pub id: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub invited_by: String,
    pub expires_at: i64,
}

/// A new invite: the one time its token is seen.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NewInvite {
    pub id: String,
    pub token: String,
    pub role: Role,
    pub expires_at: i64,
}

/// Where a project's rows live and whose board it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectRef {
    pub tenant: String,
    pub owner: String,
    pub project: String,
}

/// The board owner an organisation's projects belong to.
pub fn owner_of(org_tenant: &str) -> String {
    format!("org-{org_tenant}")
}

/// The organisation tenant a board owner names, when it names one (`org-t-<uuid>`). An
/// identity provider's org (`org-<id>`, B13) is not one: its ids are never `t-` UUIDs.
pub fn tenant_of_owner(owner: &str) -> Option<&str> {
    owner
        .strip_prefix("org-")
        .filter(|tenant| valid_tenant_id(tenant))
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The organisations of one hub, in its store.
#[derive(Clone)]
pub struct Orgs {
    store: Store,
    /// Unix seconds; tests move it.
    clock: Clock,
}

impl Orgs {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            clock: Arc::new(|| crate::agenthub::system_clock()() / 1000),
        }
    }

    #[cfg(test)]
    pub fn with_clock(store: Store, clock: Clock) -> Self {
        Self { store, clock }
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }
}
