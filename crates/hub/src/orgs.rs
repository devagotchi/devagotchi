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

use base64::Engine as _;
use devagotchi_contracts::{BridgeError, BridgeResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Row};

use crate::{
    accounts::tenants::{new_tenant_id, record_in, valid_tenant_id},
    config::{valid_name, valid_subject},
    store::{Backend, Failed, Store, transaction},
};

pub(crate) mod api;
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

/// The SHA-256 of an invite token, as stored.
fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn refused(code: &str, message: impl Into<String>) -> Failed {
    Failed::Refused(BridgeError::new(code, message))
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}

fn check_name(what: &str, name: &str) -> BridgeResult<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{what} {name:?} is not [a-z0-9][a-z0-9-]{{0,62}}"
        )))
    }
}

fn check_subject(subject: &str) -> BridgeResult<()> {
    valid_subject(subject).map_err(invalid)
}

fn check_title(name: &str) -> BridgeResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(invalid(
            "a name is 1 to 100 characters, without control characters",
        ));
    }
    Ok(name.to_string())
}

fn role_of(text: String) -> Result<Role, Failed> {
    Role::parse(&text)
        .ok_or_else(|| refused("board_store", format!("unknown role {text:?} in the store")))
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
            clock: Arc::new(|| crate::board::system_clock()() / 1000),
        }
    }

    #[cfg(test)]
    pub fn with_clock(store: Store, clock: Clock) -> Self {
        Self { store, clock }
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    // ---- organisations ------------------------------------------------------------

    /// Make an organisation named `slug`, with `by` as its owner.
    pub async fn create(&self, by: &str, slug: &str, name: &str) -> BridgeResult<Org> {
        check_subject(by)?;
        check_name("organisation", slug)?;
        let name = check_title(name)?;
        let id = new_tenant_id(crate::link::random::<16>());
        let now = self.now();
        transaction!(&self.store, |connection| {
            let taken = sqlx::query("SELECT 1 FROM tenancy_tenants WHERE slug = $1")
                .bind(slug)
                .fetch_optional(&mut *connection)
                .await?
                .is_some();
            if taken {
                return Err(refused(
                    "name_taken",
                    format!("{slug:?} is taken on this hub; pick another name"),
                ));
            }
            sqlx::query(
                "INSERT INTO tenancy_tenants (id, home, slug, name, kind, created_at)
                 VALUES ($1, 0, $2, $3, 'organisation', $4)",
            )
            .bind(&id)
            .bind(slug)
            .bind(&name)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            self.store.scope(connection, &id).await?;
            sqlx::query(
                "INSERT INTO tenancy_access (tenant_id, state, admitted_at, admitted_by)
                 VALUES ($1, 'admitted', $2, $3)",
            )
            .bind(&id)
            .bind(now)
            .bind(format!("organisation made by {by}"))
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO org_members (tenant_id, subject, role, added_by, added_at)
                 VALUES ($1, $2, 'owner', $2, $3)",
            )
            .bind(&id)
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            record_in(
                connection,
                &id,
                by,
                "org.created",
                &format!("{slug} ({name}), owner {by}"),
                now as u64,
            )
            .await?;
            Ok(Org {
                id: id.clone(),
                slug: slug.to_string(),
                name: name.clone(),
                created_at: now,
            })
        })
    }

    /// The organisation named `slug`, if there is one.
    pub async fn by_slug(&self, slug: &str) -> BridgeResult<Option<Org>> {
        if !valid_name(slug) {
            return Ok(None);
        }
        let mut connection = self.store.read().await?;
        let row = sqlx::query(
            "SELECT id, slug, name, created_at FROM tenancy_tenants
             WHERE slug = $1 AND kind = 'organisation'",
        )
        .bind(slug)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::store::store_error)?;
        row.map(|row| org_from(&row))
            .transpose()
            .map_err(crate::store::store_error)
    }

    /// The organisation whose tenant is `id`, if it is one.
    pub async fn by_id(&self, id: &str) -> BridgeResult<Option<Org>> {
        let mut connection = self.store.read().await?;
        let row = sqlx::query(
            "SELECT id, slug, name, created_at FROM tenancy_tenants
             WHERE id = $1 AND kind = 'organisation'",
        )
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::store::store_error)?;
        row.map(|row| org_from(&row))
            .transpose()
            .map_err(crate::store::store_error)
    }

    /// The organisations `subject` belongs to, with its role in each, by slug.
    pub async fn of_subject(&self, subject: &str) -> BridgeResult<Vec<(Org, Role)>> {
        let mut connection = self.store.read().await?;
        let sql = match self.store.backend() {
            Backend::Postgres => "SELECT tenant_id, role FROM devagotchi.orgs_of($1)",
            Backend::Sqlite => {
                "SELECT tenant_id, role FROM org_members WHERE subject = $1 ORDER BY tenant_id"
            }
        };
        let rows: Vec<(String, String)> = sqlx::query_as(sql)
            .bind(subject)
            .fetch_all(&mut *connection)
            .await
            .map_err(crate::store::store_error)?;
        drop(connection);
        let mut out = Vec::with_capacity(rows.len());
        for (tenant, role) in rows {
            let Some(role) = Role::parse(&role) else {
                continue;
            };
            if let Some(org) = self.by_id(&tenant).await? {
                out.push((org, role));
            }
        }
        out.sort_by(|a, b| a.0.slug.cmp(&b.0.slug));
        Ok(out)
    }

    /// `subject`'s role in `org`, if it is a member.
    pub async fn role_in(&self, org: &Org, subject: &str) -> BridgeResult<Option<Role>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            org_role(connection, &org.id, subject).await
        })
    }

    // ---- members -------------------------------------------------------------------

    pub async fn members(&self, org: &Org) -> BridgeResult<Vec<Member>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
                "SELECT subject, role, added_by, added_at FROM org_members
                 WHERE tenant_id = $1 ORDER BY subject",
            )
            .bind(&org.id)
            .fetch_all(&mut *connection)
            .await?;
            rows.into_iter()
                .map(|(subject, role, added_by, added_at)| {
                    Ok(Member {
                        subject,
                        role: role_of(role)?,
                        added_by,
                        added_at,
                    })
                })
                .collect()
        })
    }

    /// Make `subject` a member of `org` with `role`, or change its role there.
    pub async fn set_member(
        &self,
        org: &Org,
        by: &str,
        subject: &str,
        role: Role,
    ) -> BridgeResult<()> {
        check_subject(subject)?;
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let caller = admin_of(connection, org, by).await?;
            let current = org_role(connection, &org.id, subject).await?;
            if (role == Role::Owner || current == Some(Role::Owner)) && caller != Role::Owner {
                return Err(refused(
                    "not_permitted",
                    "only an owner makes or unmakes an owner",
                ));
            }
            if role > caller {
                return Err(refused(
                    "not_permitted",
                    format!(
                        "you are {}; you cannot make anyone {}",
                        caller.as_str(),
                        role.as_str()
                    ),
                ));
            }
            if current == Some(Role::Owner) && role != Role::Owner {
                keep_an_owner(connection, org).await?;
            }
            sqlx::query(
                "INSERT INTO org_members (tenant_id, subject, role, added_by, added_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (tenant_id, subject)
                 DO UPDATE SET role = excluded.role, added_by = excluded.added_by,
                               added_at = excluded.added_at",
            )
            .bind(&org.id)
            .bind(subject)
            .bind(role.as_str())
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            let detail = match current {
                Some(was) => format!("{subject}: {} to {}", was.as_str(), role.as_str()),
                None => format!("{subject}: {}", role.as_str()),
            };
            record_in(
                connection,
                &org.id,
                by,
                "org.member_set",
                &detail,
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    /// Take `subject` out of `org`: its membership, its teams there and its rows on the
    /// organisation's projects. A member may leave; an admin removes others; only an owner
    /// removes an owner; the last owner stays.
    pub async fn remove_member(&self, org: &Org, by: &str, subject: &str) -> BridgeResult<()> {
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let current = org_role(connection, &org.id, subject).await?;
            if by != subject {
                let caller = admin_of(connection, org, by).await?;
                if current == Some(Role::Owner) && caller != Role::Owner {
                    return Err(refused("not_permitted", "only an owner removes an owner"));
                }
            }
            let Some(current) = current else {
                return Err(refused(
                    "not_found",
                    format!("{subject} is not a member of {}", org.slug),
                ));
            };
            if current == Role::Owner {
                keep_an_owner(connection, org).await?;
            }
            for sql in [
                "DELETE FROM org_members WHERE tenant_id = $1 AND subject = $2",
                "DELETE FROM org_team_members WHERE tenant_id = $1 AND subject = $2",
                "DELETE FROM project_members
                  WHERE tenant_id = $1 AND principal_kind = 'subject' AND principal = $2",
            ] {
                sqlx::query(sql)
                    .bind(&org.id)
                    .bind(subject)
                    .execute(&mut *connection)
                    .await?;
            }
            record_in(
                connection,
                &org.id,
                by,
                "org.member_removed",
                &format!("{subject} (was {})", current.as_str()),
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    // ---- teams ---------------------------------------------------------------------

    pub async fn teams(&self, org: &Org) -> BridgeResult<Vec<Team>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let teams: Vec<(String, String)> = sqlx::query_as(
                "SELECT team, name FROM org_teams WHERE tenant_id = $1 ORDER BY team",
            )
            .bind(&org.id)
            .fetch_all(&mut *connection)
            .await?;
            let mut out = Vec::with_capacity(teams.len());
            for (team, name) in teams {
                let members: Vec<String> = sqlx::query_scalar(
                    "SELECT subject FROM org_team_members
                     WHERE tenant_id = $1 AND team = $2 ORDER BY subject",
                )
                .bind(&org.id)
                .bind(&team)
                .fetch_all(&mut *connection)
                .await?;
                out.push(Team {
                    team,
                    name,
                    members,
                });
            }
            Ok(out)
        })
    }

    pub async fn create_team(
        &self,
        org: &Org,
        by: &str,
        team: &str,
        name: &str,
    ) -> BridgeResult<()> {
        check_name("team", team)?;
        let name = check_title(name)?;
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            admin_of(connection, org, by).await?;
            let made = sqlx::query(
                "INSERT INTO org_teams (tenant_id, team, name, created_by, created_at)
                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT (tenant_id, team) DO NOTHING",
            )
            .bind(&org.id)
            .bind(team)
            .bind(&name)
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if made == 0 {
                return Err(refused(
                    "name_taken",
                    format!("{} already has a team {team:?}", org.slug),
                ));
            }
            record_in(
                connection,
                &org.id,
                by,
                "org.team_created",
                team,
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    /// Remove a team, who is on it, and what it was given on the organisation's projects.
    pub async fn delete_team(&self, org: &Org, by: &str, team: &str) -> BridgeResult<()> {
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            admin_of(connection, org, by).await?;
            sqlx::query(
                "DELETE FROM project_members
                  WHERE tenant_id = $1 AND principal_kind = 'team' AND principal = $2",
            )
            .bind(&org.id)
            .bind(team)
            .execute(&mut *connection)
            .await?;
            sqlx::query("DELETE FROM org_team_members WHERE tenant_id = $1 AND team = $2")
                .bind(&org.id)
                .bind(team)
                .execute(&mut *connection)
                .await?;
            let gone = sqlx::query("DELETE FROM org_teams WHERE tenant_id = $1 AND team = $2")
                .bind(&org.id)
                .bind(team)
                .execute(&mut *connection)
                .await?
                .rows_affected();
            if gone == 0 {
                return Err(refused(
                    "not_found",
                    format!("{} has no team {team:?}", org.slug),
                ));
            }
            record_in(
                connection,
                &org.id,
                by,
                "org.team_deleted",
                team,
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    /// Put a member of the organisation on one of its teams (`on`), or take them off.
    pub async fn set_team_member(
        &self,
        org: &Org,
        by: &str,
        team: &str,
        subject: &str,
        on: bool,
    ) -> BridgeResult<()> {
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            admin_of(connection, org, by).await?;
            let exists = sqlx::query("SELECT 1 FROM org_teams WHERE tenant_id = $1 AND team = $2")
                .bind(&org.id)
                .bind(team)
                .fetch_optional(&mut *connection)
                .await?
                .is_some();
            if !exists {
                return Err(refused(
                    "not_found",
                    format!("{} has no team {team:?}", org.slug),
                ));
            }
            if on {
                if org_role(connection, &org.id, subject).await?.is_none() {
                    return Err(refused(
                        "not_found",
                        format!("{subject} is not a member of {}; add them first", org.slug),
                    ));
                }
                sqlx::query(
                    "INSERT INTO org_team_members (tenant_id, team, subject, added_by, added_at)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (tenant_id, team, subject) DO NOTHING",
                )
                .bind(&org.id)
                .bind(team)
                .bind(subject)
                .bind(by)
                .bind(now)
                .execute(&mut *connection)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM org_team_members WHERE tenant_id = $1 AND team = $2 AND subject = $3",
                )
                .bind(&org.id)
                .bind(team)
                .bind(subject)
                .execute(&mut *connection)
                .await?;
            }
            record_in(
                connection,
                &org.id,
                by,
                if on {
                    "org.team_member_added"
                } else {
                    "org.team_member_removed"
                },
                &format!("{team}: {subject}"),
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    // ---- projects ------------------------------------------------------------------

    /// Record a project of `org` (made on the board side already) with its visibility.
    /// A member of the organisation may make one. Saying it again changes nothing.
    pub async fn register_project(
        &self,
        org: &Org,
        by: &str,
        project: &str,
        visibility: Visibility,
    ) -> BridgeResult<()> {
        check_name("project", project)?;
        let owner = owner_of(&org.id);
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let caller = org_role(connection, &org.id, by).await?;
            if !caller.is_some_and(Role::is_person) {
                return Err(refused(
                    "not_permitted",
                    format!(
                        "a person who is a member of {} makes its projects",
                        org.slug
                    ),
                ));
            }
            let made = sqlx::query(
                "INSERT INTO project_access (tenant_id, owner_id, project, visibility, created_by, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (tenant_id, owner_id, project) DO NOTHING",
            )
            .bind(&org.id)
            .bind(&owner)
            .bind(project)
            .bind(visibility.as_str())
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if made == 1 {
                record_in(
                    connection,
                    &org.id,
                    by,
                    "org.project_created",
                    &format!("{project} ({})", visibility.as_str()),
                    now as u64,
                )
                .await?;
            }
            Ok(())
        })
    }

    /// The organisation's projects among `names` (the boards it holds), with their
    /// visibility and own members.
    pub async fn projects(&self, org: &Org, names: &[String]) -> BridgeResult<Vec<Project>> {
        let owner = owner_of(&org.id);
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let mut out = Vec::with_capacity(names.len());
            for project in names {
                let visibility = visibility_of(connection, &org.id, &owner, project).await?;
                let members = members_of(connection, &org.id, &owner, project).await?;
                out.push(Project {
                    project: project.clone(),
                    visibility,
                    members,
                });
            }
            Ok(out)
        })
    }

    /// Change who reaches a project through its organisation.
    pub async fn set_visibility(
        &self,
        org: &Org,
        by: &str,
        project: &str,
        visibility: Visibility,
    ) -> BridgeResult<()> {
        let owner = owner_of(&org.id);
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let caller = effective(connection, &org.id, &owner, project, by).await?;
            if !caller.is_some_and(Role::may_configure) {
                return Err(refused(
                    "not_permitted",
                    format!("an admin of {}/{project} changes who reaches it", org.slug),
                ));
            }
            sqlx::query(
                "INSERT INTO project_access (tenant_id, owner_id, project, visibility, created_by, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (tenant_id, owner_id, project) DO UPDATE SET visibility = excluded.visibility",
            )
            .bind(&org.id)
            .bind(&owner)
            .bind(project)
            .bind(visibility.as_str())
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            record_in(
                connection,
                &org.id,
                by,
                "org.project_visibility",
                &format!("{project}: {}", visibility.as_str()),
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    /// Give a subject or a team `role` on a project, or change it. `floor` is the caller's
    /// role from outside the store (a configured board's grant, `HubConfig`), counted with
    /// what the store holds for it.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_project_member(
        &self,
        at: &ProjectRef,
        by: &str,
        floor: Option<Role>,
        kind: Principal,
        principal: &str,
        role: Role,
    ) -> BridgeResult<()> {
        if role == Role::Owner {
            return Err(invalid(
                "a project's members are admins, members, agents or viewers; owners are the organisation's",
            ));
        }
        match kind {
            Principal::Subject => check_subject(principal)?,
            Principal::Team => check_name("team", principal)?,
        }
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &at.tenant).await?;
            let caller = project_admin(connection, at, by, floor).await?;
            if role > caller {
                return Err(refused(
                    "not_permitted",
                    format!(
                        "you are {} on {}; you cannot make anyone {}",
                        caller.as_str(),
                        at.project,
                        role.as_str()
                    ),
                ));
            }
            if kind == Principal::Team {
                let exists =
                    sqlx::query("SELECT 1 FROM org_teams WHERE tenant_id = $1 AND team = $2")
                        .bind(&at.tenant)
                        .bind(principal)
                        .fetch_optional(&mut *connection)
                        .await?
                        .is_some();
                if !exists {
                    return Err(refused(
                        "not_found",
                        format!("there is no team {principal:?} here"),
                    ));
                }
            }
            sqlx::query(
                "INSERT INTO project_members
                   (tenant_id, owner_id, project, principal_kind, principal, role, added_by, added_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (tenant_id, owner_id, project, principal_kind, principal)
                 DO UPDATE SET role = excluded.role, added_by = excluded.added_by,
                               added_at = excluded.added_at",
            )
            .bind(&at.tenant)
            .bind(&at.owner)
            .bind(&at.project)
            .bind(kind.as_str())
            .bind(principal)
            .bind(role.as_str())
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            record_in(
                connection,
                &at.tenant,
                by,
                "project.member_set",
                &format!(
                    "{}/{}: {} {principal}: {}",
                    at.owner,
                    at.project,
                    kind.as_str(),
                    role.as_str()
                ),
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    pub async fn remove_project_member(
        &self,
        at: &ProjectRef,
        by: &str,
        floor: Option<Role>,
        kind: Principal,
        principal: &str,
    ) -> BridgeResult<()> {
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &at.tenant).await?;
            if !(kind == Principal::Subject && principal == by) {
                project_admin(connection, at, by, floor).await?;
            }
            let gone = sqlx::query(
                "DELETE FROM project_members WHERE tenant_id = $1 AND owner_id = $2 AND project = $3
                   AND principal_kind = $4 AND principal = $5",
            )
            .bind(&at.tenant)
            .bind(&at.owner)
            .bind(&at.project)
            .bind(kind.as_str())
            .bind(principal)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if gone == 0 {
                return Err(refused(
                    "not_found",
                    format!(
                        "{} {principal} holds no role of its own on {}",
                        kind.as_str(),
                        at.project
                    ),
                ));
            }
            record_in(
                connection,
                &at.tenant,
                by,
                "project.member_removed",
                &format!("{}/{}: {} {principal}", at.owner, at.project, kind.as_str()),
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    pub async fn project_members(&self, at: &ProjectRef) -> BridgeResult<Vec<ProjectMember>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &at.tenant).await?;
            members_of(connection, &at.tenant, &at.owner, &at.project).await
        })
    }

    /// `subject`'s role on a project from the store: through the organisation that owns it
    /// (when it is an organisation's) and through the project's own rows; the higher one.
    pub async fn project_role(&self, at: &ProjectRef, subject: &str) -> BridgeResult<Option<Role>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &at.tenant).await?;
            effective(connection, &at.tenant, &at.owner, &at.project, subject).await
        })
    }

    /// The projects among `names` of the organisation `org` that `subject` reaches, with
    /// its role on each.
    pub async fn visible(
        &self,
        org: &Org,
        names: &[String],
        subject: &str,
    ) -> BridgeResult<Vec<(String, Role)>> {
        let owner = owner_of(&org.id);
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let mut out = Vec::new();
            for project in names {
                if let Some(role) = effective(connection, &org.id, &owner, project, subject).await?
                {
                    out.push((project.clone(), role));
                }
            }
            Ok(out)
        })
    }

    /// The projects `subject` was given by name or through a team, across tenants:
    /// `(tenant, owner, project, role)`, the higher role where both give one.
    pub async fn projects_of(&self, subject: &str) -> BridgeResult<Vec<(ProjectRef, Role)>> {
        let mut connection = self.store.read().await?;
        let sql = match self.store.backend() {
            Backend::Postgres => {
                "SELECT tenant_id, owner_id, project, role FROM devagotchi.projects_of($1)"
            }
            Backend::Sqlite => {
                "SELECT p.tenant_id, p.owner_id, p.project, p.role FROM project_members p
                  WHERE p.principal_kind = 'subject' AND p.principal = $1
                 UNION ALL
                 SELECT p.tenant_id, p.owner_id, p.project, p.role FROM project_members p
                   JOIN org_team_members t
                     ON t.tenant_id = p.tenant_id AND t.team = p.principal AND t.subject = $1
                  WHERE p.principal_kind = 'team'
                 ORDER BY 1, 2, 3"
            }
        };
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(sql)
            .bind(subject)
            .fetch_all(&mut *connection)
            .await
            .map_err(crate::store::store_error)?;
        let mut out: Vec<(ProjectRef, Role)> = Vec::new();
        for (tenant, owner, project, role) in rows {
            let Some(role) = Role::parse(&role) else {
                continue;
            };
            let at = ProjectRef {
                tenant,
                owner,
                project,
            };
            match out.iter_mut().find(|(seen, _)| *seen == at) {
                Some((_, held)) => *held = (*held).max(role),
                None => out.push((at, role)),
            }
        }
        Ok(out)
    }

    /// Who acts as a person on a project (role member or above), by subject: whom its
    /// approvals go to.
    pub async fn people(&self, at: &ProjectRef) -> BridgeResult<Vec<String>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &at.tenant).await?;
            let mut candidates: Vec<String> = Vec::new();
            if tenant_of_owner(&at.owner) == Some(at.tenant.as_str()) {
                candidates.extend(
                    sqlx::query_scalar::<_, String>(
                        "SELECT subject FROM org_members WHERE tenant_id = $1",
                    )
                    .bind(&at.tenant)
                    .fetch_all(&mut *connection)
                    .await?,
                );
            }
            for member in members_of(connection, &at.tenant, &at.owner, &at.project).await? {
                match member.kind {
                    Principal::Subject => candidates.push(member.principal),
                    Principal::Team => candidates.extend(
                        sqlx::query_scalar::<_, String>(
                            "SELECT subject FROM org_team_members WHERE tenant_id = $1 AND team = $2",
                        )
                        .bind(&at.tenant)
                        .bind(&member.principal)
                        .fetch_all(&mut *connection)
                        .await?,
                    ),
                }
            }
            candidates.sort();
            candidates.dedup();
            let mut people = Vec::new();
            for subject in candidates {
                if effective(connection, &at.tenant, &at.owner, &at.project, &subject)
                    .await?
                    .is_some_and(Role::is_person)
                {
                    people.push(subject);
                }
            }
            Ok(people)
        })
    }

    // ---- invites -------------------------------------------------------------------

    /// A one-time link's token that makes whoever accepts it a member of `org` with `role`.
    pub async fn invite(
        &self,
        org: &Org,
        by: &str,
        role: Role,
        note: Option<&str>,
    ) -> BridgeResult<NewInvite> {
        let note = note
            .map(check_title)
            .transpose()?
            .filter(|note| !note.is_empty());
        let token = format!(
            "inv_{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(crate::link::random::<32>())
        );
        let hash = token_hash(&token);
        let now = self.now();
        let expires_at = now + INVITE_TTL_SECS;
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let caller = admin_of(connection, org, by).await?;
            if role > caller || (role == Role::Owner && caller != Role::Owner) {
                return Err(refused(
                    "not_permitted",
                    format!(
                        "you are {}; you cannot invite anyone as {}",
                        caller.as_str(),
                        role.as_str()
                    ),
                ));
            }
            sqlx::query(
                "INSERT INTO org_invites (tenant_id, hash, role, note, invited_by, created_at, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(&org.id)
            .bind(&hash)
            .bind(role.as_str())
            .bind(note.as_deref())
            .bind(by)
            .bind(now)
            .bind(expires_at)
            .execute(&mut *connection)
            .await?;
            record_in(
                connection,
                &org.id,
                by,
                "org.invite_created",
                &format!("{} as {}", &hash[..12], role.as_str()),
                now as u64,
            )
            .await?;
            Ok(())
        })?;
        Ok(NewInvite {
            id: hash[..12].to_string(),
            token,
            role,
            expires_at,
        })
    }

    pub async fn invites(&self, org: &Org, by: &str) -> BridgeResult<Vec<Invite>> {
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            admin_of(connection, org, by).await?;
            let rows: Vec<(String, String, Option<String>, String, i64)> = sqlx::query_as(
                "SELECT hash, role, note, invited_by, expires_at FROM org_invites
                 WHERE tenant_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL
                   AND expires_at > $2
                 ORDER BY created_at, hash",
            )
            .bind(&org.id)
            .bind(now)
            .fetch_all(&mut *connection)
            .await?;
            rows.into_iter()
                .map(|(hash, role, note, invited_by, expires_at)| {
                    Ok(Invite {
                        id: hash[..12].to_string(),
                        role: role_of(role)?,
                        note,
                        invited_by,
                        expires_at,
                    })
                })
                .collect()
        })
    }

    pub async fn revoke_invite(&self, org: &Org, by: &str, id: &str) -> BridgeResult<()> {
        if id.len() != 12 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(
                "an invite id is the 12 hex digits its listing shows",
            ));
        }
        let now = self.now();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            admin_of(connection, org, by).await?;
            let gone = sqlx::query(
                "UPDATE org_invites SET revoked_at = $3
                  WHERE tenant_id = $1 AND substr(hash, 1, 12) = $2
                    AND accepted_at IS NULL AND revoked_at IS NULL",
            )
            .bind(&org.id)
            .bind(id)
            .bind(now)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if gone == 0 {
                return Err(refused(
                    "not_found",
                    format!("no open invite {id} on {}", org.slug),
                ));
            }
            record_in(
                connection,
                &org.id,
                by,
                "org.invite_revoked",
                id,
                now as u64,
            )
            .await?;
            Ok(())
        })
    }

    /// `subject` takes the invite whose token is `token`: it becomes a member with the
    /// invite's role (or keeps a higher one it has). An invite works once.
    pub async fn accept(&self, token: &str, subject: &str) -> BridgeResult<(Org, Role)> {
        check_subject(subject)?;
        let hash = token_hash(token.trim());
        let tenant: Option<String> = {
            let mut connection = self.store.read().await?;
            let sql = match self.store.backend() {
                Backend::Postgres => "SELECT devagotchi.invite_tenant($1)",
                Backend::Sqlite => "SELECT tenant_id FROM org_invites WHERE hash = $1",
            };
            sqlx::query_scalar::<_, Option<String>>(sql)
                .bind(&hash)
                .fetch_optional(&mut *connection)
                .await
                .map_err(crate::store::store_error)?
                .flatten()
        };
        let not_found = || {
            BridgeError::new(
                "not_found",
                "this invite link is not one this hub made; ask for a new one",
            )
        };
        let tenant = tenant.ok_or_else(not_found)?;
        let org = self.by_id(&tenant).await?.ok_or_else(not_found)?;
        let now = self.now();
        let role = transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            let row: Option<(String, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
                "SELECT role, expires_at, accepted_at, revoked_at FROM org_invites
                 WHERE tenant_id = $1 AND hash = $2",
            )
            .bind(&org.id)
            .bind(&hash)
            .fetch_optional(&mut *connection)
            .await?;
            let Some((role, expires_at, accepted_at, revoked_at)) = row else {
                return Err(Failed::Refused(not_found()));
            };
            if accepted_at.is_some() {
                return Err(refused(
                    "spent",
                    "this invite was used already; ask for a new one",
                ));
            }
            if revoked_at.is_some() {
                return Err(refused("revoked", "this invite was withdrawn"));
            }
            if expires_at <= now {
                return Err(refused("expired", "this invite ran out; ask for a new one"));
            }
            let invited = role_of(role)?;
            let held = org_role(connection, &org.id, subject).await?;
            let role = held.map_or(invited, |held| held.max(invited));
            sqlx::query(
                "INSERT INTO org_members (tenant_id, subject, role, added_by, added_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (tenant_id, subject) DO UPDATE SET role = excluded.role",
            )
            .bind(&org.id)
            .bind(subject)
            .bind(role.as_str())
            .bind(format!("invite {}", &hash[..12]))
            .bind(now)
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "UPDATE org_invites SET accepted_by = $3, accepted_at = $4
                 WHERE tenant_id = $1 AND hash = $2",
            )
            .bind(&org.id)
            .bind(&hash)
            .bind(subject)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            record_in(
                connection,
                &org.id,
                subject,
                "org.invite_accepted",
                &format!("{} as {}", &hash[..12], role.as_str()),
                now as u64,
            )
            .await?;
            Ok(role)
        })?;
        Ok((org, role))
    }

    /// The home tenant's id: where configured boards (`@shared`) keep their members.
    pub async fn home_tenant(&self) -> BridgeResult<Option<String>> {
        let mut connection = self.store.read().await?;
        sqlx::query_scalar("SELECT id FROM tenancy_tenants WHERE home = 1")
            .fetch_optional(&mut *connection)
            .await
            .map_err(crate::store::store_error)
    }
}

