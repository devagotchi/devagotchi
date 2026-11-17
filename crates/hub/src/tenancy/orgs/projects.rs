//! An organisation's projects: registering one, which it has, who may see each (`Visibility`), the projects a subject works and the people on one.

use devagotchi_contracts::BridgeResult;
use sqlx::AnyConnection;

use super::{
    Org, Orgs, Principal, Project, ProjectRef, Role, Visibility,
    checks::{check_name, refused},
    owner_of,
    project_roles::{effective, members_of},
    roles::org_role,
    tenant_of_owner,
};
use crate::{
    identity::accounts::tenants::record_in,
    store::{Failed, transaction},
};

impl Orgs {
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
        let sql = "SELECT tenant_id, owner_id, project, role FROM devagotchi.projects_of($1)";
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
}

pub(super) async fn visibility_of(
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
