//! An organisation's members: who they are, adding or changing one within the caller's own rank, and removing one while an owner stays.

use devagotchi_contracts::BridgeResult;
use sqlx::AnyConnection;

use super::{
    Member, Org, Orgs, Role,
    checks::{check_subject, refused},
    roles::{admin_of, org_role, role_of},
};
use crate::{
    identity::accounts::tenants::record_in,
    store::{Failed, transaction},
};

impl Orgs {
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
}

/// Refuses when `org` has one owner left.
pub(super) async fn keep_an_owner(connection: &mut AnyConnection, org: &Org) -> Result<(), Failed> {
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
