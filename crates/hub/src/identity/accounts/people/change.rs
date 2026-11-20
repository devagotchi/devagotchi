use super::*;

impl Accounts {
    pub async fn update_local_user(
        &self,
        actor: &str,
        target: &str,
        expected: i64,
        name: &str,
        enabled: bool,
    ) -> BridgeResult<LocalUser> {
        let name = check_name(target, name)?;
        let now = now_secs();
        transaction!(&self.store, |connection| {
            let owner = self.local_owner(connection, actor).await?;
            let current = managed_in(connection, &owner.tenant, target).await?;
            version(&current, expected)?;
            if current.account.owner && !enabled {
                return Err(refused(
                    "owner_protected",
                    "the environment's owner cannot be disabled",
                )
                .into());
            }
            sqlx::query("UPDATE auth_accounts SET name=$1, disabled_at=$2, management_version=management_version+1 WHERE id=$3 AND tenant_id=$4")
                .bind(name).bind(if enabled { None } else { Some(secs(current.account.disabled_at.unwrap_or(now))) })
                .bind(target).bind(&owner.tenant).execute(&mut *connection).await?;
            if !enabled {
                end_access(connection, target).await?;
                sqlx::query("UPDATE auth_api_keys SET revoked_at=$1 WHERE account=$2 AND revoked_at IS NULL")
                    .bind(secs(now)).bind(target).execute(&mut *connection).await?;
            }
            let detail = serde_json::json!({"id":target,"enabled":enabled}).to_string();
            tenants::record_in(
                connection,
                &owner.tenant,
                actor,
                "admin.user.updated",
                &detail,
                now,
            )
            .await?;
            managed_in(connection, &owner.tenant, target).await
        })
    }

    pub async fn reset_local_password(
        &self,
        actor: &str,
        target: &str,
        expected: i64,
        password: &str,
    ) -> BridgeResult<LocalUser> {
        self.checked(password)?;
        let hash = passwords::hash_of(password).await?;
        let now = now_secs();
        transaction!(&self.store, |connection| {
            let owner = self.local_owner(connection, actor).await?;
            let current = managed_in(connection, &owner.tenant, target).await?;
            version(&current, expected)?;
            if current.email.is_none() {
                return Err(refused("no_password", "this user has no local password login").into());
            }
            sqlx::query("UPDATE auth_passwords SET hash=$1,updated_at=$2 WHERE account_id=$3")
                .bind(&hash)
                .bind(secs(now))
                .bind(target)
                .execute(&mut *connection)
                .await?;
            sqlx::query(
                "UPDATE auth_accounts SET management_version=management_version+1 WHERE id=$1",
            )
            .bind(target)
            .execute(&mut *connection)
            .await?;
            end_access(connection, target).await?;
            tenants::record_in(
                connection,
                &owner.tenant,
                actor,
                "admin.user.password_reset",
                target,
                now,
            )
            .await?;
            managed_in(connection, &owner.tenant, target).await
        })
    }
}

/// End sessions and outstanding sign-in grants together. Passwords/passkeys remain usable
/// for a fresh login when enabled; no old session or token is revived by re-enabling.
async fn end_access(connection: &mut AnyConnection, account: &str) -> Result<(), sqlx::Error> {
    for query in [
        "DELETE FROM auth_sessions WHERE account=$1",
        "DELETE FROM oauth_tokens WHERE account=$1",
        "DELETE FROM oauth_codes WHERE account=$1",
        "DELETE FROM oauth_devices WHERE account=$1",
        "DELETE FROM auth_links WHERE account=$1",
    ] {
        sqlx::query(query)
            .bind(account)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}
