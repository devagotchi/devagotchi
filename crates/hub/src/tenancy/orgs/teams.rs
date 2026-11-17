//! An organisation's teams: listing, making and deleting one, and who is on it.

use devagotchi_contracts::BridgeResult;

use super::{
    Org, Orgs, Team,
    checks::{check_name, check_title, refused},
    roles::{admin_of, org_role},
};
use crate::{identity::accounts::tenants::record_in, store::transaction};

impl Orgs {
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
}