fn org_from(row: &sqlx::any::AnyRow) -> Result<Org, sqlx::Error> {
    Ok(Org {
        id: row.try_get("id")?,
        slug: row
            .try_get::<Option<String>, _>("slug")?
            .unwrap_or_default(),
        name: row
            .try_get::<Option<String>, _>("name")?
            .unwrap_or_default(),
        created_at: row.try_get("created_at")?,
    })
}

async fn org_role(
    connection: &mut AnyConnection,
    tenant: &str,
    subject: &str,
) -> Result<Option<Role>, Failed> {
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM org_members WHERE tenant_id = $1 AND subject = $2")
            .bind(tenant)
            .bind(subject)
            .fetch_optional(&mut *connection)
            .await?;
    role.map(role_of).transpose()
}

/// The caller's role when it is an admin or an owner of `org`; refused otherwise.
async fn admin_of(connection: &mut AnyConnection, org: &Org, by: &str) -> Result<Role, Failed> {
    match org_role(connection, &org.id, by).await? {
        Some(role) if role.may_configure() => Ok(role),
        Some(role) => Err(refused(
            "not_permitted",
            format!(
                "you are {} of {}; an admin or an owner does this",
                role.as_str(),
                org.slug
            ),
        )),
        None => Err(refused(
            "not_found",
            format!("you are not a member of {}", org.slug),
        )),
    }
}

