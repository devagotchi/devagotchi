//! Invites into an organisation: making one link that works for a week, listing and revoking them, and accepting one.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use devagotchi_contracts::{BridgeError, BridgeResult};
use sha2::{Digest, Sha256};

use super::{
    INVITE_TTL_SECS, Invite, NewInvite, Org, Orgs, Role,
    checks::{check_subject, check_title, invalid, refused},
    roles::{admin_of, org_role, role_of},
};
use crate::{
    identity::accounts::tenants::record_in,
    store::{Failed, transaction},
};

impl Orgs {
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
            URL_SAFE_NO_PAD.encode(crate::machines::random::<32>())
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
            let sql = "SELECT devagotchi.invite_tenant($1)";
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
}

/// The SHA-256 of an invite token, as stored.
pub(super) fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
