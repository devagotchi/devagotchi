//! Who works a project with which role: its own members (a subject or a team), the role a subject holds there, and who may change that.

use devagotchi_contracts::BridgeResult;
use sqlx::AnyConnection;

use super::{
    Orgs, Principal, ProjectMember, ProjectRef, Role, Visibility,
    checks::{check_name, check_subject, invalid, refused},
    projects::visibility_of,
    roles::{org_role, role_of},
    tenant_of_owner,
};
use crate::{
    identity::accounts::tenants::record_in,
    store::{Failed, transaction},
};

impl Orgs {
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
}

pub(super) async fn members_of(
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
pub(super) async fn effective(
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
pub(super) async fn project_admin(
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