/// Refuses when `org` has one owner left.
async fn keep_an_owner(connection: &mut AnyConnection, org: &Org) -> Result<(), Failed> {
    let owners: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM org_members WHERE tenant_id = $1 AND role = 'owner'",
    )
    .bind(&org.id)
    .fetch_one(&mut *connection)
    .await?;
    if owners <= 1 {
        return Err(refused(
            "last_owner",
            format!(
                "{} keeps at least one owner; make someone else owner first",
                org.slug
            ),
        ));
    }
    Ok(())
}

async fn visibility_of(
    connection: &mut AnyConnection,
    tenant: &str,
    owner: &str,
    project: &str,
) -> Result<Visibility, Failed> {
    let visibility: Option<String> = sqlx::query_scalar(
        "SELECT visibility FROM project_access WHERE tenant_id = $1 AND owner_id = $2 AND project = $3",
    )
    .bind(tenant)
    .bind(owner)
    .bind(project)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(visibility
        .as_deref()
        .and_then(Visibility::parse)
        .unwrap_or(Visibility::Org))
}

async fn members_of(
    connection: &mut AnyConnection,
    tenant: &str,
    owner: &str,
    project: &str,
) -> Result<Vec<ProjectMember>, Failed> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT principal_kind, principal, role FROM project_members
         WHERE tenant_id = $1 AND owner_id = $2 AND project = $3
         ORDER BY principal_kind, principal",
    )
    .bind(tenant)
    .bind(owner)
    .bind(project)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|(kind, principal, role)| {
            Ok(ProjectMember {
                kind: Principal::parse(&kind).ok_or_else(|| {
                    refused("board_store", format!("unknown principal kind {kind:?}"))
                })?,
                principal,
                role: role_of(role)?,
            })
        })
        .collect()
}

