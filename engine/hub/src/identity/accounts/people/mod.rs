//! Owns local-user administration over existing auth_accounts and credentials.
//! Serves the single-environment owner's Local users page. Settings: password minimum
//! from Hub settings; tenancy mode is deploy-time. Depends on accounts/passwords/tenants.
use super::*;
use crate::config::TenancyMode;

mod change;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct LocalUser {
    pub account: Account,
    pub email: Option<String>,
    pub version: i64,
}

fn refused(code: &str, message: &str) -> BridgeError {
    BridgeError::new(code, message)
}

impl Accounts {
    /// Require the actual, active owner in this one environment; lock the administration
    /// authority for a mutation's transaction. A page guard alone is not authority here.
    async fn local_owner(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
    ) -> Result<Account, Failed> {
        if self.tenancy.mode != TenancyMode::Single {
            return Err(refused(
                "not_available",
                "local users are managed only in a self-hosted environment",
            )
            .into());
        }
        if self
            .enter(connection, Lookup::Account(actor))
            .await?
            .is_none()
        {
            return Err(
                refused("not_granted", "only this environment's owner manages users").into(),
            );
        }
        let allowed: Option<String> = sqlx::query_scalar("SELECT id FROM auth_accounts WHERE id=$1 AND owner=1 AND disabled_at IS NULL FOR UPDATE")
            .bind(actor).fetch_optional(&mut *connection).await?;
        if allowed.is_none() {
            return Err(
                refused("not_granted", "only this environment's owner manages users").into(),
            );
        }
        account_in(connection, actor)
            .await?
            .ok_or_else(|| refused("not_granted", "owner is unavailable").into())
    }

    pub async fn local_users(
        &self,
        actor: &str,
        after: Option<&str>,
    ) -> BridgeResult<Vec<LocalUser>> {
        transaction!(&self.store, |connection| {
            let owner = self.local_owner(connection, actor).await?;
            let rows = sqlx::query(concat!(
                "SELECT ",
                account_columns_a!(),
                ", p.email, a.management_version
                FROM auth_accounts a LEFT JOIN auth_passwords p ON p.account_id=a.id
                WHERE a.tenant_id=$1 AND ($2::text IS NULL OR a.id>$2) ORDER BY a.id LIMIT 101"
            ))
            .bind(&owner.tenant)
            .bind(after)
            .fetch_all(&mut *connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(LocalUser {
                        account: account_row(row)?,
                        email: row.try_get(7)?,
                        version: row.try_get(8)?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Failed::from)
        })
    }

    pub async fn local_user(&self, actor: &str, target: &str) -> BridgeResult<LocalUser> {
        transaction!(&self.store, |connection| {
            let owner = self.local_owner(connection, actor).await?;
            managed_in(connection, &owner.tenant, target).await
        })
    }

    /// The account, its login and password save together, or none of them do.
    pub async fn create_local_user(
        &self,
        actor: &str,
        name: &str,
        email: &str,
        password: &str,
    ) -> BridgeResult<LocalUser> {
        let id = new_account_id(crate::machines::random::<16>());
        let name = check_name(&id, name)?;
        let email = email::normalize_email(email)
            .ok_or_else(|| refused("invalid_email", "not an email address"))?;
        self.checked(password)?;
        let proven = identities::Proven {
            provider: "email".into(),
            subject: email.clone(),
            verified_email: None,
            name: None,
        };
        if self.is_operator_identity(&proven) {
            return Err(refused(
                "email_taken",
                "that address is reserved for the owner",
            ));
        }
        let hash = passwords::hash_of(password).await?;
        let now = now_secs();
        transaction!(&self.store, |connection| {
            let owner = self.local_owner(connection, actor).await?;
            if self
                .store
                .tenant_of(connection, Lookup::Identity("email", &email))
                .await?
                .is_some()
            {
                return Err(refused("email_taken", "that email already has an account").into());
            }
            create_in(
                &self.store,
                connection,
                &id,
                name,
                false,
                &owner.tenant,
                now,
            )
            .await?;
            let account = account_in(connection, &id)
                .await?
                .ok_or_else(|| refused("no_account", "new account is unavailable"))?;
            identities::insert_identity(connection, &account, &proven, self.counting, now).await?;
            sqlx::query("INSERT INTO auth_passwords (account_id,tenant_id,email,hash,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$5)")
                .bind(&id).bind(&owner.tenant).bind(&email).bind(&hash).bind(secs(now)).execute(&mut *connection).await?;
            tenants::record_in(
                connection,
                &owner.tenant,
                actor,
                "admin.user.created",
                &id,
                now,
            )
            .await?;
            managed_in(connection, &owner.tenant, &id).await
        })
    }
}

async fn managed_in(
    connection: &mut AnyConnection,
    tenant: &str,
    id: &str,
) -> Result<LocalUser, Failed> {
    let row = sqlx::query(concat!(
        "SELECT ",
        account_columns_a!(),
        ", p.email, a.management_version
        FROM auth_accounts a LEFT JOIN auth_passwords p ON p.account_id=a.id
        WHERE a.tenant_id=$1 AND a.id=$2 FOR UPDATE OF a"
    ))
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| refused("not_found", "no such user in this environment"))?;
    Ok(LocalUser {
        account: account_row(&row)?,
        email: row.try_get(7)?,
        version: row.try_get(8)?,
    })
}

fn version(current: &LocalUser, expected: i64) -> Result<(), Failed> {
    if current.version != expected {
        Err(refused(
            "version_conflict",
            "this user changed; reload before saving again",
        )
        .into())
    } else {
        Ok(())
    }
}