/// `subject`'s role on a project, in a transaction scoped to its tenant: the organisation's
/// role when the organisation owns the project and the project is visible to it (always for
/// admins and owners), the project's own rows for the subject and its teams; the higher.
async fn effective(
    connection: &mut AnyConnection,
    tenant: &str,
    owner: &str,
    project: &str,
    subject: &str,
) -> Result<Option<Role>, Failed> {
    let mut best: Option<Role> = None;
    if tenant_of_owner(owner) == Some(tenant)
        && let Some(role) = org_role(connection, tenant, subject).await?
    {
        let reaches = role.may_configure()
            || visibility_of(connection, tenant, owner, project).await? == Visibility::Org;
        if reaches {
            best = Some(role);
        }
    }
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT p.role FROM project_members p
          WHERE p.tenant_id = $1 AND p.owner_id = $2 AND p.project = $3
            AND ((p.principal_kind = 'subject' AND p.principal = $4)
              OR (p.principal_kind = 'team' AND p.principal IN
                    (SELECT t.team FROM org_team_members t
                      WHERE t.tenant_id = $1 AND t.subject = $4)))",
    )
    .bind(tenant)
    .bind(owner)
    .bind(project)
    .bind(subject)
    .fetch_all(&mut *connection)
    .await?;
    for role in rows {
        let role = role_of(role)?;
        best = Some(best.map_or(role, |held| held.max(role)));
    }
    Ok(best)
}

/// The caller's role on a project when it may manage who works it; refused otherwise.
async fn project_admin(
    connection: &mut AnyConnection,
    at: &ProjectRef,
    by: &str,
    floor: Option<Role>,
) -> Result<Role, Failed> {
    let held = effective(connection, &at.tenant, &at.owner, &at.project, by).await?;
    let role = match (held, floor) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    match role {
        Some(role) if role.may_configure() => Ok(role),
        _ => Err(refused(
            "not_permitted",
            format!("an admin of {} manages who works it", at.project),
        )),
    }
}
